//! Tab-local recovery for unfinished forms. Never persist credentials or server records.
use serde::{de::DeserializeOwned, Serialize};

pub fn key(owner: &str, scope: &str) -> String {
    format!("buildry.draft.v1:{owner}:{scope}")
}

/// Protected pages mount only after AdminApp has resolved the current account.
pub fn account_key(scope: &str) -> Option<String> {
    let user = crate::pages::workspace::USER()?;
    Some(key(&format!("user:{}", user["id"].as_i64()?), scope))
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok().flatten()
}

pub fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    let value = storage()?.get_item(key).ok()??;
    serde_json::from_str(&value).ok()
}

pub fn save<T: Serialize>(key: &str, value: &T) {
    // Unavailable/full storage must never prevent normal form use or submission.
    if let (Some(storage), Ok(value)) = (storage(), serde_json::to_string(value)) {
        let _ = storage.set_item(key, &value);
    }
}

pub fn clear(key: &str) {
    if let Some(storage) = storage() {
        let _ = storage.remove_item(key);
    }
}
