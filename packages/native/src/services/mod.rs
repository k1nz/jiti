//! 服务层：窗口/焦点/材质（panel）、热键、托盘、选择读取、
//! 密钥（secrets / keys.json）、非密钥配置（settings）、历史库（history）、出网（transport）。

pub mod dev;
pub mod history;
pub mod hotkeys;
pub mod panel;
pub mod secrets;
pub mod permissions;
pub mod selection;
pub mod settings;
pub mod transport;
pub mod tray;
