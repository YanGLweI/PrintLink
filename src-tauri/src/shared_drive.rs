//! SMB 共享盘连接模块
//! 支持连接/断开 SMB 共享盘、枚举根文件夹、调用资源管理器打开

use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, ERROR_NO_MORE_ITEMS, HANDLE};
use windows::Win32::NetworkManagement::WNet::{
    WNetAddConnection2W, WNetCancelConnection2W, WNetOpenEnumW, WNetEnumResourceW,
    WNetCloseEnum, NETRESOURCEW, NET_CONNECT_FLAGS, RESOURCETYPE_DISK,
    RESOURCEUSAGE_ALL, RESOURCE_GLOBALNET, RESOURCE_CONNECTED, CONNECT_UPDATE_PROFILE, WNET_OPEN_ENUM_USAGE,
};

/// 共享盘持久化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedDriveConfig {
    /// 共享盘服务器地址
    pub server_addr: String,
    /// SMB 账号
    pub username: String,
    /// SMB 密码
    pub password: String,
}

/// 获取共享盘配置文件路径：%APPDATA%/PrintLink/shared_drive.json
fn get_config_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("PrintLink")
        .join("shared_drive.json")
}

/// 读取共享盘配置
fn load_shared_drive_config() -> Option<SharedDriveConfig> {
    let path = get_config_path();
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 保存共享盘配置
fn save_shared_drive_config(config: &SharedDriveConfig) -> Result<(), String> {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建配置目录: {e}"))?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("配置序列化失败: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("配置写入失败: {e}"))?;
    Ok(())
}

/// 建立 SMB 连接（WNetAddConnection2W）
fn establish_smb_connection(server_addr: &str, username: &str, password: &str) -> Result<(), String> {
    let remote = format!("\\\\{server_addr}\\IPC$");
    let remote_w = HSTRING::from(&remote);
    let username_w = HSTRING::from(username);
    let password_w = HSTRING::from(password);

    unsafe {
        // 先尝试断开已有连接（忽略错误）
        let _ = WNetCancelConnection2W(&remote_w, NET_CONNECT_FLAGS(0), true);

        let net_resource = NETRESOURCEW {
            dwScope: Default::default(),
            dwType: RESOURCETYPE_DISK,
            dwDisplayType: 0,
            dwUsage: RESOURCEUSAGE_ALL.0,
            lpLocalName: PWSTR::null(),
            lpRemoteName: PWSTR(remote_w.as_ptr() as *mut u16),
            lpComment: PWSTR::null(),
            lpProvider: PWSTR::null(),
        };

        let result = WNetAddConnection2W(
            &net_resource,
            &password_w,
            &username_w,
            CONNECT_UPDATE_PROFILE,
        );

        if result == ERROR_SUCCESS {
            log::info!("SMB 连接成功: {remote}");
            Ok(())
        } else {
            let err_msg = match result.0 {
                5 => "访问被拒绝，请检查账号密码是否正确".to_string(),
                53 => format!("找不到网络路径 {remote}，请检查服务器地址"),
                86 => "指定的网络密码不正确".to_string(),
                1219 => {
                    // 已有不同凭据的连接，强制断开后重试
                    let _ = WNetCancelConnection2W(&remote_w, NET_CONNECT_FLAGS(0), true);
                    let retry = WNetAddConnection2W(
                        &net_resource,
                        &password_w,
                        &username_w,
                        CONNECT_UPDATE_PROFILE,
                    );
                    if retry == ERROR_SUCCESS {
                        log::info!("SMB 重连成功: {remote}");
                        return Ok(());
                    }
                    format!("已有其他凭据连接到该服务器（错误码 {}），请先断开其他连接", retry.0)
                }
                1326 => "登录失败：用户名或密码错误".to_string(),
                _ => format!("SMB 连接失败（错误码 {}）", result.0),
            };
            log::error!("SMB 连接失败: {err_msg}");
            Err(err_msg)
        }
    }
}

