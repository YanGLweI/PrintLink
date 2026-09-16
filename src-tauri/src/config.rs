//! 应用配置持久化模块
//! 配置文件存储于 %APPDATA%/PrintLink/config.json，支持读取、保存、恢复默认

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 默认打印服务器地址
pub const DEFAULT_SERVER_ADDR: &str = "10.60.254.90";
/// 默认凭据用户名
pub const DEFAULT_USERNAME: &str = "print";
/// 默认凭据密码
pub const DEFAULT_PASSWORD: &str = "a*999999";

/// 应用配置结构体（主打印服务器）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainServerConfig {
    /// 打印服务器地址（IP 或主机名）
    pub server_addr: String,
    /// SMB 凭据用户名
    pub username: String,
    /// SMB 凭据密码
    pub password: String,
}

impl Default for MainServerConfig {
    fn default() -> Self {
        Self {
            server_addr: DEFAULT_SERVER_ADDR.to_string(),
            username: DEFAULT_USERNAME.to_string(),
            password: DEFAULT_PASSWORD.to_string(),
        }
    }
}

/// 附加服务器信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraServer {
    /// 服务器地址（IP/域名）
    pub server_addr: String,
    /// 用户名
    pub username: String,
    /// 是否已保存凭据到 Windows（禁止前端传密码）
    pub credential_saved: bool,
    /// 添加时间
    pub created_at: u64,
}

/// 多服务器配置容器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiServerConfig {
    /// 主服务器地址（沿用 config.json）
    pub main_server_addr: Option<String>,
    /// 附加服务器列表
    pub extra_servers: Vec<ExtraServer>,
}

impl Default for MultiServerConfig {
    fn default() -> Self {
        Self {
            main_server_addr: None,
            extra_servers: Vec::new(),
        }
    }
}

/// 获取多服务器配置文件路径：%APPDATA%/PrintLink/multi_servers.json
fn get_multi_server_config_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("PrintLink")
        .join("multi_servers.json")
}

/// 加载多服务器配置
#[tauri::command]
pub fn load_multi_server_config() -> MultiServerConfig {
    let path = get_multi_server_config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<MultiServerConfig>(&content) {
            Ok(config) => {
                log::info!("多服务器配置加载成功，附加服务器数量：{}", config.extra_servers.len());
                config
            }
            Err(e) => {
                log::warn!("多服务器配置文件解析失败，使用默认配置：{e}");
                MultiServerConfig::default()
            }
        },
        Err(_) => {
            log::info!("多服务器配置文件不存在，使用默认配置");
            MultiServerConfig::default()
        }
    }
}

/// 保存多服务器配置到 JSON 文件
#[tauri::command]
pub fn save_multi_server_config(config: &MultiServerConfig) -> Result<(), String> {
    let path = get_multi_server_config_path();
    // 确保目录存在
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建配置目录：{e}"))?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("配置序列化失败：{e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("配置文件写入失败：{e}"))?;
    log::info!("多服务器配置已保存，附加服务器数量：{}", config.extra_servers.len());
    Ok(())
}

/// 获取配置文件路径：%APPDATA%/PrintLink/config.json
fn get_config_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("PrintLink")
        .join("config.json")
}

// ===== 临时配置钩子（用于扫描附加服务器时不持久化） =====

use std::sync::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    static ref TEMP_CONFIG: Mutex<Option<MainServerConfig>> = Mutex::new(None);
}

/// 设置临时配置（仅在当前线程/任务中生效）
pub fn set_temp_config(cfg: &MainServerConfig) {
    let mut guard = TEMP_CONFIG.lock().unwrap();
    *guard = Some(cfg.clone());
}

/// 获取当前配置（优先返回临时配置）
pub fn get_current_config() -> MainServerConfig {
    let temp_guard = TEMP_CONFIG.lock().unwrap();
    if let Some(temp) = temp_guard.as_ref() {
        temp.clone()
    } else {
        load_config()
    }
}

/// 清除临时配置
pub fn clear_temp_config() {
    let mut guard = TEMP_CONFIG.lock().unwrap();
    *guard = None;
}

/// 加载配置：读取 JSON 文件，文件不存在或解析失败时返回默认值
pub fn load_config() -> MainServerConfig {
    let path = get_config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<MainServerConfig>(&content) {
            Ok(config) => {
                log::info!("配置加载成功：server={}", config.server_addr);
                config
            }
            Err(e) => {
                log::warn!("配置文件解析失败，使用默认配置：{e}");
                MainServerConfig::default()
            }
        },
        Err(_) => {
            log::info!("配置文件不存在，使用默认配置");
            MainServerConfig::default()
        }
    }
}

/// 保存配置到 JSON 文件
pub fn save_config(config: &MainServerConfig) -> Result<(), String> {
    let path = get_config_path();
    // 确保目录存在
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建配置目录: {e}"))?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("配置序列化失败: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("配置文件写入失败: {e}"))?;
    log::info!("配置已保存: server={}", config.server_addr);
    Ok(())
}

/// Tauri 指令：获取当前配置
#[tauri::command]
pub async fn get_config() -> Result<MainServerConfig, String> {
    Ok(load_config())
}

/// Tauri 指令：保存配置
#[tauri::command]
pub async fn save_config_command(
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

    let config = MainServerConfig {
        server_addr,
        username,
        password,
    };
    save_config(&config)?;
    Ok(format!("配置已保存（服务器：{}）", config.server_addr))
}

/// Tauri 指令：恢复默认配置
#[tauri::command]
pub async fn reset_config() -> Result<MainServerConfig, String> {
    let path = get_config_path();
    // 删除配置文件（如果存在）
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("无法删除配置文件：{e}"))?;
    }
    log::info!("配置已恢复默认");
    Ok(MainServerConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MainServerConfig::default();
        assert_eq!(config.server_addr, "10.60.254.90");
        assert_eq!(config.username, "print");
        assert_eq!(config.password, "a*999999");
    }

    #[test]
    fn test_config_serialization() {
        let config = MainServerConfig {
            server_addr: "192.168.1.1".to_string(),
            username: "admin".to_string(),
            password: "pass123".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("192.168.1.1"));
        assert!(json.contains("admin"));

        let deserialized: MainServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.server_addr, config.server_addr);
        assert_eq!(deserialized.username, config.username);
        assert_eq!(deserialized.password, config.password);
    }

    #[test]
    fn test_config_path() {
        let path = get_config_path();
        assert!(path.to_string_lossy().contains("PrintLink"));
        assert!(path.to_string_lossy().ends_with("config.json"));
    }
}
