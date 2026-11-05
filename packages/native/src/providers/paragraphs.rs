//! 把原文拆成可独立翻译的段落，再按原顺序拼回。

/// 空行分隔的段落，用一个空行拼回。
pub fn join_paragraphs(parts: &[String]) -> String {
    parts.join("\n\n")
}

/// 拆出段落。
///
/// 空行是段落边界。以句末标点结束的行也单独成段，用来保住「一段一行」的邮件和网页。
/// 没有句末标点的连续行视为同一句的折行，合并后再译。列表项各自成段。
pub fn split_paragraphs(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut paragraphs = Vec::new();
    let mut buffer: Vec<String> = Vec::new();

    for raw in normalized.split('\n') {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut paragraphs, &mut buffer);
            continue;
        }
        if !buffer.is_empty() && (is_list_item(line) || ends_sentence(buffer.last().unwrap())) {
            flush(&mut paragraphs, &mut buffer);
        }
        buffer.push(line.to_string());
    }
    flush(&mut paragraphs, &mut buffer);
    paragraphs
}

fn flush(paragraphs: &mut Vec<String>, buffer: &mut Vec<String>) {
    if buffer.is_empty() {
        return;
    }
    paragraphs.push(join_wrapped(buffer));
    buffer.clear();
}

fn join_wrapped(lines: &[String]) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 && !cjk_boundary(&out, line) {
            out.push(' ');
        }
        out.push_str(line);
    }
    out
}

/// 汉字折行中间不插空格。
fn cjk_boundary(prev: &str, next: &str) -> bool {
    fn is_cjk(c: char) -> bool {
        matches!(
            c,
            '\u{3040}'..='\u{30FF}'
                | '\u{3400}'..='\u{9FFF}'
                | '\u{AC00}'..='\u{D7AF}'
                | '\u{F900}'..='\u{FAFF}'
        )
    }
    prev.chars().next_back().is_some_and(is_cjk) && next.chars().next().is_some_and(is_cjk)
}

fn ends_sentence(line: &str) -> bool {
    let trimmed = line.trim_end_matches(|c: char| {
        matches!(
            c,
            '"' | '\'' | '”' | '’' | ')' | ']' | '）' | '』' | '」' | '》'
        )
    });
    matches!(
        trimmed.chars().next_back(),
        Some('.' | '!' | '?' | '。' | '！' | '？')
    )
}

fn is_list_item(line: &str) -> bool {
    let line = line.trim_start();
    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with('•') {
        return true;
    }
    let mut chars = line.chars();
    let mut digits = 0;
    loop {
        match chars.next() {
            Some(c) if c.is_ascii_digit() => digits += 1,
            Some('.' | ')' | '、') if digits > 0 => return true,
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_lines_split_paragraphs() {
        assert_eq!(
            split_paragraphs("第一段。\n\n第二段。"),
            vec!["第一段。".to_string(), "第二段。".to_string()]
        );
        assert_eq!(
            join_paragraphs(&split_paragraphs("第一段。\n\n第二段。")),
            "第一段。\n\n第二段。"
        );
    }

    #[test]
    fn sentence_ending_lines_are_separate_paragraphs() {
        assert_eq!(
            split_paragraphs("First paragraph ends here.\nSecond paragraph starts here."),
            vec![
                "First paragraph ends here.".to_string(),
                "Second paragraph starts here.".to_string(),
            ]
        );
    }

    #[test]
    fn wrapped_sentence_stays_one_paragraph() {
        assert_eq!(
            split_paragraphs("This is a long line that\ncontinues here."),
            vec!["This is a long line that continues here.".to_string()]
        );
    }

    #[test]
    fn wrapped_cjk_joins_without_a_space() {
        assert_eq!(
            split_paragraphs("这是一句被\n折行的话。"),
            vec!["这是一句被折行的话。".to_string()]
        );
    }

    #[test]
    fn list_items_are_separate_paragraphs() {
        assert_eq!(
            split_paragraphs("- item one\n- item two\n* third\n• fourth\n1. fifth"),
            vec![
                "- item one".to_string(),
                "- item two".to_string(),
                "* third".to_string(),
                "• fourth".to_string(),
                "1. fifth".to_string(),
            ]
        );
    }

    #[test]
    fn crlf_blank_line_splits() {
        assert_eq!(
            split_paragraphs("第一段。\r\n\r\n第二段。"),
            vec!["第一段。".to_string(), "第二段。".to_string()]
        );
    }
}