/// 枚举共享盘根文件夹（使用 WNetOpenEnumW / WNetEnumResourceW）
fn list_root_folders(server_addr: &str) -> Result<Vec<String>, String> {
    let unc = format!("\\\\{server_addr}");
    let unc_w = HSTRING::from(&unc);

    let net_resource = NETRESOURCEW {
        dwScope: Default::default(),
        dwType: RESOURCETYPE_DISK,
        dwDisplayType: 0,
        dwUsage: RESOURCEUSAGE_ALL.0,
        lpLocalName: PWSTR::null(),
        lpRemoteName: PWSTR(unc_w.as_ptr() as *mut u16),
        lpComment: PWSTR::null(),
        lpProvider: PWSTR::null(),
    };

    let mut folders = Vec::new();

    unsafe {
        let mut handle = HANDLE::default();
        let result = WNetOpenEnumW(
            RESOURCE_GLOBALNET,
            RESOURCETYPE_DISK,
            WNET_OPEN_ENUM_USAGE(0),
            Some(&net_resource as *const NETRESOURCEW),
            &mut handle,
        );
        if result != ERROR_SUCCESS {
            return Err(format!("无法枚举共享目录 {unc}（错误码 {}）", result.0));
        }

        // 循环枚举共享资源
        let buf_size: u32 = 16384;
        let mut buffer: Vec<u8> = vec![0u8; buf_size as usize];

        loop {
            let mut count: u32 = u32::MAX; // 请求尽可能多的条目
            let mut bytes_needed: u32 = buf_size;

            let enum_result = WNetEnumResourceW(
                handle,
                &mut count,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                &mut bytes_needed,
            );

            if enum_result == ERROR_NO_MORE_ITEMS {
                break;
            }
            if enum_result != ERROR_SUCCESS {
                let _ = WNetCloseEnum(handle);
                return Err(format!("枚举共享资源失败（错误码 {}）", enum_result.0));
            }

            // 解析返回的 NETRESOURCEW 数组
            let ptr = buffer.as_ptr() as *const NETRESOURCEW;
            for i in 0..count as isize {
                let resource = &*ptr.offset(i);
                if !resource.lpRemoteName.is_null() {
                    let remote_name = resource.lpRemoteName.to_string().unwrap_or_default();
                    // lpRemoteName 格式: \\server\sharename
                    if let Some(share_name) = remote_name.rsplit('\\').next() {
                        if !share_name.is_empty() && !share_name.ends_with('$') {
                            folders.push(share_name.to_string());
                        }
                    }
                }
            }
        }

        let _ = WNetCloseEnum(handle);
    }

    folders.sort();
    folders.dedup();
    log::info!("共享盘 {unc} 发现 {} 个文件夹", folders.len());
    Ok(folders)
}

// ===== Tauri 指令 =====

/// Tauri 指令：连接共享盘（写入凭据 + 建立 SMB 会话 + 返回文件夹列表）
#[tauri::command]
pub async fn connect_shared_drive(
    server_addr: String,
    username: String,
    password: String,
) -> Result<Vec<String>, String> {
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

    // 1. 建立 SMB 连接
    establish_smb_connection(&server_addr, &username, &password)?;

    // 2. 写入 Windows 凭据管理器
    crate::credential::write_credential(&server_addr, &username, &password)?;

    // 3. 枚举根文件夹
    let folders = list_root_folders(&server_addr)?;

    // 4. 保存配置（供后续自动连接）
    let config = SharedDriveConfig {
        server_addr: server_addr.clone(),
        username,
        password,
    };
    save_shared_drive_config(&config)?;

    log::info!("共享盘连接完成: \\\\{server_addr}，{} 个文件夹", folders.len());
    Ok(folders)
}

/// Tauri 指令：获取共享盘文件夹列表（已连接状态下刷新用）
#[tauri::command]
pub async fn get_shared_drive_folders() -> Result<Vec<String>, String> {
    let config = load_shared_drive_config()
        .ok_or_else(|| "未找到共享盘配置，请先连接".to_string())?;
    list_root_folders(&config.server_addr)
}

/// Tauri 指令：打开文件夹（调用系统资源管理器）
#[tauri::command]
pub async fn open_shared_folder(folder_path: String) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(&folder_path)
        .spawn()
        .map_err(|e| format!("无法打开资源管理器: {e}"))?;
    log::info!("已打开文件夹: {folder_path}");
    Ok(())
}

