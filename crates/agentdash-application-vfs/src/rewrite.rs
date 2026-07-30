use std::collections::BTreeSet;
use std::ops::Range;

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

/// 扫描 shell 实际执行区域里的 VFS URI。
///
/// PowerShell here-string 的正文是传给子程序或重定向的数据，其中的 URI 字面量
/// 不属于 shell 路径参数。
pub fn find_shell_mount_uri_candidates(input: &str, mount_ids: &[String]) -> Vec<RewriteCandidate> {
    let data_ranges = powershell_here_string_ranges(input);
    find_mount_uri_candidates(input, mount_ids)
        .into_iter()
        .filter(|candidate| {
            !data_ranges
                .iter()
                .any(|range| range.contains(&candidate.start))
        })
        .collect()
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
    quote_shell_literal(path)
}

#[cfg(windows)]
fn quote_shell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(windows))]
fn quote_shell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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

fn powershell_here_string_ranges(input: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut active: Option<(&'static str, usize)> = None;
    let mut line_start = 0;

    for segment in input.split_inclusive('\n') {
        let line_end = line_start + segment.len();
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let line = line.strip_suffix('\r').unwrap_or(line);

        if let Some((terminator, content_start)) = active {
            if line.trim() == terminator {
                ranges.push(content_start..line_start);
                active = None;
            }
        } else if let Some(terminator) = powershell_here_string_terminator(line) {
            active = Some((terminator, line_end));
        }

        line_start = line_end;
    }

    if let Some((_, content_start)) = active {
        ranges.push(content_start..input.len());
    }
    ranges
}

fn powershell_here_string_terminator(line: &str) -> Option<&'static str> {
    let line = line.trim_end();
    if line.ends_with("@'") {
        Some("'@")
    } else if line.ends_with("@\"") {
        Some("\"@")
    } else {
        None
    }
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
    fn shell_scan_ignores_powershell_here_string_data() {
        let mounts = vec!["main".to_string(), "lifecycle".to_string()];
        let command = "$source = @'\nmain://docs/example.md\nlifecycle://skills/demo\n'@\npython main://scripts/update.py";

        let found = find_shell_mount_uri_candidates(command, &mounts);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].value, "main://scripts/update.py");
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

    #[test]
    fn quotes_paths_for_the_platform_shell() {
        #[cfg(windows)]
        {
            assert_eq!(
                quote_for_shell_path("C:\\A B\\demo.py"),
                "'C:\\A B\\demo.py'"
            );
            assert_eq!(
                quote_for_shell_path("C:\\A'B\\demo.py"),
                "'C:\\A''B\\demo.py'"
            );
        }

        #[cfg(not(windows))]
        {
            assert_eq!(
                quote_for_shell_path("/tmp/a b/demo.py"),
                "'/tmp/a b/demo.py'"
            );
            assert_eq!(
                quote_for_shell_path("/tmp/a'b/demo.py"),
                "'/tmp/a'\\''b/demo.py'"
            );
        }
    }
}
