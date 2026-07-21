//! SMB 共享打印机扫描模块
//! 枚举打印服务器下所有共享打印机（两阶段：快速枚举 + 后台驱动查询）

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{
    GetLastError, HANDLE, ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS,
    NO_ERROR,
};
use windows::Win32::Graphics::Printing::{
    ClosePrinter, EnumPrintersW, GetPrinterW, OpenPrinterW, PRINTER_ENUM_NETWORK,
    PRINTER_INFO_1W, PRINTER_INFO_2W,
};
use windows::Win32::NetworkManagement::WNet::{
    WNetCloseEnum, WNetEnumResourceW, WNetOpenEnumW, NETRESOURCEW, NET_RESOURCE_SCOPE,
    RESOURCE_GLOBALNET, RESOURCETYPE_PRINT, RESOURCEUSAGE_ALL, RESOURCEUSAGE_CONTAINER,
};

use crate::config;
use crate::credential::wide_ptr_to_string;
use crate::utils::{check_server_online, win_error_message};

/// 可连接打印机信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterItem {
    /// 打印机显示名称（共享名）
    pub name: String,
    /// 完整 SMB 共享路径
    pub share_path: String,
    /// 驱动名称
    pub driver_name: String,
    /// 设备状态
    pub status: String,
}

/// 驱动信息更新事件 payload
#[derive(Debug, Clone, Serialize)]
pub struct DriverInfoUpdate {
    /// 打印机共享路径
    pub share_path: String,
    /// 驱动名称
    pub driver_name: String,
    /// 当前进度
    pub progress: u32,
    /// 总数
    pub total: u32,
}

/// Tauri 指令：获取服务器所有可连接共享打印机（快速枚举，不含驱动信息）
#[tauri::command]
pub async fn get_server_printer_list() -> Result<Vec<PrinterItem>, String> {
    scan_server_printers_fast()
}

/// Tauri 指令：后台批量获取驱动信息，通过 Event 逐条推送
#[tauri::command]
pub async fn fetch_driver_info_async(
    app: tauri::AppHandle,
    printers: Vec<PrinterItem>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let total = printers.len();
        for (i, printer) in printers.iter().enumerate() {
            if let Some(driver) = get_remote_driver_info(&printer.share_path) {
                let _ = app.emit(
                    "driver-info-updated",
                    DriverInfoUpdate {
                        share_path: printer.share_path.clone(),
                        driver_name: driver,
                        progress: (i + 1) as u32,
                        total: total as u32,
                    },
                );
            }
        }
        let _ = app.emit("driver-info-complete", total);
        log::info!("后台驱动信息获取完成，共 {total} 台");
    });
    Ok(())
}

/// 阶段一：快速枚举打印服务器共享打印机（仅名称+路径，不查驱动）
pub fn scan_server_printers_fast() -> Result<Vec<PrinterItem>, String> {
    let cfg = config::load_config();
    let server_addr = &cfg.server_addr;

    // 1. 网络可达性预检
    check_server_online(server_addr)?;

    // 2. 优先使用 EnumPrintersW 枚举，失败则回退 WNet 枚举
    let server_unc = format!("\\\\{server_addr}");
    match enum_printers_network(&server_unc, server_addr) {
        Ok(items) if !items.is_empty() => Ok(items),
        Ok(_) => enum_wnet_printers(&server_unc, server_addr),
        Err(e) => {
            log::warn!("EnumPrintersW 枚举失败: {e}，尝试 WNet 方式");
            enum_wnet_printers(&server_unc, server_addr)
        }
    }
}

/// 方式一：EnumPrintersW + PRINTER_ENUM_NETWORK 枚举远程共享打印机
fn enum_printers_network(server_unc: &str, server_addr: &str) -> Result<Vec<PrinterItem>, String> {
    let server_w = HSTRING::from(server_unc);
    unsafe {
        let mut needed: u32 = 0;
        let mut returned: u32 = 0;

        // 第一次调用获取所需缓冲区大小
        let _ = EnumPrintersW(
            PRINTER_ENUM_NETWORK,
            &server_w,
            1,
            None,
            &mut needed,
            &mut returned,
        );

        let err = GetLastError();
        if err != ERROR_INSUFFICIENT_BUFFER && err != ERROR_MORE_DATA {
            if returned == 0 && needed == 0 {
                return Ok(Vec::new());
            }
            return Err(win_error_message("打印机列表枚举", err.0));
        }
        if needed == 0 {
            return Ok(Vec::new());
        }

        // 第二次调用填充数据
        let mut buf: Vec<u8> = vec![0; needed as usize];
        let result = EnumPrintersW(
            PRINTER_ENUM_NETWORK,
            &server_w,
            1,
            Some(buf.as_mut_slice()),
            &mut needed,
            &mut returned,
        );
        if result.is_err() && returned == 0 {
            return Err(win_error_message("打印机列表枚举", GetLastError().0));
        }

        let infos = std::slice::from_raw_parts(
            buf.as_ptr() as *const PRINTER_INFO_1W,
            returned as usize,
        );

        let mut printers = Vec::new();
        for info in infos {
            if let Some(item) = parse_printer_info1(info, server_addr) {
                printers.push(item);
            }
        }
        log::info!("EnumPrintersW 发现 {} 台共享打印机", printers.len());
        Ok(printers)
    }
}

