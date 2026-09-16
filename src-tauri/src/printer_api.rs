//! 打印机系统 API 模块
//! 连接/断开网络打印机、本地列表读取、设为默认、打开属性/首选项

use serde::Serialize;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{GetLastError, HANDLE};
use windows::Win32::Graphics::Printing::{
    AddPrinterConnectionW, ClosePrinter, DeletePrinter, DeletePrinterConnectionW, EnumPrintersW,
    GetDefaultPrinterW, OpenPrinterW, SetDefaultPrinterW, PRINTER_ENUM_CONNECTIONS, PRINTER_INFO_2W,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

use crate::config;
use crate::credential;
use crate::credential::wide_ptr_to_string;
use crate::smb_scan;
use crate::utils::win_error_message;

/// 本地已连接打印机信息
#[derive(Debug, Clone, Serialize)]
pub struct LocalPrinterItem {
    /// 打印机名称（UNC 路径）
    pub name: String,
    /// 端口名称
    pub port_name: String,
    /// 驱动名称
    pub driver_name: String,
    /// 是否为默认打印机
    pub is_default: bool,
    /// 设备状态
    pub status: String,
}

/// Tauri 指令：连接网络共享打印机
#[tauri::command]
pub async fn connect_printer(printer_path: String) -> Result<String, String> {
    // 重复连接拦截
    if is_printer_connected(&printer_path) {
        return Err("该打印机已安装，无需重复连接".to_string());
    }

    // 提取打印机所属服务器
    let printer_server = extract_printer_server(&printer_path);
    
    // 检查是否是附加服务器
    let multi_cfg = config::load_multi_server_config();
    let is_extra_server = multi_cfg.extra_servers.iter().any(|s| s.server_addr == printer_server);
    
    unsafe {
        // 如果是附加服务器，先建立 SMB 连接
        if is_extra_server {
            if let Some(server_info) = multi_cfg.extra_servers.iter().find(|s| s.server_addr == printer_server) {
                // 建立 SMB 会话（会重用 Windows 保存的凭据）
                let unc_path = format!("\\\\{}", printer_server);
                let _ = std::process::Command::new("net")
                    .args(["use", &unc_path])
                    .output();
                
                log::info!("已为 {} 建立 SMB 会话", printer_server);
            }
        }
        
        let path_w = HSTRING::from(&printer_path);
        
        // 执行打印机连接
        let result = AddPrinterConnectionW(PCWSTR(path_w.as_ptr()));
        
        if result.as_bool() {
            log::info!("打印机连接成功：{printer_path}");
            Ok(format!("打印机连接成功：{printer_path}"))
        } else {
            let code = GetLastError().0;
            log::error!("打印机连接失败：{printer_path}, 错误码={code}");
            
            Err(match code {
                1797 | 1930 => {
                    "打印机驱动缺失，请先手动安装对应驱动后重试".to_string()
                }
                1801 => {
                    let username = if is_extra_server { 
                        multi_cfg.extra_servers.iter().find(|s| s.server_addr == printer_server).map(|s| s.username.as_str()).unwrap_or("?") 
                    } else { 
                        "(主服务器)" 
                    };
                    format!(
                        "无法连接到打印机 {}\n\n请确认:\n1. 服务器 {} 在线且可访问\n2. SMB 共享服务已启用\n3. 用户 '{}' 有访问权限\n4. 防火墙未阻止 445 端口",
                        printer_path, 
                        printer_server,
                        username
                    )
                },
                1802 => "该打印机已安装，无需重复连接".to_string(),
                1326 => "凭据验证失败，请检查账号密码".to_string(),
                53 | 1203 => format!("打印服务器 {} 网络不通，请检查内网连接", printer_server),
                _ => win_error_message("打印机连接", code),
            })
        }
    }
}

/// 从 UNC 路径提取服务器地址
fn extract_printer_server(printer_path: &str) -> String {
    if printer_path.starts_with("\\\\") {
        let parts: Vec<&str> = printer_path.split('\\').skip(1).collect();
        if !parts.is_empty() {
            return parts[0].to_string();
        }
    }
    printer_path.to_string()
}

/// Tauri 指令：获取本机已安装的打印服务器打印机
#[tauri::command]
pub async fn get_local_printer_list() -> Result<Vec<LocalPrinterItem>, String> {
    let cfg = config::load_config();
    let default_printer = get_default_printer_name().unwrap_or_default();
    let mut items = Vec::new();

    unsafe {
        let mut needed: u32 = 0;
        let mut returned: u32 = 0;

        // 第一次调用获取缓冲区大小
        let _ = EnumPrintersW(
            PRINTER_ENUM_CONNECTIONS,
            PCWSTR::null(),
            2,
            None,
            &mut needed,
            &mut returned,
        );

        if needed == 0 {
            return Ok(items);
        }

        let mut buf: Vec<u8> = vec![0; needed as usize];
        let result = EnumPrintersW(
            PRINTER_ENUM_CONNECTIONS,
            PCWSTR::null(),
            2,
            Some(buf.as_mut_slice()),
            &mut needed,
            &mut returned,
        );
        if result.is_err() && returned == 0 {
            let code = GetLastError().0;
            // 没有连接打印机时可能返回错误，视为空列表
            if code == 1801 || code == 87 {
                return Ok(items);
            }
            return Err(win_error_message("本地打印机枚举", code));
        }

        let infos = std::slice::from_raw_parts(
            buf.as_ptr() as *const PRINTER_INFO_2W,
            returned as usize,
        );

        let server_prefix_lower = format!("\\\\{}", cfg.server_addr).to_lowercase();
        for info in infos {
            let name = wide_ptr_to_string(info.pPrinterName.0);
            // 仅保留目标打印服务器的打印机
            if !name.to_lowercase().starts_with(&server_prefix_lower) {
                continue;
            }
            let port_name = wide_ptr_to_string(info.pPortName.0);
            let driver_name = wide_ptr_to_string(info.pDriverName.0);
            items.push(LocalPrinterItem {
                is_default: name == default_printer,
                name,
                port_name,
                driver_name,
                status: "空闲".to_string(),
            });
        }
    }

    log::info!("本地已连接 {} 台打印服务器打印机", items.len());
    Ok(items)
}

/// Tauri 指令：设置默认打印机
#[tauri::command]
pub async fn set_default_printer(name: String) -> Result<String, String> {
    let name_w = HSTRING::from(&name);
    unsafe {
        if SetDefaultPrinterW(PCWSTR(name_w.as_ptr())).as_bool() {
            log::info!("已设为默认打印机：{name}");
            Ok(format!("已将「{}」设为默认打印机", display_name(&name)))
        } else {
            let code = GetLastError().0;
            log::error!("设置默认打印机失败：{name}, 错误码={code}");
            Err(win_error_message("设置默认打印机", code))
        }
    }
}

/// Tauri 指令：断开/删除网络打印机
#[tauri::command]
pub async fn remove_printer(name: String) -> Result<String, String> {
    let name_w = HSTRING::from(&name);
    unsafe {
        // 首选：网络打印机连接专用断开 API（作用于当前用户连接，无需管理员权限）
        if DeletePrinterConnectionW(PCWSTR(name_w.as_ptr())).as_bool() {
            log::info!("打印机连接已断开：{name}");
            return Ok(format!("已断开打印机：{}", display_name(&name)));
        }
        let conn_code = GetLastError().0;
        log::warn!("DeletePrinterConnectionW 失败（错误码={conn_code}），回退 DeletePrinter");

        // 回退：打开并删除打印机对象（兼容本地打印机）
        let mut handle = HANDLE(std::ptr::null_mut());
        if OpenPrinterW(PCWSTR(name_w.as_ptr()), &mut handle, None).is_err() {
            let code = GetLastError().0;
            return Err(win_error_message("断开打印机", code));
        }
        if DeletePrinter(handle).is_err() {
            let code = GetLastError().0;
            let _ = ClosePrinter(handle);
            return Err(win_error_message("断开打印机", code));
        }
        let _ = ClosePrinter(handle);
    }

    log::info!("打印机已删除：{name}");
    Ok(format!("已断开打印机：{}", display_name(&name)))
}

/// Tauri 指令：打开 Windows 原生打印机属性窗口
#[tauri::command]
pub async fn open_printer_property(name: String) -> Result<String, String> {
    open_printui_dialog("/p", &name)?;
    Ok("已打开打印机属性窗口".to_string())
}

/// Tauri 指令：打开打印首选项配置面板
#[tauri::command]
pub async fn open_printer_preference(name: String) -> Result<String, String> {
    open_printui_dialog("/e", &name)?;
    Ok("已打开打印首选项窗口".to_string())
}

/// Tauri 指令：获取当前默认打印机名称
#[tauri::command]
pub async fn get_default_printer() -> Result<String, String> {
    get_default_printer_name().ok_or_else(|| "未找到默认打印机".to_string())
}

/// 调用 rundll32 printui.dll 打开打印机对话框
fn open_printui_dialog(flag: &str, printer_name: &str) -> Result<(), String> {
    let params = format!("printui.dll,PrintUIEntry {flag} /n \"{printer_name}\"");
    let params_w = HSTRING::from(&params);
    let rundll32 = HSTRING::from("rundll32.exe");
    unsafe {
        let result = ShellExecuteW(
            None,
            PCWSTR::null(), // "open" 默认操作
            PCWSTR(rundll32.as_ptr()),
            PCWSTR(params_w.as_ptr()),
            PCWSTR::null(),
            SW_SHOW,
        );
        // ShellExecuteW 返回值 > 32 表示成功
        if result.0 as isize <= 32 {
            log::error!("打开打印 UI 失败：{flag} {printer_name}, 代码={}", result.0 as isize);
            return Err("无法打开打印机配置窗口".to_string());
        }
    }
    log::info!("打开打印机对话框：{flag} {printer_name}");
    Ok(())
}

/// 检查指定路径的打印机是否已连接
fn is_printer_connected(printer_path: &str) -> bool {
    unsafe {
        let mut needed: u32 = 0;
        let mut returned: u32 = 0;
        let _ = EnumPrintersW(
            PRINTER_ENUM_CONNECTIONS,
            PCWSTR::null(),
            2,
            None,
            &mut needed,
            &mut returned,
        );
        if needed == 0 {
            return false;
        }
        let mut buf: Vec<u8> = vec![0; needed as usize];
        if EnumPrintersW(
            PRINTER_ENUM_CONNECTIONS,
            PCWSTR::null(),
            2,
            Some(buf.as_mut_slice()),
            &mut needed,
            &mut returned,
        )
        .is_err()
            && returned == 0
        {
            return false;
        }
        let infos = std::slice::from_raw_parts(
            buf.as_ptr() as *const PRINTER_INFO_2W,
            returned as usize,
        );
        let target_lower = printer_path.to_lowercase();
        infos.iter().any(|info| {
            wide_ptr_to_string(info.pPrinterName.0).to_lowercase() == target_lower
        })
    }
}

/// 获取系统默认打印机名称
fn get_default_printer_name() -> Option<String> {
    unsafe {
        let mut buf: Vec<u16> = vec![0; 512];
        let mut size = buf.len() as u32;
        if GetDefaultPrinterW(PWSTR(buf.as_mut_ptr()), &mut size).as_bool() {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..len]))
        } else {
            None
        }
    }
}

