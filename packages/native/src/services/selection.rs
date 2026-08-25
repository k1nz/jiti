//! 选中文本读取（M1 实现，§4.5 降级链：AX/UIA → 剪贴板模拟 → 手动输入）。
//! M0 只预留边界，返回空串占位。

pub fn read_selected_text() -> String {
    String::new()
}