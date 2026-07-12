//! 登录会话只应写入系统安全存储，不应落库或写入明文文件。
//! 原因是数据库和普通文件更容易被备份、拷贝、调试导出或被其他进程直接读取，
//! 而刷新令牌属于长期凭据，泄露后风险明显高于普通业务数据。

use crate::sync::AuthSession;
use keyring::{Entry, Error as KeyringError};

const SERVICE: &str = "com.shelf.reader.sync";
const SESSION_USERNAME: &str = "session";

fn session_entry() -> Result<Entry, String> {
    Entry::new(SERVICE, SESSION_USERNAME).map_err(|err| map_keyring_error("创建系统钥匙串条目失败", err))
}

fn serialize_session(session: &AuthSession) -> Result<String, String> {
    serde_json::to_string(session).map_err(|err| format!("序列化登录会话失败：{err}"))
}

fn deserialize_session(json: &str) -> Result<AuthSession, String> {
    serde_json::from_str(json).map_err(|err| format!("解析登录会话 JSON 失败：{err}"))
}

fn map_keyring_error(context: &str, err: KeyringError) -> String {
    let detail = match err {
        KeyringError::NoEntry => "系统钥匙串中不存在该会话条目".to_string(),
        KeyringError::BadEncoding(_) => "系统钥匙串中的会话数据不是有效 UTF-8".to_string(),
        KeyringError::TooLong(field, limit) => {
            format!("字段 {field} 超出系统钥匙串长度限制（上限 {limit}）")
        }
        KeyringError::Invalid(field, reason) => format!("字段 {field} 无效：{reason}"),
        KeyringError::Ambiguous(_) => "系统钥匙串中存在多个同名会话条目".to_string(),
        KeyringError::NoStorageAccess(inner) => format!("无法访问系统钥匙串：{inner}"),
        KeyringError::PlatformFailure(inner) => format!("系统钥匙串操作失败：{inner}"),
        _ => format!("未知钥匙串错误：{err}"),
    };
    format!("{context}：{detail}")
}

pub(crate) fn save_session(session: &crate::sync::AuthSession) -> Result<(), String> {
    let json = serialize_session(session)?;
    let entry = session_entry()?;
    entry
        .set_password(&json)
        .map_err(|err| map_keyring_error("写入登录会话到系统钥匙串失败", err))
}

pub(crate) fn load_session() -> Result<Option<crate::sync::AuthSession>, String> {
    let entry = session_entry()?;
    let json = match entry.get_password() {
        Ok(json) => json,
        Err(KeyringError::NoEntry) => return Ok(None),
        Err(err) => return Err(map_keyring_error("读取系统钥匙串中的登录会话失败", err)),
    };

    match deserialize_session(&json) {
        Ok(session) => Ok(Some(session)),
        Err(_) => {
            // 钥匙串中的 JSON 已损坏时，按“无会话”处理，并尽力清除脏条目，避免后续重复解析失败。
            let _ = entry.delete_credential();
            Ok(None)
        }
    }
}

pub(crate) fn clear_session() -> Result<(), String> {
    let entry = session_entry()?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(err) => Err(map_keyring_error("清除系统钥匙串中的登录会话失败", err)),
    }
}

#[cfg(test)]
mod tests {
    use super::{deserialize_session, serialize_session};
    use crate::sync::AuthSession;

    #[test]
    fn session_json_roundtrip() {
        let session = AuthSession {
            user_id: "user-123".to_string(),
            access_token: "access-token".to_string(),
            refresh_token: "refresh-token".to_string(),
            expires_at: 1_725_000_000_000,
        };

        let json = serialize_session(&session).expect("会话应能序列化为 JSON");
        let decoded = deserialize_session(&json).expect("会话 JSON 应能反序列化");

        assert_eq!(decoded, session);
    }
}
