//! 服务层：窗口/焦点/材质（panel）、热键、托盘、选择读取、
//! 密钥（secrets / keys.json）、非密钥配置（settings）、通用偏好（prefs）、
//! 设置窗口、开机自启、历史库（history）、错题本（mistakes）、出网（transport）。

pub mod autostart;
pub mod crash;
pub mod database;
pub mod dev;
pub mod history;
pub mod hotkeys;
pub mod mistakes;
pub mod panel;
pub mod permissions;
pub mod prefs;
pub mod secrets;
pub mod selection;
pub mod settings;
pub mod settings_window;
pub mod transport;
pub mod tray;