/// 解析 PRINTER_INFO_1W 为 PrinterItem（快速模式：不查询驱动信息）
fn parse_printer_info1(info: &PRINTER_INFO_1W, server_addr: &str) -> Option<PrinterItem> {
    let name = wide_ptr_to_string(info.pName.0);
    if name.is_empty() {
        return None;
    }

    // pName 可能是 \\server\share 或纯共享名
    let share_name = name.rsplit('\\').next().unwrap_or(&name).to_string();
    let share_path = if name.starts_with("\\\\") {
        name.clone()
    } else {
        format!("\\\\{server_addr}\\{name}")
    };

    Some(PrinterItem {
        name: share_name,
        share_path,
        driver_name: "连接后自动识别".to_string(),
        status: "空闲".to_string(),
    })
}

/// 方式二：WNet 网络资源枚举（回退方案）
fn enum_wnet_printers(server_unc: &str, server_addr: &str) -> Result<Vec<PrinterItem>, String> {
    let server_w = HSTRING::from(server_unc);
    unsafe {
        let net_resource = NETRESOURCEW {
            dwScope: NET_RESOURCE_SCOPE(0),
            dwType: RESOURCETYPE_PRINT,
            dwDisplayType: 0,
            dwUsage: RESOURCEUSAGE_CONTAINER.0,
            lpLocalName: PWSTR::null(),
            lpRemoteName: PWSTR(server_w.as_ptr() as *mut u16),
            lpComment: PWSTR::null(),
            lpProvider: PWSTR::null(),
        };

        let mut handle = HANDLE(std::ptr::null_mut());
        let result = WNetOpenEnumW(
            RESOURCE_GLOBALNET,
            RESOURCETYPE_PRINT,
            RESOURCEUSAGE_ALL,
            Some(&net_resource as *const NETRESOURCEW),
            &mut handle,
        );
        if result != NO_ERROR {
            return Err(match result.0 {
                53 | 1203 => format!("打印服务器 {server_addr} 网络不通，请检查内网连接"),
                1326 => "凭据验证失败，请重启程序重试".to_string(),
                _ => win_error_message("网络枚举", result.0),
            });
        }

        let mut printers = Vec::new();
        let mut buf: Vec<u8> = vec![0; 16384];

        loop {
            let mut count: u32 = u32::MAX;
            let mut buf_size: u32 = buf.len() as u32;

            let result = WNetEnumResourceW(
                handle,
                &mut count,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                &mut buf_size,
            );

            if result == ERROR_NO_MORE_ITEMS {
                break;
            }
            if result.0 == ERROR_INSUFFICIENT_BUFFER.0 || result.0 == ERROR_MORE_DATA.0 {
                if buf_size > buf.len() as u32 {
                    buf.resize(buf_size as usize, 0);
                    continue;
                }
                // 缓冲区无法扩大，避免无限循环
                let _ = WNetCloseEnum(handle);
                return Err("打印机枚举缓冲区分配异常".to_string());
            }
            if result != NO_ERROR {
                let _ = WNetCloseEnum(handle);
                return Err(win_error_message("打印机枚举", result.0));
            }

            let resources = std::slice::from_raw_parts(
                buf.as_ptr() as *const NETRESOURCEW,
                count as usize,
            );
            for res in resources {
                if res.dwType != RESOURCETYPE_PRINT {
                    continue;
                }
                let remote_name = wide_ptr_to_string(res.lpRemoteName.0);
                if remote_name.is_empty() {
                    continue;
                }
                let share_name = remote_name
                    .rsplit('\\')
                    .next()
                    .unwrap_or(&remote_name)
                    .to_string();
                printers.push(PrinterItem {
                    name: share_name,
                    share_path: remote_name,
                    driver_name: "连接后自动识别".to_string(),
                    status: "空闲".to_string(),
                });
            }
        }

        let _ = WNetCloseEnum(handle);
        log::info!("WNet 枚举发现 {} 台共享打印机", printers.len());
        Ok(printers)
    }
}

