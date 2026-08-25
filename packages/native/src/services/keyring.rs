//! API Key 存储（§6.2）：一律 keyring（macOS Keychain / Windows Credential Manager）。
//! WebView 永远拿不到密钥明文（D2），这里也只暴露存在性/读写动作。

use keyring::v1::Entry;

const SERVICE: &str = "com.jiti.app";

fn entry_name(provider: &str) -> String {
    format!("{provider}_api_key")
}

pub fn get_api_key(provider: &str) -> Result<String, keyring::Error> {
    Entry::new(SERVICE, &entry_name(provider))?.get_password()
}

pub fn has_api_key(provider: &str) -> bool {
    get_api_key(provider)
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
}

pub fn set_api_key(provider: &str, api_key: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, &entry_name(provider)).map_err(|e| e.to_string())?;
    entry
        .set_password(api_key)
        .map_err(|e| format!("Keychain 写入失败：{e}"))
}

pub fn delete_api_key(provider: &str) {
    if let Ok(entry) = Entry::new(SERVICE, &entry_name(provider)) {
        let _ = entry.delete_credential();
    }
}
