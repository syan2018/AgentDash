use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteCandidate {
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub quoted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteReplacement {
    pub start: usize,
    pub end: usize,
    pub value: String,
}

/// 扫描字符串里显式出现的 `mount_id://...` 引用。
///
/// 这里不尝试解析 shell AST，只做 session mount URI 的明确识别。调用方负责
/// 对命中的 URI 做 VFS 解析、权限校验和物化。
pub fn find_mount_uri_candidates(input: &str, mount_ids: &[String]) -> Vec<RewriteCandidate> {
    let mount_ids = mount_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let bytes = input.as_bytes();
    let mut candidates = Vec::new();
    let mut index = 0;

    while let Some(offset) = input[index..].find("://") {
        let sep = index + offset;
        let start = mount_start(input, sep);
        if start == sep {
            index = sep + 3;
            continue;
        }
        let mount_id = &input[start..sep];
        if !mount_ids.contains(mount_id) {
            index = sep + 3;
            continue;
        }

        let mut end = sep + 3;
        while end < input.len() {
            let ch = input[end..].chars().next().expect("valid char boundary");
            if is_uri_delimiter(ch) {
                break;
            }
            end += ch.len_utf8();
        }
        if end == sep + 3 {
            index = end;
            continue;
        }

        let quoted = start > 0
            && end < input.len()
            && ((bytes[start - 1] == b'"' && bytes[end] == b'"')
                || (bytes[start - 1] == b'\'' && bytes[end] == b'\''));
        candidates.push(RewriteCandidate {
            value: input[start..end].to_string(),
            start,
            end,
            quoted,
        });
        index = end;
    }

    candidates
}

pub fn apply_replacements(input: &str, replacements: &[RewriteReplacement]) -> String {
    let mut output = input.to_string();
    let mut sorted = replacements.to_vec();
    sorted.sort_by_key(|replacement| replacement.start);
    for replacement in sorted.into_iter().rev() {
        output.replace_range(replacement.start..replacement.end, &replacement.value);
    }
    output
}

pub fn quote_for_shell_path(path: &str) -> String {
    format!("\"{}\"", path.replace('"', "\\\""))
}

fn mount_start(input: &str, sep: usize) -> usize {
    let mut start = sep;
    for (idx, ch) in input[..sep].char_indices().rev() {
        if is_mount_id_char(ch) {
            start = idx;
        } else {
            break;
        }
    }
    start
}

fn is_mount_id_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn is_uri_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '"' | '\'' | '`' | '<' | '>' | '|' | ';' | '&' | '(' | ')' | '[' | ']' | '{' | '}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_only_known_mount_uris() {
        let mounts = vec!["skill-assets".to_string(), "lifecycle".to_string()];
        let found = find_mount_uri_candidates(
            "cat skill-assets://skills/foo/scripts/check.sh http://example.test lifecycle://a/b",
            &mounts,
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].value, "skill-assets://skills/foo/scripts/check.sh");
        assert_eq!(found[1].value, "lifecycle://a/b");
    }

    #[test]
    fn detects_existing_quotes() {
        let mounts = vec!["skill-assets".to_string()];
        let found =
            find_mount_uri_candidates("cat \"skill-assets://skills/foo/SKILL.md\"", &mounts);
        assert_eq!(found.len(), 1);
        assert!(found[0].quoted);
    }

    #[test]
    fn applies_replacements_from_original_offsets() {
        let rewritten = apply_replacements(
            "cat a://one b://two",
            &[
                RewriteReplacement {
                    start: 4,
                    end: 11,
                    value: "\"/tmp/one\"".to_string(),
                },
                RewriteReplacement {
                    start: 12,
                    end: 19,
                    value: "\"/tmp/two\"".to_string(),
                },
            ],
        );
        assert_eq!(rewritten, "cat \"/tmp/one\" \"/tmp/two\"");
    }
}
