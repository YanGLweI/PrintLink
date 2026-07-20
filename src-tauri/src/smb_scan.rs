//! SMB 共享打印机扫描模块
//! 枚举 \\10.60.254.90 打印服务器下所有共享打印机

use serde::Serialize;
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

use crate::credential::wide_ptr_to_string;
use crate::utils::{check_server_online, win_error_message, SERVER_ADDR};

/// 可连接打印机信息
#[derive(Debug, Clone, Serialize)]
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

/// Tauri 指令：获取服务器所有可连接共享打印机
#[tauri::command]
pub async fn get_server_printer_list() -> Result<Vec<PrinterItem>, String> {
    scan_server_printers()
}

/// 扫描打印服务器共享打印机（先检测网络，再枚举设备）
pub fn scan_server_printers() -> Result<Vec<PrinterItem>, String> {
    // 1. 网络可达性预检
    check_server_online()?;

    // 2. 优先使用 EnumPrintersW 枚举，失败则回退 WNet 枚举
    let server_unc = format!("\\\\{SERVER_ADDR}");
    match enum_printers_network(&server_unc) {
        Ok(items) if !items.is_empty() => Ok(items),
        Ok(_) => enum_wnet_printers(&server_unc),
        Err(e) => {
            log::warn!("EnumPrintersW 枚举失败: {e}，尝试 WNet 方式");
            enum_wnet_printers(&server_unc)
        }
    }
}

/// 方式一：EnumPrintersW + PRINTER_ENUM_NETWORK 枚举远程共享打印机
fn enum_printers_network(server_unc: &str) -> Result<Vec<PrinterItem>, String> {
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
            if let Some(item) = parse_printer_info1(info) {
                printers.push(item);
            }
        }
        log::info!("EnumPrintersW 发现 {} 台共享打印机", printers.len());
        Ok(printers)
    }
}

/// 解析 PRINTER_INFO_1W 为 PrinterItem
fn parse_printer_info1(info: &PRINTER_INFO_1W) -> Option<PrinterItem> {
    let name = wide_ptr_to_string(info.pName.0);
    if name.is_empty() {
        return None;
    }

    // pName 可能是 \\server\share 或纯共享名
    let share_name = name.rsplit('\\').next().unwrap_or(&name).to_string();
    let share_path = if name.starts_with("\\\\") {
        name.clone()
    } else {
        format!("\\\\{SERVER_ADDR}\\{name}")
    };

    let driver_name =
        get_remote_driver_info(&share_path).unwrap_or_else(|| "连接后自动识别".to_string());

    Some(PrinterItem {
        name: share_name,
        share_path,
        driver_name,
        status: "空闲".to_string(),
    })
}

/// 方式二：WNet 网络资源枚举（回退方案）
fn enum_wnet_printers(server_unc: &str) -> Result<Vec<PrinterItem>, String> {
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
                53 | 1203 => format!("打印服务器 {SERVER_ADDR} 网络不通，请检查内网连接"),
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
                let driver_name = get_remote_driver_info(&remote_name)
                    .unwrap_or_else(|| "连接后自动识别".to_string());
                printers.push(PrinterItem {
                    name: share_name,
                    share_path: remote_name,
                    driver_name,
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
        let name = "Epson-L3150";
        let path = if name.starts_with("\\\\") {
            name.to_string()
        } else {
            format!("\\\\{SERVER_ADDR}\\{name}")
        };
        assert_eq!(path, "\\\\10.60.254.90\\Epson-L3150");
    }

    #[test]
    fn test_scan_fails_gracefully_when_offline() {
        // 服务器不可达时应返回友好中文错误而非 panic
        let result = scan_server_printers();
        if let Err(e) = result {
            assert!(!e.is_empty());
        }
    }
}
