use std::sync::Arc;

use agentdash_spi::{AgentToolResult, ContentPart, Vfs};
use tokio::sync::RwLock;

use crate::vfs::{ResourceRef, build_canvas_mount, parse_mount_uri};

/// Resolve a tool parameter path into a `ResourceRef`.
///
/// Rules:
/// 1. Contains `://` -> split into mount_id and relative path by URI syntax
/// 2. No `://` and the VFS has exactly one mount -> use that mount implicitly
/// 3. Otherwise -> error, require explicit mount prefix
pub fn resolve_uri_path(vfs: &Vfs, path: &str) -> Result<ResourceRef, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path must not be empty".to_string());
    }

    if trimmed.contains("://") {
        return parse_mount_uri(trimmed, vfs);
    }

    if vfs.mounts.len() == 1 {
        let mount_id = vfs.mounts[0].id.clone();
        return Ok(ResourceRef {
            mount_id,
            path: trimmed.to_string(),
        });
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

    pub async fn append_canvas_mount(&self, canvas: &agentdash_domain::canvas::Canvas) {
        let mut guard = self.inner.write().await;
        let mount = build_canvas_mount(canvas);
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
