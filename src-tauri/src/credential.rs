//! Windows 系统凭据自动创建模块
//! 程序启动时静默写入打印服务器 SMB 凭据，存在则覆盖，不存在则新建

use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CREDENTIAL_ATTRIBUTEW, CRED_FLAGS,
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_DOMAIN_PASSWORD,
};

use crate::config;
use crate::utils::win_error_message;

/// Tauri 指令：自动创建/更新打印服务器凭据
#[tauri::command]
pub async fn init_print_credential() -> Result<String, String> {
    let cfg = config::load_config();
    write_credential(&cfg.server_addr, &cfg.username, &cfg.password)
}

/// 写入 Windows 网络凭据（幂等：存在则覆盖更新）
pub fn write_credential(target: &str, username: &str, password: &str) -> Result<String, String> {
    let target_w = HSTRING::from(target);
    let username_w = HSTRING::from(username);
    let password_bytes: Vec<u8> = password.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();

    let cred = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_DOMAIN_PASSWORD,
        TargetName: PWSTR(target_w.as_ptr() as *mut u16),
        Comment: PWSTR::null(),
        LastWritten: Default::default(),
        CredentialBlobSize: password_bytes.len() as u32,
        CredentialBlob: password_bytes.as_ptr() as *mut u8,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut::<CREDENTIAL_ATTRIBUTEW>(),
        TargetAlias: PWSTR::null(),
        UserName: PWSTR(username_w.as_ptr() as *mut u16),
    };

    unsafe {
        if CredWriteW(&cred, 0).is_ok() {
            log::info!("凭据写入成功: 目标={target}, 用户={username}");
            Ok(format!("凭据写入成功（{target} / {username}）"))
        } else {
            let code = GetLastError().0;
            log::error!("凭据写入失败: 目标={target}, 错误码={code}");
            Err(credential_error_hint(code))
        }
    }
}

/// 读取凭据并返回用户名（用于验证凭据是否正确写入）
#[allow(dead_code)]
pub fn read_credential_username(target: &str) -> Result<String, String> {
    let target_w = HSTRING::from(target);
    unsafe {
        let mut cred_ptr: *mut CREDENTIALW = std::ptr::null_mut();
        match CredReadW(
            PCWSTR(target_w.as_ptr()),
            CRED_TYPE_DOMAIN_PASSWORD,
            0,
            &mut cred_ptr,
        ) {
            Ok(()) => {
                let cred = &*cred_ptr;
                let username = wide_ptr_to_string(cred.UserName.0);
                CredFree(cred_ptr as *const _);
                Ok(username)
            }
            Err(_) => {
                let code = GetLastError().0;
                if code == ERROR_NOT_FOUND.0 {
                    Err(format!("凭据不存在: {target}"))
                } else {
                    Err(win_error_message("凭据读取", code))
                }
            }
        }
    }
}

/// 删除凭据（测试用）
#[allow(dead_code)]
pub fn delete_credential(target: &str) -> Result<(), String> {
    let target_w = HSTRING::from(target);
    unsafe {
        match CredDeleteW(PCWSTR(target_w.as_ptr()), CRED_TYPE_DOMAIN_PASSWORD, 0) {
            Ok(()) => Ok(()),
            Err(_) => Err(win_error_message("凭据删除", GetLastError().0)),
        }
    }
}

/// 凭据写入失败的友好提示
fn credential_error_hint(code: u32) -> String {
    match code {
        5 => "系统权限不足，请以管理员身份运行程序".to_string(),
        1326 | 86 => "凭据验证失败，请重启程序重试".to_string(),
        _ => format!("凭据创建失败（错误码 {code}），如域策略禁止修改凭据请联系运维"),
    }
}

/// 将 PCWSTR 宽字符指针转为 Rust String
pub(crate) fn wide_ptr_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TARGET: &str = "PrintLinkUnitTest";

    #[test]
    fn test_credential_write_and_read_back() {
        // 写入凭据
        let result = write_credential(TEST_TARGET, "testuser", "testpass123");
        assert!(result.is_ok(), "凭据写入应成功: {:?}", result.err());

        // 读回验证用户名
        let username = read_credential_username(TEST_TARGET);
        assert!(username.is_ok(), "凭据读取应成功: {:?}", username.err());
        assert_eq!(username.unwrap(), "testuser");

        // 清理
        let _ = delete_credential(TEST_TARGET);
    }

    #[test]
    fn test_credential_write_idempotent() {
        // 重复写入不应报错（覆盖模式）
        let r1 = write_credential(TEST_TARGET, "user_a", "pass_a");
        assert!(r1.is_ok());
        let r2 = write_credential(TEST_TARGET, "user_b", "pass_b");
        assert!(r2.is_ok(), "重复写入应幂等成功: {:?}", r2.err());

        let username = read_credential_username(TEST_TARGET).unwrap();
        assert_eq!(username, "user_b", "覆盖后应为新用户名");

        let _ = delete_credential(TEST_TARGET);
    }

    #[test]
    fn test_credential_not_found() {
        let result = read_credential_username("NonExistentTarget_XYZ_12345");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不存在"));
    }

    #[test]
    fn test_credential_error_hint() {
        assert!(credential_error_hint(5).contains("管理员"));
        assert!(credential_error_hint(1326).contains("凭据验证失败"));
        assert!(credential_error_hint(9999).contains("错误码 9999"));
    }
}
