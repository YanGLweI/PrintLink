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

/// 应用配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 打印服务器地址（IP 或主机名）
    pub server_addr: String,
    /// SMB 凭据用户名
    pub username: String,
    /// SMB 凭据密码
    pub password: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server_addr: DEFAULT_SERVER_ADDR.to_string(),
            username: DEFAULT_USERNAME.to_string(),
            password: DEFAULT_PASSWORD.to_string(),
        }
    }
}

/// 获取配置文件路径：%APPDATA%/PrintLink/config.json
fn get_config_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("PrintLink")
        .join("config.json")
}

/// 加载配置：读取 JSON 文件，文件不存在或解析失败时返回默认值
pub fn load_config() -> AppConfig {
    let path = get_config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<AppConfig>(&content) {
            Ok(config) => {
                log::info!("配置加载成功: server={}", config.server_addr);
                config
            }
            Err(e) => {
                log::warn!("配置文件解析失败，使用默认配置: {e}");
                AppConfig::default()
            }
        },
        Err(_) => {
            log::info!("配置文件不存在，使用默认配置");
            AppConfig::default()
        }
    }
}

/// 保存配置到 JSON 文件
pub fn save_config(config: &AppConfig) -> Result<(), String> {
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
pub async fn get_config() -> Result<AppConfig, String> {
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

    let config = AppConfig {
        server_addr,
        username,
        password,
    };
    save_config(&config)?;
    Ok(format!("配置已保存（服务器：{}）", config.server_addr))
}

/// Tauri 指令：恢复默认配置
#[tauri::command]
pub async fn reset_config() -> Result<AppConfig, String> {
    let path = get_config_path();
    // 删除配置文件（如果存在）
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("无法删除配置文件: {e}"))?;
    }
    log::info!("配置已恢复默认");
    Ok(AppConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.server_addr, "10.60.254.90");
        assert_eq!(config.username, "print");
        assert_eq!(config.password, "a*999999");
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig {
            server_addr: "192.168.1.1".to_string(),
            username: "admin".to_string(),
            password: "pass123".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("192.168.1.1"));
        assert!(json.contains("admin"));

        let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
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
