//! 开机自启：真实系统注册状态，不把开关当普通 JSON 布尔。

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use super::prefs::AutostartStatus;

pub fn map_error(err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    if cfg!(target_os = "macos")
        && (lower.contains("/applications")
            || lower.contains("smapp")
            || lower.contains("not permitted")
            || lower.contains("operation not permitted"))
    {
        return macos_install_message().into();
    }
    if err.trim().is_empty() {
        "无法更新开机自启".into()
    } else {
        err.to_string()
    }
}

pub fn macos_install_hint() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        if !installed_in_applications() {
            return Some(macos_install_message().into());
        }
    }
    None
}

fn macos_install_message() -> &'static str {
    "macOS 开机自启需要把 Jiti 放到 /Applications。开发目录启动无法注册。"
}

#[cfg(target_os = "macos")]
fn installed_in_applications() -> bool {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().contains("/Applications/"))
        .unwrap_or(false)
}

pub fn status(app: &AppHandle) -> AutostartStatus {
    let enabled = app.autolaunch().is_enabled().unwrap_or(false);
    AutostartStatus {
        enabled,
        hint: macos_install_hint(),
    }
}

pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|err| map_error(&err.to_string()))?;
    let actual = manager.is_enabled().unwrap_or(enabled);
    if actual != enabled {
        return Err(map_error("autostart state did not change"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_path_errors_become_install_hint() {
        let mapped = map_error("failed: /Applications/Jiti.app not permitted");
        assert!(mapped.contains("/Applications"));
    }

    #[test]
    fn empty_error_gets_fallback() {
        assert_eq!(map_error("   "), "无法更新开机自启");
    }

    #[test]
    fn passthrough_other_errors() {
        assert_eq!(map_error("registry write failed"), "registry write failed");
    }
}
