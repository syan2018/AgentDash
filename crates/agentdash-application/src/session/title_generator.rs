//! 会话标题派生
//!
//! 会话列表标题只需要稳定、可预期地概括首条用户消息，不参与 Agent 执行链路。

const MAX_TITLE_CHARS: usize = 22;

/// 根据首条用户 prompt 派生会话标题。
pub fn derive_session_title(user_prompt: &str) -> Option<String> {
    user_prompt
        .lines()
        .filter_map(clean_title_candidate)
        .find(|title| !title.is_empty())
}

fn clean_title_candidate(line: &str) -> Option<String> {
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let stripped = strip_leading_markers(normalized.trim());
    let trimmed = trim_title_punctuation(stripped);
    if trimmed.is_empty() {
        return None;
    }

    let mut title = trimmed.chars().take(MAX_TITLE_CHARS).collect::<String>();
    title = trim_title_punctuation(&title).to_string();
    (!title.is_empty()).then_some(title)
}

fn strip_leading_markers(mut text: &str) -> &str {
    loop {
        let current = text.trim_start();
        if current.is_empty() {
            return current;
        }

        if let Some(rest) = current.strip_prefix("- [ ]") {
            text = rest;
            continue;
        }
        if let Some(rest) = current
            .strip_prefix("- [x]")
            .or_else(|| current.strip_prefix("- [X]"))
        {
            text = rest;
            continue;
        }
        if let Some(rest) = current
            .strip_prefix('#')
            .or_else(|| current.strip_prefix('>'))
            .or_else(|| current.strip_prefix('-'))
            .or_else(|| current.strip_prefix('*'))
            .or_else(|| current.strip_prefix('+'))
        {
            text = rest;
            continue;
        }

        if let Some(rest) = strip_ordered_list_marker(current) {
            text = rest;
            continue;
        }

        return current;
    }
}

fn strip_ordered_list_marker(text: &str) -> Option<&str> {
    let digit_count = text
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return None;
    }

    let rest = &text[text
        .char_indices()
        .nth(digit_count)
        .map(|(index, _)| index)
        .unwrap_or(text.len())..];
    if rest.starts_with('.') || rest.starts_with(')') || rest.starts_with('、') {
        return Some(&rest[rest.chars().next().map(char::len_utf8).unwrap_or(0)..]);
    }
    None
}

fn trim_title_punctuation(text: &str) -> &str {
    text.trim_matches(|character: char| {
        character.is_whitespace()
            || matches!(
                character,
                '"' | '\''
                    | '`'
                    | '“'
                    | '”'
                    | '‘'
                    | '’'
                    | '「'
                    | '」'
                    | '『'
                    | '』'
                    | '《'
                    | '》'
                    | '('
                    | ')'
                    | '（'
                    | '）'
                    | '['
                    | ']'
                    | '【'
                    | '】'
                    | '.'
                    | '。'
                    | ','
                    | '，'
                    | ':'
                    | '：'
                    | ';'
                    | '；'
                    | '!'
                    | '！'
                    | '?'
                    | '？'
                    | '、'
            )
    })
}

#[cfg(test)]
mod tests {
    use super::derive_session_title;

    #[test]
    fn derives_title_from_first_meaningful_line() {
        let title = derive_session_title("\n\n## 帮我修复会话标题生成失败的问题\n后续内容");

        assert_eq!(title.as_deref(), Some("帮我修复会话标题生成失败的问题"));
    }

    #[test]
    fn strips_list_markers_and_truncates() {
        let title =
            derive_session_title("1. 请帮我 review provider 相关的自动标题生成逻辑为什么不稳定");

        assert_eq!(title.as_deref(), Some("请帮我 review provider 相关"));
    }

    #[test]
    fn ignores_empty_marker_lines() {
        let title = derive_session_title("-\n- 实现本地 session 标题派生");

        assert_eq!(title.as_deref(), Some("实现本地 session 标题派生"));
    }
}