/// 枚举当前所有已连接的网络资源，取消所有到指定服务器的连接
fn cancel_all_connections_to_server(server_addr: &str) {
    let prefix = format!("\\\\{}", server_addr.to_lowercase());

    unsafe {
        let mut handle = HANDLE::default();
        let result = WNetOpenEnumW(
            RESOURCE_CONNECTED,
            RESOURCETYPE_DISK,
            WNET_OPEN_ENUM_USAGE(0),
            None,
            &mut handle,
        );
        if result != ERROR_SUCCESS {
            return;
        }

        let buf_size: u32 = 16384;
        let mut buffer: Vec<u8> = vec![0u8; buf_size as usize];

        loop {
            let mut count: u32 = u32::MAX;
            let mut bytes_needed: u32 = buf_size;
            let enum_result = WNetEnumResourceW(
                handle,
                &mut count,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                &mut bytes_needed,
            );
            if enum_result == ERROR_NO_MORE_ITEMS { break; }
            if enum_result != ERROR_SUCCESS { break; }

            let ptr = buffer.as_ptr() as *const NETRESOURCEW;
            for i in 0..count as isize {
                let resource = &*ptr.offset(i);
                if !resource.lpRemoteName.is_null() {
                    let remote = resource.lpRemoteName.to_string().unwrap_or_default();
                    if remote.to_lowercase().starts_with(&prefix) {
                        let remote_w = HSTRING::from(&remote);
                        let _ = WNetCancelConnection2W(&remote_w, NET_CONNECT_FLAGS(1), true);
                        log::info!("已断开连接: {remote}");
                    }
                }
            }
        }
        let _ = WNetCloseEnum(handle);
    }
}

/// Tauri 指令：退出登录（删除凭据 + 断开连接 + 清理会话）
#[tauri::command]
pub async fn disconnect_shared_drive() -> Result<String, String> {
    let config = load_shared_drive_config();

    if let Some(ref cfg) = config {
        // 1. 删除 Windows 凭据管理器中的凭据
        let _ = crate::credential::delete_credential(&cfg.server_addr);

        // 2. 枚举并取消所有到该服务器的共享连接（含 Explorer 创建的）
        cancel_all_connections_to_server(&cfg.server_addr);

        // 3. 显式取消 IPC$（会话级连接）
        let ipc = format!("\\\\{}\\IPC$", cfg.server_addr);
        let ipc_w = HSTRING::from(&ipc);
        unsafe {
            let _ = WNetCancelConnection2W(&ipc_w, NET_CONNECT_FLAGS(1), true);
        }

        // 4. 兜底：通过 net use CLI 强制删除 IPC$ 会话（隐藏窗口）
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("net")
            .args(["use", &ipc, "/delete", "/y"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        // 5. 用虚假凭据覆盖旧会话（Windows SMB 同地址仅允许一个会话）
        let dummy_user = HSTRING::from("__printlink_cleanup__");
        let dummy_pass = HSTRING::from("invalid");
        unsafe {
            let net_resource = NETRESOURCEW {
                dwScope: Default::default(),
                dwType: RESOURCETYPE_DISK,
                dwDisplayType: 0,
                dwUsage: RESOURCEUSAGE_ALL.0,
                lpLocalName: PWSTR::null(),
                lpRemoteName: PWSTR(ipc_w.as_ptr() as *mut u16),
                lpComment: PWSTR::null(),
                lpProvider: PWSTR::null(),
            };
            // 尝试用虚假凭据建立连接（覆盖旧会话），不持久化
            let _ = WNetAddConnection2W(&net_resource, &dummy_pass, &dummy_user, NET_CONNECT_FLAGS(0));
            // 立即取消该临时连接
            let _ = WNetCancelConnection2W(&ipc_w, NET_CONNECT_FLAGS(0), true);
        }

        log::info!("已清理共享盘凭据与会话: \\{}", cfg.server_addr);
    }

    // 6. 删除配置文件
    let path = get_config_path();
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("无法删除配置文件: {e}"))?;
    }

    Ok("已退出登录，凭据已删除，会话已清理".to_string())
}

/// Tauri 指令：检查是否已保存共享盘配置（用于启动时自动连接）
#[tauri::command]
pub async fn get_shared_drive_config() -> Result<Option<SharedDriveConfig>, String> {
    Ok(load_shared_drive_config())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_drive_config_serialization() {
        let config = SharedDriveConfig {
            server_addr: "192.168.1.100".to_string(),
            username: "admin".to_string(),
            password: "pass123".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("192.168.1.100"));
        let restored: SharedDriveConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.server_addr, "192.168.1.100");
        assert_eq!(restored.username, "admin");
    }

    #[test]
    fn test_config_path() {
        let path = get_config_path();
        assert!(path.to_string_lossy().contains("PrintLink"));
        assert!(path.to_string_lossy().ends_with("shared_drive.json"));
    }

    #[test]
    fn test_filter_hidden_shares() {
        // 以 $ 结尾的共享应被过滤
        let names = vec!["public", "IPC$", "admin$", "docs"];
        let filtered: Vec<&str> = names
            .into_iter()
            .filter(|n| !n.ends_with('$'))
            .collect();
        assert_eq!(filtered, vec!["public", "docs"]);
    }
}