/// 从 UNC 路径提取显示名称
fn display_name(name: &str) -> String {
    name.rsplit('\\').next().unwrap_or(name).to_string()
}

/// 拼接完整 SMB 打印机路径
#[allow(dead_code)]
pub fn build_printer_path(server: &str, share_name: &str) -> String {
    if share_name.starts_with("\\\\") {
        share_name.to_string()
    } else {
        format!("\\\\{server}\\{share_name}")
    }
}

/// 过滤本地打印机列表，仅保留目标服务器的设备
#[allow(dead_code)]
pub fn filter_server_printers(
    all_names: &[String],
    server: &str,
) -> Vec<String> {
    let prefix = format!("\\\\{server}").to_lowercase();
    all_names
        .iter()
        .filter(|n| n.to_lowercase().starts_with(&prefix))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_printer_path() {
        assert_eq!(
            build_printer_path("10.60.254.90", "HP-M4"),
            "\\\\10.60.254.90\\HP-M4"
        );
        // 已经是完整路径则不重复拼接
        assert_eq!(
            build_printer_path("10.60.254.90", "\\\\10.60.254.90\\HP-M4"),
            "\\\\10.60.254.90\\HP-M4"
        );
    }

    #[test]
    fn test_filter_server_printers() {
        let names = vec![
            "\\\\10.60.254.90\\HP-M4".to_string(),
            "\\\\192.168.1.1\\Other".to_string(),
            "\\\\10.60.254.90\\Canon".to_string(),
            "Microsoft Print to PDF".to_string(),
        ];
        let filtered = filter_server_printers(&names, "10.60.254.90");
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&"\\\\10.60.254.90\\HP-M4".to_string()));
        assert!(filtered.contains(&"\\\\10.60.254.90\\Canon".to_string()));
    }

    #[test]
    fn test_filter_case_insensitive() {
        let names = vec!["\\\\10.60.254.90\\hp-m4".to_string()];
        let filtered = filter_server_printers(&names, "10.60.254.90");
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_display_name() {
        assert_eq!(display_name("\\\\10.60.254.90\\HP-M4"), "HP-M4");
        assert_eq!(display_name("HP-M4"), "HP-M4");
    }

    #[test]
    fn test_extract_printer_server() {
        assert_eq!(extract_printer_server("\\\\127.0.0.1\\IT-Print"), "127.0.0.1");
        assert_eq!(extract_printer_server("\\\\10.60.254.90\\HP-M4"), "10.60.254.90");
    }

    #[test]
    fn test_local_printer_item_serialization() {
        let item = LocalPrinterItem {
            name: "\\\\10.60.254.90\\HP-M4".to_string(),
            port_name: "\\\\10.60.254.90\\HP-M4".to_string(),
            driver_name: "HP UPD".to_string(),
            is_default: true,
            status: "空闲".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("is_default"));
        assert!(json.contains("true"));
    }
}

