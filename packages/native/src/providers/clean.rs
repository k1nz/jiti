//! 译文出口清洗：去掉模型包了一层 JSON / 截断残留的括号、替换符和怪异空白。

const JSON_KEYS: &[&str] = &[
    "translation",
    "translated_text",
    "translated",
    "output",
    "result",
    "content",
    "text",
];

pub fn sanitize_translation_output(raw: &str) -> String {
    let mut s = strip_think(raw);
    s = strip_fences(&s);
    s = s.trim().to_string();
    if s.is_empty() {
        return s;
    }
    if let Some(extracted) = extract_json_payload(&s) {
        s = extracted;
    }
    s = strip_wrapping_quotes(&s);
    s = strip_dangling_brackets(&s);
    s = normalize_junk_chars(&s);
    s.trim().to_string()
}

fn strip_think(raw: &str) -> String {
    let mut s = raw.to_string();
    loop {
        let Some(start) = s.find("<think>") else {
            break;
        };
        let close = "</think>";
        if let Some(rel) = s[start..].find(close) {
            s.replace_range(start..start + rel + close.len(), "");
        } else {
            s.replace_range(start.., "");
            break;
        }
    }
    s
}

fn strip_fences(content: &str) -> String {
    let mut s = content.trim().to_string();
    if let Some(rest) = s.strip_prefix("```json") {
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix("```") {
        s = rest.to_string();
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.to_string();
    }
    s.trim().to_string()
}

fn extract_json_payload(s: &str) -> Option<String> {
    let t = s.trim();
    if t.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(t) {
            if let Some(text) = pick_json_text(&value) {
                return Some(text);
            }
        }
        if let Some(text) = scan_json_string_field(t) {
            return Some(text);
        }
        let inner = t
            .trim_start_matches('{')
            .trim_end_matches('}')
            .trim()
            .to_string();
        if !inner.is_empty() && !inner.contains(':') && !inner.contains('"') {
            return Some(inner);
        }
    }
    None
}

fn pick_json_text(value: &serde_json::Value) -> Option<String> {
    let obj = value.as_object()?;
    for key in JSON_KEYS {
        if let Some(text) = obj.get(*key).and_then(|v| v.as_str()) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn scan_json_string_field(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    for key in JSON_KEYS {
        let needle = format!("\"{key}\"");
        let Some(pos) = lower.find(&needle) else {
            continue;
        };
        let after = s[pos + needle.len()..].trim_start();
        let after = after.strip_prefix(':')?.trim_start();
        let Some(rest) = after.strip_prefix('"') else {
            continue;
        };
        let mut escaped = false;
        let mut value = String::new();
        let mut closed = false;
        for c in rest.chars() {
            if escaped {
                value.push(match c {
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                });
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == '"' {
                closed = true;
                break;
            }
            value.push(c);
        }
        let value = value.trim().to_string();
        if !value.is_empty() && (closed || value.chars().count() <= 80) {
            return Some(value);
        }
    }
    None
}

fn strip_wrapping_quotes(s: &str) -> String {
    let t = s.trim();
    let pairs = [
        ('"', '"'),
        ('\'', '\''),
        ('“', '”'),
        ('‘', '’'),
        ('「', '」'),
        ('『', '』'),
        ('`', '`'),
    ];
    for (open, close) in pairs {
        if t.starts_with(open) && t.ends_with(close) && t.len() >= 2 {
            let inner = &t[open.len_utf8()..t.len() - close.len_utf8()];
            if !inner.contains(open) {
                return inner.trim().to_string();
            }
        }
    }
    t.to_string()
}

fn strip_dangling_brackets(s: &str) -> String {
    let mut t = s.trim().to_string();
    loop {
        let Some(last) = t.chars().last() else {
            break;
        };
        let opener = match last {
            '}' => '{',
            ']' => '[',
            ')' => '(',
            _ => break,
        };
        let open = t.chars().filter(|&c| c == opener).count();
        let close = t.chars().filter(|&c| c == last).count();
        if close > open {
            t.pop();
            t = t.trim_end().to_string();
        } else {
            break;
        }
    }
    t
}

fn normalize_junk_chars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_space = false;
    for c in s.chars() {
        if c == '\u{FFFD}' || c == '\u{FFFC}' {
            continue;
        }
        if c == '\n' {
            out.push('\n');
            last_was_space = true;
            continue;
        }
        if c == '\t' {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }
        if c.is_control() {
            continue;
        }
        if c.is_whitespace()
            || matches!(
                c,
                '\u{00A0}'
                    | '\u{2007}'
                    | '\u{2009}'
                    | '\u{200A}'
                    | '\u{200B}'
                    | '\u{202F}'
                    | '\u{2060}'
                    | '\u{3000}'
                    | '\u{FEFF}'
            )
        {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }
        out.push(c);
        last_was_space = false;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_truncated_json_brace() {
        assert_eq!(sanitize_translation_output("Definition}"), "Definition");
        assert_eq!(
            sanitize_translation_output("{\"text\":\"Definition\"}"),
            "Definition"
        );
        assert_eq!(
            sanitize_translation_output("{\"translation\":\"Definition"),
            "Definition"
        );
    }

    #[test]
    fn strips_replacement_and_exotic_spaces() {
        let raw = "Definition\u{202F}\u{202F}\u{00A0}\u{FFFD}\u{FFFD}";
        assert_eq!(sanitize_translation_output(raw), "Definition");
    }

    #[test]
    fn keeps_balanced_code_braces() {
        assert_eq!(
            sanitize_translation_output("int main() { }"),
            "int main() { }"
        );
        assert_eq!(sanitize_translation_output("你好，世界"), "你好，世界");
    }

    #[test]
    fn unwraps_fences_and_quotes() {
        assert_eq!(
            sanitize_translation_output("```\nDefinition\n```"),
            "Definition"
        );
        assert_eq!(sanitize_translation_output("\"Definition\""), "Definition");
    }
}
