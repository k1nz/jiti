//! 中英翻译的源语言启发式。
//! 短拉丁词（如 Epoch）交给 DeepL 自动识别时经常被标成 SK，译文原样返回。

pub fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        ('\u{3400}'..='\u{4DBF}').contains(&c) || ('\u{4E00}'..='\u{9FFF}').contains(&c)
    })
}

pub fn has_latin(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_alphabetic())
}

/// 纯汉字 → zh；纯拉丁字母 → en；混排或无法判断则 None（仍可让引擎检测）。
pub fn guess_source(text: &str) -> Option<&'static str> {
    let cjk = has_cjk(text);
    let latin = has_latin(text);
    if cjk && !latin {
        return Some("zh");
    }
    if latin && !cjk {
        return Some("en");
    }
    None
}

pub fn resolve_source(from: Option<&str>, text: &str) -> Option<String> {
    if let Some(from) = from {
        if !from.trim().is_empty() && !from.eq_ignore_ascii_case("auto") {
            return Some(from.to_string());
        }
    }
    guess_source(text).map(str::to_string)
}

pub fn language_name(code: &str) -> String {
    match code.trim().to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" | "zh_cn" => "Simplified Chinese".into(),
        "zh-tw" | "zh-hant" | "zh_tw" => "Traditional Chinese".into(),
        "en" | "en-us" | "en-gb" | "en_us" | "en_gb" => "English".into(),
        "ja" | "jp" => "Japanese".into(),
        "ko" | "kr" => "Korean".into(),
        "de" => "German".into(),
        "fr" => "French".into(),
        "es" => "Spanish".into(),
        "sk" => "Slovak".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_english_not_auto() {
        assert_eq!(guess_source("Epoch"), Some("en"));
        assert_eq!(resolve_source(None, "Epoch").as_deref(), Some("en"));
        assert_eq!(resolve_source(Some("auto"), "Hello").as_deref(), Some("en"));
    }

    #[test]
    fn cjk_is_chinese() {
        assert_eq!(guess_source("你好"), Some("zh"));
        assert_eq!(guess_source("Hello 世界"), None);
    }

    #[test]
    fn explicit_from_wins() {
        assert_eq!(resolve_source(Some("zh"), "Epoch").as_deref(), Some("zh"));
    }

    #[test]
    fn zh_prompt_name_is_chinese_not_code() {
        assert_eq!(language_name("zh"), "Simplified Chinese");
        assert_eq!(language_name("en"), "English");
    }
}