// ===== Multi-Server Management Functions =====

/// Tauri Command: Add extra SMB server
#[tauri::command]
pub async fn add_extra_server(
    server_addr: String,
    username: String,
    password: String,
) -> Result<String, String> {
    let server_addr = server_addr.trim().to_string();
    let username = username.trim().to_string();

    if server_addr.is_empty() {
        return Err("服务器地址不能为空".to_string());
    }
    if username.is_empty() {
        return Err("账号不能为空".to_string());
    }
    if password.is_empty() {
        return Err("密码不能为空".to_string());
    }

    // 1. Check if already exists
    let multi_cfg = config::load_multi_server_config();
    if multi_cfg.extra_servers.iter().any(|s| s.server_addr == server_addr) {
        return Err(format!("服务器 {} 已存在", server_addr));
    }

    // 2. Write Windows credentials
    credential::write_credential(&server_addr, &username, &password)?;

    // 3. Save to configuration
    let new_server = config::ExtraServer {
        server_addr: server_addr.clone(),
        username: username.clone(),
        credential_saved: true,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    
    let mut multi_cfg = config::load_multi_server_config();
    multi_cfg.extra_servers.push(new_server.clone());
    config::save_multi_server_config(&multi_cfg)?;

    // Immediately scan printers after adding
    let cfg = config::MainServerConfig {
        server_addr: server_addr.clone(),
        username: username.clone(),
        password: password.clone(),
    };
    config::set_temp_config(&cfg);
    
    let printers = smb_scan::scan_server_printers_fast()?;
    
    config::clear_temp_config();
    
    // Save cache
    save_printer_cache_to_disk(server_addr.clone(), printers.clone())?;

    Ok(format!("已添加服务器 {}，发现 {} 台打印机", server_addr, printers.len()))
}

/// Tauri Command: List all extra servers
#[tauri::command]
pub async fn list_extra_servers() -> Result<Vec<config::ExtraServer>, String> {
    let multi_cfg = config::load_multi_server_config();
    Ok(multi_cfg.extra_servers)
}

/// Tauri Command: Remove extra server
#[tauri::command]
pub async fn remove_extra_server(server_addr: String) -> Result<(), String> {
    let mut multi_cfg = config::load_multi_server_config();
    
    // Find and remove
    let idx = multi_cfg
        .extra_servers
        .iter()
        .position(|s| s.server_addr == server_addr);
    
    match idx {
        Some(i) => {
            multi_cfg.extra_servers.remove(i);
            config::save_multi_server_config(&multi_cfg)?;
            log::info!("已删除服务器：{}", server_addr);
            Ok(())
        }
        None => Err(format!("未找到服务器 {}", server_addr)),
    }
}

/// Tauri Command: Scan cached extra server (using saved credentials)
#[tauri::command]
pub async fn scan_cached_server(server_addr: String) -> Result<Vec<smb_scan::PrinterItem>, String> {
    // 1. Load extra server configuration (with username)
    let multi_cfg = config::load_multi_server_config();
    let server = multi_cfg
        .extra_servers
        .iter()
        .find(|s| s.server_addr == server_addr)
        .ok_or_else(|| format!("未找到服务器 {}", server_addr))?;
    
    // 2. Error if credentials not saved
    if !server.credential_saved {
        return Err("请重新输入密码保存凭据".to_string());
    }
    
    // 3. Use saved credentials for scanning (Windows will auto-inject)
    let cfg = config::MainServerConfig {
        server_addr: server_addr.clone(),
        username: server.username.clone(),
        password: "".to_string(), // Empty password lets Windows auto-lookup
    };
    config::set_temp_config(&cfg);
    
    // Call scan
    let printers = smb_scan::scan_server_printers_fast()?;
    
    // Restore configuration
    config::clear_temp_config();
    
    // 4. Save cache
    save_printer_cache_to_disk(server_addr.clone(), printers.clone())?;
    
    Ok(printers)
}

/// Save printer list to disk (sync function)
fn save_printer_cache_to_disk(
    server_addr: String,
    printers: Vec<smb_scan::PrinterItem>,
) -> Result<(), String> {
    let cache = smb_scan::ServerPrinterCache {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        server_addr: server_addr.clone(),
        printers,
    };
    
    let path = smb_scan::get_server_cache_path(&server_addr);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    
    let json = serde_json::to_string(&cache)
        .map_err(|e| format!("缓存序列化失败：{e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("缓存写入失败：{e}"))?;
    
    log::info!("服务器 {} 的打印机缓存已保存", server_addr);
    Ok(())
}

/// Tauri Command: Get multi-server cache
#[tauri::command]
pub async fn get_printer_cache_multi(server_addr: String) -> Result<Option<Vec<smb_scan::PrinterItem>>, String> {
    let path = smb_scan::get_server_cache_path(&server_addr);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    
    let cache: smb_scan::ServerPrinterCache = match serde_json::from_str(&content) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    
    if cache.server_addr != server_addr {
        log::warn!("缓存服务器地址不匹配");
        return Ok(None);
    }
    
    log::info!("命中服务器 {} 的缓存（{} 台）", server_addr, cache.printers.len());
    Ok(Some(cache.printers))
}

