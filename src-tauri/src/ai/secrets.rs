//! 密钥只进系统钥匙串，不写配置文件。

use crate::error::{AppError, Result};

const SERVICE: &str = "com.nomile.app.ai";

fn entry(provider_id: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, provider_id)
        .map_err(|e| AppError::Message(format!("系统钥匙串不可用：{e}")))
}

pub fn get(provider_id: &str) -> Option<String> {
    entry(provider_id).ok()?.get_password().ok()
}

pub fn has(provider_id: &str) -> bool {
    get(provider_id).is_some()
}

pub fn set(provider_id: &str, key: &str) -> Result<()> {
    entry(provider_id)?
        .set_password(key)
        .map_err(|e| AppError::Message(format!("保存密钥失败：{e}")))
}

pub fn delete(provider_id: &str) {
    if let Ok(entry) = entry(provider_id) {
        let _ = entry.delete_credential();
    }
}
