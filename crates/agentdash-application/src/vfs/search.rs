use std::collections::BTreeMap;

use crate::runtime::Mount;
use crate::vfs::inline_persistence::InlineContentOverlay;
use crate::vfs::types::runtime_entry_is_binary;
use crate::vfs::{
    ListOptions, MountError, MountOperationContext, MountProviderRegistry, SearchMatch,
};

/// 与 CC GrepTool 一致的 VCS 黑名单（design.md A3：硬编码不可配置）。
const VCS_EXCLUDE_DIRS: &[&str] = &[".git", ".svn", ".hg", ".bzr", ".jj", ".sl"];

/// CC GrepTool 的 `--max-columns 500` 等价：超长 line trim 到 500 字符 + 后缀。
const MAX_LINE_LEN: usize = 500;
const TRUNCATE_SUFFIX: &str = "...(truncated)";

pub struct TextSearchParams<'a> {
    pub mount_id: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub is_regex: bool,
    pub include_glob: Option<&'a str>,
    pub max_results: usize,
    pub context_lines: usize,
    pub overlay: Option<&'a InlineContentOverlay>,
    pub identity: Option<&'a agentdash_spi::platform::auth::AuthIdentity>,
    /// `false` ⇒ smart-case；`true` ⇒ 严格大小写。默认 `true`（与历史行为一致）。
    pub case_sensitive: bool,
    /// `-B` 等价；与 `context_lines` 同时设置时取 `max(before_lines, context_lines)`。
    pub before_lines: usize,
    /// `-A` 等价；与 `context_lines` 同时设置时取 `max(after_lines, context_lines)`。
    pub after_lines: usize,
    /// `true` ⇒ pattern `.` 跨行 + `^/$` 匹配每行（ripgrep multiline）。
    pub multiline: bool,
    /// 输出形态。默认 `Content`。
    pub output_mode: agentdash_spi::platform::mount::SearchOutputMode,
}

/// 路径段（任一 `/` 分隔的中间段）命中 VCS 黑名单 ⇒ 返回 true。
pub(crate) fn is_vcs_path(path: &str) -> bool {
    path.split('/').any(|seg| VCS_EXCLUDE_DIRS.contains(&seg))
}

pub(crate) fn format_search_matches(matches: &[SearchMatch]) -> Vec<String> {
    matches
        .iter()
        .filter(|m| !is_vcs_path(&m.path))
        .map(format_search_match)
        .collect()
}

pub(crate) async fn grep_inline(
    provider_registry: &MountProviderRegistry,
    mount: &Mount,
    base_path: &str,
    params: &TextSearchParams<'_>,
) -> Result<(Vec<String>, bool), MountError> {
    let provider = provider_registry
        .get(&mount.provider)
        .ok_or_else(|| MountError::ProviderNotRegistered(mount.provider.clone()))?;
    let ctx = MountOperationContext {
        identity: params.identity.cloned(),
        ..MountOperationContext::default()
    };
    let full_opts = ListOptions {
        path: String::new(),
        pattern: None,
        recursive: true,
    };
    let full_result = provider.list(mount, &full_opts, &ctx).await?;
    let mut files = BTreeMap::new();
    for entry in full_result.entries {
        if !entry.is_dir {
            if runtime_entry_is_binary(&entry) {
                continue;
            }
            let read_result = provider.read_text(mount, &entry.path, &ctx).await?;
            files.insert(entry.path, read_result.content);
        }
    }
    if let Some(ov) = params.overlay {
        ov.apply_to_files(&mount.id, &mut files).await;
    }

    let re = if params.is_regex {
        let mut builder = regex::RegexBuilder::new(params.query);
        builder
            .case_insensitive(!params.case_sensitive)
            .multi_line(params.multiline)
            .dot_matches_new_line(params.multiline);
        Some(
            builder
                .build()
                .map_err(|e| MountError::OperationFailed(format!("无效正则: {e}")))?,
        )
    } else {
        None
    };

    let glob_matcher = match params.include_glob {
        Some(pat) => Some(
            globset::Glob::new(pat)
                .map_err(|e| MountError::OperationFailed(format!("无效 glob: {e}")))?
                .compile_matcher(),
        ),
        None => None,
    };

    let before = params.before_lines.max(params.context_lines);
    let after = params.after_lines.max(params.context_lines);

    let mut hits = Vec::new();
    let mut truncated = false;

    for (file_path, content) in &files {
        if is_vcs_path(file_path) {
            continue;
        }
        if !file_path.starts_with(base_path.trim_start_matches("./").trim_start_matches('/'))
            && !base_path.is_empty()
            && base_path != "."
        {
            continue;
        }
        if let Some(matcher) = &glob_matcher
            && !matcher.is_match(file_path.as_str())
        {
            continue;
        }
        let lines: Vec<&str> = content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let matched = match &re {
                Some(re) => re.is_match(line),
                None => {
                    if params.case_sensitive {
                        line.contains(params.query)
                    } else {
                        line.to_lowercase().contains(&params.query.to_lowercase())
                    }
                }
            };
            if matched {
                let mut formatted =
                    format!("{}:{}: {}", file_path, idx + 1, trim_long_line(line.trim()));
                if before > 0 || after > 0 {
                    let start = idx.saturating_sub(before);
                    let end = (idx + 1 + after).min(lines.len());
                    if start < idx {
                        let before_lines_fmt: Vec<String> = (start..idx)
                            .map(|i| {
                                format!(
                                    "{}:{}- {}",
                                    file_path,
                                    i + 1,
                                    trim_long_line(lines[i].trim())
                                )
                            })
                            .collect();
                        formatted = format!("{}\n{}", before_lines_fmt.join("\n"), formatted);
                    }
                    if idx + 1 < end {
                        let after_lines_fmt: Vec<String> = (idx + 1..end)
                            .map(|i| {
                                format!(
                                    "{}:{}- {}",
                                    file_path,
                                    i + 1,
                                    trim_long_line(lines[i].trim())
                                )
                            })
                            .collect();
                        formatted = format!("{}\n{}", formatted, after_lines_fmt.join("\n"));
                    }
                }
                hits.push(formatted);
                if hits.len() >= params.max_results {
                    truncated = true;
                    return Ok((hits, truncated));
                }
            }
        }
    }

    Ok((hits, truncated))
}

fn format_search_match(search_match: &SearchMatch) -> String {
    let trimmed = trim_long_line(&search_match.content);
    if let Some(line) = search_match.line {
        format!("{}:{}: {}", search_match.path, line, trimmed)
    } else {
        format!("{}: {}", search_match.path, trimmed)
    }
}

fn trim_long_line(line: &str) -> String {
    if line.chars().count() <= MAX_LINE_LEN {
        line.to_string()
    } else {
        let head: String = line.chars().take(MAX_LINE_LEN).collect();
        format!("{head}{TRUNCATE_SUFFIX}")
    }
}
