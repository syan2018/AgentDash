use agentdash_domain::DomainError;
use agentdash_domain::canvas::Canvas;

pub const CANVAS_MOUNT_ID_PREFIX: &str = "cvs-";
pub const CANVAS_MODULE_ID_PREFIX: &str = "canvas:";
pub const CANVAS_PRESENTATION_SCHEME: &str = "canvas";
pub const CANVAS_PROVIDER_ROOT_SCHEME: &str = "canvas-root";

pub fn derive_canvas_mount_id(title: &str) -> String {
    format!("{CANVAS_MOUNT_ID_PREFIX}{}", derive_canvas_slug(title))
}

pub fn normalize_canvas_mount_id(raw: &str) -> Result<String, DomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidConfig(
            "Canvas canvas_mount_id 不能为空".to_string(),
        ));
    }
    if !trimmed.starts_with(CANVAS_MOUNT_ID_PREFIX) {
        return Err(DomainError::InvalidConfig(format!(
            "Canvas canvas_mount_id 必须使用 `{CANVAS_MOUNT_ID_PREFIX}` 前缀"
        )));
    }
    let suffix = &trimmed[CANVAS_MOUNT_ID_PREFIX.len()..];
    if suffix.is_empty() {
        return Err(DomainError::InvalidConfig(
            "Canvas canvas_mount_id 缺少前缀后的稳定标识".to_string(),
        ));
    }
    if suffix.starts_with(CANVAS_MOUNT_ID_PREFIX) {
        return Err(DomainError::InvalidConfig(
            "Canvas canvas_mount_id 不能重复添加 `cvs-` 前缀".to_string(),
        ));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(DomainError::InvalidConfig(
            "Canvas canvas_mount_id 不能包含空白字符".to_string(),
        ));
    }
    if trimmed.chars().any(|ch| matches!(ch, '/' | '\\' | ':')) {
        return Err(DomainError::InvalidConfig(
            "Canvas canvas_mount_id 不能包含 `/`、`\\` 或 `:`".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn canvas_vfs_mount_id(canvas: &Canvas) -> String {
    normalize_canvas_mount_id(&canvas.mount_id)
        .expect("Canvas canvas_mount_id must be normalized before VFS projection")
}

pub fn canvas_module_id(canvas_mount_id: &str) -> String {
    format!("{CANVAS_MODULE_ID_PREFIX}{canvas_mount_id}")
}

pub fn parse_canvas_module_id(module_id: &str) -> Option<&str> {
    module_id.strip_prefix(CANVAS_MODULE_ID_PREFIX)
}

pub fn canvas_presentation_uri(canvas_mount_id: &str) -> String {
    format!("{CANVAS_PRESENTATION_SCHEME}://{canvas_mount_id}")
}

pub fn canvas_vfs_uri(canvas_mount_id: &str, path: &str) -> String {
    if path.trim().is_empty() {
        format!("{canvas_mount_id}://")
    } else {
        format!("{canvas_mount_id}://{}", path.trim_start_matches('/'))
    }
}

pub fn canvas_provider_root_ref(canvas: &Canvas) -> String {
    format!("{CANVAS_PROVIDER_ROOT_SCHEME}://{}", canvas.id)
}

fn derive_canvas_slug(title: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;
    for ch in title.trim().chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, '-' | '_') {
            Some(ch)
        } else if ch.is_whitespace() {
            Some('-')
        } else {
            None
        };

        let Some(next) = mapped else {
            continue;
        };

        if matches!(next, '-' | '_') {
            if out.is_empty() || last_was_sep {
                continue;
            }
            last_was_sep = true;
            out.push(next);
            continue;
        }

        last_was_sep = false;
        out.push(next);
    }

    let normalized = out.trim_matches(['-', '_']).to_string();
    let normalized = normalized
        .strip_prefix(CANVAS_MOUNT_ID_PREFIX)
        .unwrap_or(&normalized)
        .trim_matches(['-', '_'])
        .to_string();
    if normalized.is_empty() {
        "canvas".to_string()
    } else {
        normalized
    }
}
