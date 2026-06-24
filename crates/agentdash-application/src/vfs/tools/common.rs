use std::sync::Arc;

use agentdash_spi::{AgentToolResult, ContentPart, Vfs};
use tokio::sync::RwLock;

use crate::vfs::{CanvasMountAccess, ResourceRef, build_canvas_mount, parse_mount_uri};

/// Resolve a tool parameter path into a `ResourceRef`.
///
/// Rules:
/// 1. Contains `://` -> split into mount_id and relative path by URI syntax
/// 2. No `://` and the VFS has a default mount -> use that mount implicitly
/// 3. No `://` and the VFS has exactly one mount -> use that mount implicitly
/// 4. Otherwise -> error, require explicit mount prefix
pub fn resolve_uri_path(vfs: &Vfs, path: &str) -> Result<ResourceRef, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path must not be empty".to_string());
    }

    if trimmed.contains("://") {
        return parse_mount_uri(trimmed, vfs);
    }

    let has_default_mount = vfs
        .default_mount_id
        .as_ref()
        .is_some_and(|default_mount_id| {
            vfs.mounts.iter().any(|mount| &mount.id == default_mount_id)
        });
    if has_default_mount || vfs.mounts.len() == 1 {
        return parse_mount_uri(trimmed, vfs);
    }

    Err(format!(
        "path `{trimmed}` is missing a mount prefix (format: mount_id://path). \
        The current session has {} mount(s); call mounts_list to see available mounts, \
        then use a fully qualified URI.",
        vfs.mounts.len(),
    ))
}

#[derive(Clone)]
pub struct SharedRuntimeVfs {
    inner: Arc<RwLock<Vfs>>,
}

impl SharedRuntimeVfs {
    pub fn new(vfs: Vfs) -> Self {
        Self {
            inner: Arc::new(RwLock::new(vfs)),
        }
    }

    pub async fn snapshot(&self) -> Vfs {
        self.inner.read().await.clone()
    }

    pub async fn replace(&self, vfs: Vfs) {
        let mut guard = self.inner.write().await;
        *guard = vfs;
    }

    pub async fn append_canvas_mount(
        &self,
        canvas: &agentdash_domain::canvas::Canvas,
        access: CanvasMountAccess,
    ) {
        let mut guard = self.inner.write().await;
        let mount = build_canvas_mount(canvas, access);
        guard.mounts.retain(|existing| existing.id != mount.id);
        guard.mounts.push(mount);
    }
}

pub fn ok_text(text: String) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentPart::text(text)],
        is_error: false,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::{Mount, MountCapability};

    fn mount(id: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "memory".to_string(),
            backend_id: String::new(),
            root_ref: format!("memory://{id}"),
            capabilities: vec![MountCapability::Read],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn resolve_uri_path_unqualified_uses_default_mount_when_available() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let resolved = resolve_uri_path(&vfs, ".").expect("resolve");

        assert_eq!(resolved.mount_id, "main");
        assert_eq!(resolved.path, "");
    }

    #[test]
    fn resolve_uri_path_unqualified_normalizes_through_default_mount() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let resolved = resolve_uri_path(&vfs, "./src//lib.rs").expect("resolve");

        assert_eq!(resolved.mount_id, "main");
        assert_eq!(resolved.path, "src/lib.rs");
    }

    #[test]
    fn resolve_uri_path_unqualified_rejects_parent_escape() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let err = resolve_uri_path(&vfs, "../secret").expect_err("escape rejected");

        assert!(err.contains("越界"));
    }

    #[test]
    fn resolve_uri_path_unqualified_rejects_windows_absolute_path() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let err = resolve_uri_path(&vfs, "C:/repo/file.rs").expect_err("absolute path rejected");

        assert!(err.contains("相对于 mount 根目录"));
    }

    #[test]
    fn resolve_uri_path_unqualified_requires_prefix_without_default_or_single_mount() {
        let vfs = Vfs {
            mounts: vec![mount("main"), mount("docs")],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };

        let err = resolve_uri_path(&vfs, ".").expect_err("ambiguous path rejected");

        assert!(err.contains("missing a mount prefix"));
    }
}