/// 尝试获取远程打印机驱动信息（可能失败，失败返回 None）
fn get_remote_driver_info(share_path: &str) -> Option<String> {
    let path_w = HSTRING::from(share_path);
    unsafe {
        let mut handle = HANDLE(std::ptr::null_mut());
        if OpenPrinterW(&path_w, &mut handle, None).is_err() {
            return None;
        }

        let mut needed: u32 = 0;
        let _ = GetPrinterW(handle, 2, None, &mut needed);
        if needed == 0 {
            let _ = ClosePrinter(handle);
            return None;
        }

        let mut buf: Vec<u8> = vec![0; needed as usize];
        let result = GetPrinterW(handle, 2, Some(buf.as_mut_slice()), &mut needed);
        let _ = ClosePrinter(handle);

        if result.is_err() {
            return None;
        }

        let info = &*(buf.as_ptr() as *const PRINTER_INFO_2W);
        let driver = wide_ptr_to_string(info.pDriverName.0);
        if driver.is_empty() {
            None
        } else {
            Some(driver)
        }
    }
}

// ===== 打印机列表缓存 =====

/// 缓存文件结构
#[derive(Serialize, Deserialize)]
struct PrinterCache {
    /// 缓存时间戳（Unix seconds）
    timestamp: u64,
    /// 缓存对应的服务器地址（配置变更时缓存失效）
    server_addr: String,
    /// 打印机列表
    printers: Vec<PrinterItem>,
}

/// 获取缓存文件路径：%APPDATA%/PrintLink/printer_cache.json
fn get_cache_path() -> std::path::PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(appdata)
        .join("PrintLink")
        .join("printer_cache.json")
}

/// Tauri 指令：读取打印机缓存（校验 server_addr 一致性）
#[tauri::command]
pub async fn get_printer_cache() -> Result<Option<Vec<PrinterItem>>, String> {
    let path = get_cache_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let cache: PrinterCache = match serde_json::from_str(&content) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    // 校验服务器地址一致，不一致则缓存失效
    let cfg = config::load_config();
    if cache.server_addr != cfg.server_addr {
        log::info!("缓存服务器地址不匹配，缓存失效");
        return Ok(None);
    }
    log::info!("命中打印机缓存（{} 台）", cache.printers.len());
    Ok(Some(cache.printers))
}

/// Tauri 指令：保存打印机列表到缓存
#[tauri::command]
pub async fn save_printer_cache(printers: Vec<PrinterItem>) -> Result<(), String> {
    let cfg = config::load_config();
    let cache = PrinterCache {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        server_addr: cfg.server_addr,
        printers,
    };
    let path = get_cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string(&cache).map_err(|e| format!("缓存序列化失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("缓存写入失败: {e}"))?;
    log::info!("打印机缓存已保存");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_printer_item_serialization() {
        let item = PrinterItem {
            name: "HP-M4".to_string(),
            share_path: "\\\\10.60.254.90\\HP-M4".to_string(),
            driver_name: "HP Universal".to_string(),
            status: "空闲".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("HP-M4"));
        assert!(json.contains("share_path"));
    }

    #[test]
    fn test_share_name_extraction() {
        let full_path = "\\\\10.60.254.90\\HP-LaserJet";
        let share_name = full_path.rsplit('\\').next().unwrap();
        assert_eq!(share_name, "HP-LaserJet");

        let plain_name = "Canon-ADV";
        let share_name = plain_name.rsplit('\\').next().unwrap();
        assert_eq!(share_name, "Canon-ADV");
    }

    #[test]
    fn test_share_path_construction() {
        let server_addr = config::DEFAULT_SERVER_ADDR;
        let name = "Epson-L3150";
        let path = if name.starts_with("\\\\") {
            name.to_string()
        } else {
            format!("\\\\{server_addr}\\{name}")
        };
        assert_eq!(path, "\\\\10.60.254.90\\Epson-L3150");
    }

    #[test]
    fn test_scan_fails_gracefully_when_offline() {
        // 服务器不可达时应返回友好中文错误而非 panic
        let result = scan_server_printers_fast();
        if let Err(e) = result {
            assert!(!e.is_empty());
        }
    }

    #[test]
    fn test_printer_cache_serialization() {
        let cache = PrinterCache {
            timestamp: 1700000000,
            server_addr: "10.60.254.90".to_string(),
            printers: vec![PrinterItem {
                name: "Test".to_string(),
                share_path: "\\\\10.60.254.90\\Test".to_string(),
                driver_name: "连接后自动识别".to_string(),
                status: "空闲".to_string(),
            }],
        };
        let json = serde_json::to_string(&cache).unwrap();
        let restored: PrinterCache = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.server_addr, "10.60.254.90");
        assert_eq!(restored.printers.len(), 1);
    }
}
