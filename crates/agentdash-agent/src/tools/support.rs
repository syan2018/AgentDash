use std::path::{Component, Path, PathBuf};

use anyhow::{Result, anyhow};

fn is_workspace_absolute_input(raw_input: &str, path: &Path) -> bool {
    if path.is_absolute() {
        return true;
    }

    if raw_input.starts_with("\\\\?\\") || raw_input.starts_with("//?/") {
        return true;
    }

    let bytes = raw_input.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

pub fn normalize_relative_path(input: &str) -> Result<PathBuf> {
    let path = Path::new(input);
    if is_workspace_absolute_input(input, path) {
        return Ok(path.to_path_buf());
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(anyhow!("路径越界：不允许访问工作空间之外的路径"));
                }
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(anyhow!("路径必须是相对于工作空间根目录的相对路径"));
            }
        }
    }

    Ok(normalized)
}

pub fn workspace_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .and_then(|rel| {
            if rel.as_os_str().is_empty() {
                None
            } else {
                Some(rel.to_string_lossy().replace('\\', "/"))
            }
        })
        .unwrap_or_else(|| ".".to_string())
}

pub fn resolve_existing_path(root: &Path, input: &str) -> Result<PathBuf> {
    let canonical_root = root.canonicalize()?;
    let raw_input = Path::new(input);
    let candidate = if is_workspace_absolute_input(input, raw_input) {
        raw_input.to_path_buf()
    } else {
        let normalized = normalize_relative_path(input)?;
        root.join(normalized)
    };

    if !candidate.exists() {
        return Err(anyhow!("目标不存在: {}", candidate.display()));
    }

    let canonical_candidate = candidate.canonicalize()?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(anyhow!("路径越界：{}", input));
    }
    Ok(canonical_candidate)
}

pub fn resolve_path_for_write(root: &Path, input: &str) -> Result<PathBuf> {
    let normalized = normalize_relative_path(input)?;
    let candidate = root.join(normalized);

    let parent = candidate
        .parent()
        .ok_or_else(|| anyhow!("无效路径：{}", candidate.display()))?;
    std::fs::create_dir_all(parent)?;

    let canonical_root = root.canonicalize()?;
    let canonical_parent = parent.canonicalize()?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(anyhow!("路径越界：{}", input));
    }

    Ok(candidate)
}

pub fn truncate_chars(input: &str, max_chars: usize) -> (String, bool) {
    let total = input.chars().count();
    if total <= max_chars {
        return (input.to_string(), false);
    }

    let truncated = input.chars().take(max_chars).collect::<String>();
    (truncated, true)
}
