use std::fmt::Display;

use thiserror::Error;

pub const CANVAS_MOUNT_ID_PREFIX: &str = "cvs-";
pub const CANVAS_MODULE_ID_PREFIX: &str = "canvas:";
pub const CANVAS_PRESENTATION_SCHEME: &str = "canvas";
pub const CANVAS_PROVIDER_ROOT_SCHEME: &str = "canvas-root";

pub const CANVAS_PREVIEW_VIEW_KEY: &str = "preview";
pub const CANVAS_RENDERER_KIND: &str = "canvas";
pub const CANVAS_BIND_DATA_OPERATION_KEY: &str = "canvas.bind_data";
pub const CANVAS_BIND_DATA_ORIGIN: &str = "host_canvas";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CanvasIdentityError {
    #[error("Canvas canvas_mount_id 不能为空")]
    EmptyMountId,
    #[error("Canvas canvas_mount_id 必须使用 `cvs-` 前缀")]
    MissingMountPrefix,
    #[error("Canvas canvas_mount_id 缺少前缀后的稳定标识")]
    EmptyMountSuffix,
    #[error("Canvas canvas_mount_id 不能重复添加 `cvs-` 前缀")]
    RepeatedMountPrefix,
    #[error("Canvas canvas_mount_id 不能包含空白字符")]
    MountIdContainsWhitespace,
    #[error("Canvas canvas_mount_id 不能包含 `/`、`\\` 或 `:`")]
    MountIdContainsPathSeparator,
}

pub fn derive_canvas_mount_id(title: &str) -> String {
    format!("{CANVAS_MOUNT_ID_PREFIX}{}", derive_canvas_slug(title))
}

pub fn normalize_canvas_mount_id(raw: &str) -> Result<String, CanvasIdentityError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CanvasIdentityError::EmptyMountId);
    }
    if !trimmed.starts_with(CANVAS_MOUNT_ID_PREFIX) {
        return Err(CanvasIdentityError::MissingMountPrefix);
    }
    let suffix = &trimmed[CANVAS_MOUNT_ID_PREFIX.len()..];
    if suffix.is_empty() {
        return Err(CanvasIdentityError::EmptyMountSuffix);
    }
    if suffix.starts_with(CANVAS_MOUNT_ID_PREFIX) {
        return Err(CanvasIdentityError::RepeatedMountPrefix);
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(CanvasIdentityError::MountIdContainsWhitespace);
    }
    if trimmed.chars().any(|ch| matches!(ch, '/' | '\\' | ':')) {
        return Err(CanvasIdentityError::MountIdContainsPathSeparator);
    }
    Ok(trimmed.to_string())
}

pub fn canvas_vfs_mount_id(canvas_mount_id: &str) -> String {
    canvas_mount_id.to_string()
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

pub fn canvas_provider_root_ref(canvas_id: impl Display) -> String {
    format!("{CANVAS_PROVIDER_ROOT_SCHEME}://{canvas_id}")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_canvas_mount_id_builds_prefixed_ascii_slug() {
        assert_eq!(
            derive_canvas_mount_id("Demo Dashboard"),
            "cvs-demo-dashboard"
        );
        assert_eq!(derive_canvas_mount_id("  Demo__Board  "), "cvs-demo_board");
        assert_eq!(derive_canvas_mount_id("cvs-demo"), "cvs-demo");
        assert_eq!(derive_canvas_mount_id("数据看板"), "cvs-canvas");
    }

    #[test]
    fn normalize_canvas_mount_id_accepts_normalized_value() {
        assert_eq!(
            normalize_canvas_mount_id("  cvs-dashboard-a  "),
            Ok("cvs-dashboard-a".to_string())
        );
    }

    #[test]
    fn normalize_canvas_mount_id_rejects_invalid_values() {
        assert_eq!(
            normalize_canvas_mount_id(""),
            Err(CanvasIdentityError::EmptyMountId)
        );
        assert_eq!(
            normalize_canvas_mount_id("dashboard-a"),
            Err(CanvasIdentityError::MissingMountPrefix)
        );
        assert_eq!(
            normalize_canvas_mount_id("cvs-"),
            Err(CanvasIdentityError::EmptyMountSuffix)
        );
        assert_eq!(
            normalize_canvas_mount_id("cvs-cvs-dashboard-a"),
            Err(CanvasIdentityError::RepeatedMountPrefix)
        );
        assert_eq!(
            normalize_canvas_mount_id("cvs-dashboard a"),
            Err(CanvasIdentityError::MountIdContainsWhitespace)
        );
        assert_eq!(
            normalize_canvas_mount_id("cvs-dashboard/a"),
            Err(CanvasIdentityError::MountIdContainsPathSeparator)
        );
        assert_eq!(
            normalize_canvas_mount_id("cvs-dashboard\\a"),
            Err(CanvasIdentityError::MountIdContainsPathSeparator)
        );
        assert_eq!(
            normalize_canvas_mount_id("cvs-dashboard:a"),
            Err(CanvasIdentityError::MountIdContainsPathSeparator)
        );
    }

    #[test]
    fn builds_and_parses_module_identity() {
        assert_eq!(
            canvas_module_id("cvs-dashboard-a"),
            "canvas:cvs-dashboard-a"
        );
        assert_eq!(
            parse_canvas_module_id("canvas:cvs-dashboard-a"),
            Some("cvs-dashboard-a")
        );
        assert_eq!(parse_canvas_module_id("ext:demo"), None);
    }

    #[test]
    fn builds_canvas_uris_and_refs() {
        assert_eq!(
            canvas_presentation_uri("cvs-dashboard-a"),
            "canvas://cvs-dashboard-a"
        );
        assert_eq!(canvas_vfs_uri("cvs-dashboard-a", ""), "cvs-dashboard-a://");
        assert_eq!(
            canvas_vfs_uri("cvs-dashboard-a", "/src/main.tsx"),
            "cvs-dashboard-a://src/main.tsx"
        );
        assert_eq!(canvas_vfs_mount_id("cvs-dashboard-a"), "cvs-dashboard-a");
        assert_eq!(
            canvas_provider_root_ref("00000000-0000-0000-0000-000000000001"),
            "canvas-root://00000000-0000-0000-0000-000000000001"
        );
    }
}
