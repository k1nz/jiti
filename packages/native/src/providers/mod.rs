//! 引擎 Provider 层。
//!
//! M0 仅预留模块边界，不实现任何 Provider（llm / deepl / youdao / google / languagetool
//! 在 M1+ 按 §5 实现；所有引擎 HTTP 走 Rust reqwest，WebView 内不对引擎域名 fetch）。