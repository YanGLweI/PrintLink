//! 工具模块：日志初始化、网络检测、Windows 错误码转中文提示

use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use simplelog::{CombinedLogger, Config, LevelFilter, WriteLogger};

/// SMB 服务端口
pub const SMB_PORT: u16 = 445;
/// 网络探测超时（秒）
pub const NETWORK_TIMEOUT_SECS: u64 = 3;

/// 初始化日志系统，写入 %APPDATA%/PrintLink/logs/printlink.log
pub fn init_logger() {
    let log_dir = get_log_dir();
    if std::fs::create_dir_all(&log_dir).is_err() {
        eprintln!("无法创建日志目录: {}", log_dir.display());
        return;
    }
    let log_file = log_dir.join("printlink.log");
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("无法打开日志文件: {e}");
            return;
        }
    };
    let _ = CombinedLogger::init(vec![WriteLogger::new(
        LevelFilter::Info,
        Config::default(),
        file,
    )]);
    log::info!("========== PrintLink 启动 ==========");
}

/// 获取日志目录路径
pub fn get_log_dir() -> PathBuf {
    let appdata =
        std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata).join("PrintLink").join("logs")
}

/// 检测打印服务器 SMB 端口是否可达（3 秒超时）
pub fn check_server_online(server_addr: &str) -> Result<(), String> {
    let addr = format!("{server_addr}:{SMB_PORT}");
    match TcpStream::connect_timeout(
        &addr.parse().map_err(|_| format!("地址解析失败: {addr}"))?,
        Duration::from_secs(NETWORK_TIMEOUT_SECS),
    ) {
        Ok(_) => {
            log::info!("服务器 {server_addr} SMB 端口可达");
            Ok(())
        }
        Err(e) => {
            log::warn!("服务器 {server_addr} 网络不通: {e}");
            Err(format!(
                "打印服务器 {server_addr} 网络不通，请检查内网连接"
            ))
        }
    }
}

/// 将 Windows 错误码转换为中文提示
pub fn win_error_message(operation: &str, code: u32) -> String {
    let detail = match code {
        5 => "权限不足，请尝试以管理员身份运行程序",
        53 => "网络路径未找到，请确认已连接内网",
        64 => "网络名称不可用",
        85 => "本地设备名已在使用中",
        86 => "指定的网络密码不正确",
        87 => "参数错误",
        1200 => "指定的设备名称无效",
        1203 => "网络路径未找到",
        1219 => "已存在与该服务器的会话，凭据冲突",
        1326 => "登录失败：用户名或密码错误",
        1327 => "用户账户限制，无法登录",
        1797 => "打印机驱动程序未知",
        1801 => "打印机名称无效",
        1802 => "打印机已存在",
        1803 => "打印机连接失败",
        1804 => "指定的打印机驱动程序已安装",
        1930 => "打印机驱动程序不兼容",
        2 => "系统找不到指定的文件",
        3 => "系统找不到指定的路径",
        _ => "未知错误",
    };
    format!("{operation}失败（错误码 {code}）：{detail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_dir_path() {
        let dir = get_log_dir();
        assert!(dir.ends_with("logs"));
        assert!(dir.to_string_lossy().contains("PrintLink"));
    }

    #[test]
    fn test_network_check_unreachable() {
        // 使用一个不可路由的地址测试超时返回错误
        let start = std::time::Instant::now();
        let result = TcpStream::connect_timeout(
            &"192.0.2.1:445".parse().unwrap(),
            Duration::from_secs(NETWORK_TIMEOUT_SECS),
        );
        let elapsed = start.elapsed();
        assert!(result.is_err());
        assert!(elapsed < Duration::from_secs(NETWORK_TIMEOUT_SECS + 2));
    }

    #[test]
    fn test_win_error_message() {
        let msg = win_error_message("凭据写入", 5);
        assert!(msg.contains("权限不足"));
        assert!(msg.contains("5"));

        let msg = win_error_message("连接", 1326);
        assert!(msg.contains("密码错误"));
    }

    #[test]
    fn test_server_addr_format() {
        use crate::config::DEFAULT_SERVER_ADDR;
        assert_eq!(DEFAULT_SERVER_ADDR, "10.60.254.90");
        let addr = format!("{DEFAULT_SERVER_ADDR}:{SMB_PORT}");
        assert!(addr.parse::<std::net::SocketAddr>().is_ok());
    }
}
