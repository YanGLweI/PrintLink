//! 打印机系统 API 模块
//! 连接/断开网络打印机、本地列表读取、设为默认、打开属性/首选项

use serde::Serialize;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{GetLastError, HANDLE};
use windows::Win32::Graphics::Printing::{
    AddPrinterConnectionW, ClosePrinter, DeletePrinter, EnumPrintersW, GetPrinterW,
    OpenPrinterW, SetDefaultPrinterW, PRINTER_ENUM_CONNECTIONS, PRINTER_INFO_2W,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

use crate::credential::wide_ptr_to_string;
use crate::smb_scan::PrinterItem;
use crate::utils::{win_error_message, SERVER_ADDR};

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

    let path_w = HSTRING::from(&printer_path);
    unsafe {
        match AddPrinterConnectionW(PCWSTR(path_w.as_ptr())) {
            Ok(()) => {
                log::info!("打印机连接成功: {printer_path}");
                Ok(format!("打印机连接成功：{printer_path}"))
            }
            Err(_) => {
                let code = GetLastError().0;
                log::error!("打印机连接失败: {printer_path}, 错误码={code}");
                Err(match code {
                    1797 | 1930 => {
                        "打印机驱动缺失，请先手动安装对应驱动后重试".to_string()
                    }
                    1802 => "该打印机已安装，无需重复连接".to_string(),
                    1326 => "凭据验证失败，请重启程序重试".to_string(),
                    53 | 1203 => {
                        format!("打印服务器 {SERVER_ADDR} 网络不通，请检查内网连接")
                    }
                    _ => win_error_message("打印机连接", code),
                })
            }
        }
    }
}

/// Tauri 指令：获取本机已安装的打印服务器打印机
#[tauri::command]
pub async fn get_local_printer_list() -> Result<Vec<LocalPrinterItem>, String> {
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
            0,
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
            needed,
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

        let server_prefix_lower = format!("\\\\{SERVER_ADDR}").to_lowercase();
        for info in infos {
            let name = wide_ptr_to_string(info.pPrinterName);
            // 仅保留目标打印服务器的打印机
            if !name.to_lowercase().starts_with(&server_prefix_lower) {
                continue;
            }
            let port_name = wide_ptr_to_string(info.pPortName);
            let driver_name = wide_ptr_to_string(info.pDriverName);
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
        match SetDefaultPrinterW(PCWSTR(name_w.as_ptr())) {
            Ok(()) => {
                log::info!("已设为默认打印机: {name}");
                Ok(format!("已将「{}」设为默认打印机", display_name(&name)))
            }
            Err(_) => {
                let code = GetLastError().0;
                log::error!("设置默认打印机失败: {name}, 错误码={code}");
                Err(win_error_message("设置默认打印机", code))
            }
        }
    }
}

/// Tauri 指令：断开/删除网络打印机
#[tauri::command]
pub async fn remove_printer(name: String) -> Result<String, String> {
    let name_w = HSTRING::from(&name);
    unsafe {
        let mut handle = HANDLE(std::ptr::null_mut());
        if let Err(_) = OpenPrinterW(
            PWSTR(name_w.as_ptr() as *mut u16),
            &mut handle,
            None,
        ) {
            let code = GetLastError().0;
            return Err(win_error_message("打开打印机", code));
        }

        if let Err(_) = DeletePrinter(handle) {
            let code = GetLastError().0;
            let _ = ClosePrinter(handle);
            return Err(win_error_message("删除打印机", code));
        }
        let _ = ClosePrinter(handle);
    }

    log::info!("打印机已断开: {name}");
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
            log::error!("打开打印 UI 失败: {flag} {printer_name}, 代码={}", result.0 as isize);
            return Err("无法打开打印机配置窗口".to_string());
        }
    }
    log::info!("打开打印机对话框: {flag} {printer_name}");
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
            0,
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
            needed,
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
            wide_ptr_to_string(info.pPrinterName).to_lowercase() == target_lower
        })
    }
}

/// 获取系统默认打印机名称
fn get_default_printer_name() -> Option<String> {
    unsafe {
        let mut buf: Vec<u16> = vec![0; 512];
        let mut size = buf.len() as u32;
        let name_ptr = windows::Win32::Graphics::Printing::GetDefaultPrinterW(
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        if name_ptr.is_ok() {
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
pub fn build_printer_path(server: &str, share_name: &str) -> String {
    if share_name.starts_with("\\\\") {
        share_name.to_string()
    } else {
        format!("\\\\{server}\\{share_name}")
    }
}

/// 过滤本地打印机列表，仅保留目标服务器的设备
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
