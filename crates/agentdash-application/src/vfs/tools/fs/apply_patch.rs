use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_spi::Vfs;
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::vfs::inline_persistence::InlineContentOverlay;
use crate::vfs::mutation_queue::MutationQueue;
use crate::vfs::service::VfsService;
use crate::vfs::tools::common::SharedRuntimeVfs;
use crate::vfs::{normalize_patch_entry_targets, parse_patch_text, resolve_mount_id};

// ---------------------------------------------------------------------------
// fs_apply_patch — Codex-style description
// ---------------------------------------------------------------------------

const FS_APPLY_PATCH_DESCRIPTION: &str = "\
Apply edits to one or more files using the Codex apply_patch format.\n\
This is NOT a unified diff. Use this tool for all file modifications: \
creating new files, editing existing files, deleting files, and renaming.\n\
\n\
Usage:\n\
- Paths inside the patch can use `mount_id://relative/path` to target a specific mount; \
paths without a prefix fall back to the `mount` parameter or the session default mount.\n\
- ALWAYS read the target file with fs_read before editing, so context lines are accurate.\n\
- To create a new file, use `*** Add File: path` with every content line prefixed by `+`.\n\
- NEVER use unified diff syntax (`---`/`+++`); use only the grammar below.\n\
\n\
Grammar:\n\
  Patch       := \"*** Begin Patch\" NL { FileOp } \"*** End Patch\" NL?\n\
  FileOp      := AddFile | DeleteFile | UpdateFile\n\
  AddFile     := \"*** Add File: \" path NL { \"+\" line NL }\n\
  DeleteFile  := \"*** Delete File: \" path NL\n\
  UpdateFile  := \"*** Update File: \" path NL [ MoveTo ] { Hunk }\n\
  MoveTo      := \"*** Move to: \" newPath NL\n\
  Hunk        := \"@@\" [ header ] NL { HunkLine } [ \"*** End of File\" NL ]\n\
  HunkLine    := (\" \" | \"-\" | \"+\") text NL\n\
\n\
Example:\n\
```\n\
*** Begin Patch\n\
*** Add File: src/util.rs\n\
+pub fn helper() -> &'static str {\n\
+    \"hello\"\n\
+}\n\
*** Update File: src/main.rs\n\
@@ fn main()\n\
 fn main() {\n\
-    println!(\"old\");\n\
+    println!(\"{}\", util::helper());\n\
 }\n\
*** Delete File: obsolete.rs\n\
*** End Patch\n\
```\n\
\n\
Important:\n\
- The patch MUST begin with `*** Begin Patch` and end with `*** End Patch`.\n\
- Context lines (space prefix) must exactly match the current file content.\n\
- Add File content lines must ALL begin with `+`.\n\
- Show ~3 lines of context above and below each change for reliable anchoring.";

// ---------------------------------------------------------------------------
// fs_apply_patch
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct FsApplyPatchTool {
    service: Arc<VfsService>,
    vfs: SharedRuntimeVfs,
    overlay: Option<Arc<InlineContentOverlay>>,
    identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    mutation_queue: MutationQueue,
}
impl FsApplyPatchTool {
    pub fn new(
        service: Arc<VfsService>,
        vfs: SharedRuntimeVfs,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    ) -> Self {
        Self {
            service,
            vfs,
            overlay,
            identity,
            mutation_queue: MutationQueue::default(),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FsApplyPatchParams {
    /// Default mount ID. Paths in the patch that lack a mount prefix will use this mount. If omitted, the session's default mount is used.
    pub mount: Option<String>,
    /// The patch text in Codex apply_patch format. See the tool description for the full grammar and examples. Paths inside the patch may use `mount_id://relative/path` to target a specific mount.
    pub patch: String,
}

#[async_trait]
impl AgentTool for FsApplyPatchTool {
    fn name(&self) -> &str {
        "fs_apply_patch"
    }
    fn description(&self) -> &str {
        FS_APPLY_PATCH_DESCRIPTION
    }
    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<FsApplyPatchParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: FsApplyPatchParams = serde_json::from_value(args)
            .map_err(|e| AgentToolError::InvalidArguments(format!("invalid arguments: {e}")))?;
        let vfs = self.vfs.snapshot().await;
        let mutation_keys =
            fs_apply_patch_mutation_keys(&vfs, params.mount.as_deref(), &params.patch)
                .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;
        let result = self
            .mutation_queue
            .with_locks(
                mutation_keys,
                self.service.apply_patch_multi(
                    &vfs,
                    params.mount.as_deref(),
                    &params.patch,
                    self.overlay.as_ref().map(|arc| arc.as_ref()),
                    self.identity.as_ref(),
                ),
            )
            .await
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;

        let mut lines = Vec::new();
        if !result.added.is_empty() {
            lines.push(format!("added: {}", result.added.join(", ")));
        }
        if !result.modified.is_empty() {
            lines.push(format!("modified: {}", result.modified.join(", ")));
        }
        if !result.deleted.is_empty() {
            lines.push(format!("deleted: {}", result.deleted.join(", ")));
        }
        for err in &result.errors {
            lines.push(format!(
                "error: {}://{} — {}",
                err.mount_id, err.path, err.message
            ));
        }
        if lines.is_empty() {
            lines.push("patch produced no changes.".to_string());
        }
        let is_error = result.added.is_empty()
            && result.modified.is_empty()
            && result.deleted.is_empty()
            && !result.errors.is_empty();
        Ok(AgentToolResult {
            content: vec![ContentPart::text(lines.join("\n"))],
            is_error,
            details: None,
        })
    }
}

fn fs_apply_patch_mutation_keys(
    vfs: &Vfs,
    default_mount_id: Option<&str>,
    patch: &str,
) -> Result<Vec<String>, String> {
    let entries = parse_patch_text(patch).map_err(|e| format!("patch 解析失败: {e}"))?;
    let fallback_mount_id = match default_mount_id {
        Some(id) if !id.trim().is_empty() => id.to_string(),
        _ => resolve_mount_id(vfs, None)?,
    };

    let mut keys = BTreeSet::new();
    for mut entry in entries {
        let targets = normalize_patch_entry_targets(&mut entry, &fallback_mount_id)?;
        keys.insert(format!(
            "{}://{}",
            targets.primary.mount_id, targets.primary.relative_path
        ));
        if let Some(move_target) = targets.move_target {
            keys.insert(format!(
                "{}://{}",
                move_target.mount_id, move_target.relative_path
            ));
        }
    }
    Ok(keys.into_iter().collect())
}

#[cfg(test)]
mod fs_apply_patch_mutation_tests {
    use super::*;
    use agentdash_spi::{Mount, MountCapability, Vfs};

    fn vfs() -> Vfs {
        Vfs {
            mounts: vec![
                Mount {
                    id: "workspace".to_string(),
                    provider: crate::vfs::PROVIDER_RELAY_FS.to_string(),
                    backend_id: "local-dev-1".to_string(),
                    root_ref: "D:\\workspace".to_string(),
                    capabilities: vec![MountCapability::Write],
                    default_write: true,
                    display_name: "workspace".to_string(),
                    metadata: serde_json::Value::Null,
                },
                Mount {
                    id: "canvas".to_string(),
                    provider: crate::vfs::PROVIDER_INLINE_FS.to_string(),
                    backend_id: String::new(),
                    root_ref: "inline://canvas".to_string(),
                    capabilities: vec![MountCapability::Write],
                    default_write: false,
                    display_name: "canvas".to_string(),
                    metadata: serde_json::Value::Null,
                },
            ],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    #[test]
    fn apply_patch_mutation_keys_include_default_mount_and_move_target() {
        let keys = fs_apply_patch_mutation_keys(
            &vfs(),
            None,
            r#"*** Begin Patch
*** Update File: src/old.rs
*** Move to: src/new.rs
@@
 old
*** End Patch"#,
        )
        .expect("keys should parse");

        assert_eq!(
            keys,
            vec!["workspace://src/new.rs", "workspace://src/old.rs"]
        );
    }

    #[test]
    fn apply_patch_mutation_keys_preserve_explicit_mount_prefix() {
        let keys = fs_apply_patch_mutation_keys(
            &vfs(),
            Some("workspace"),
            r#"*** Begin Patch
*** Add File: canvas://src/view.tsx
+export const value = 1;
*** Delete File: src/old.rs
*** End Patch"#,
        )
        .expect("keys should parse");

        assert_eq!(
            keys,
            vec!["canvas://src/view.tsx", "workspace://src/old.rs"]
        );
    }

    #[test]
    fn apply_patch_mutation_keys_normalize_explicit_mount_paths() {
        let keys = fs_apply_patch_mutation_keys(
            &vfs(),
            Some("workspace"),
            r#"*** Begin Patch
*** Update File: workspace://src//old.rs
*** Move to: workspace://src/./new.rs
@@
 old
*** End Patch"#,
        )
        .expect("keys should parse");

        assert_eq!(
            keys,
            vec!["workspace://src/new.rs", "workspace://src/old.rs"]
        );
    }

    #[test]
    fn apply_patch_mutation_keys_reject_cross_mount_move_target() {
        let err = fs_apply_patch_mutation_keys(
            &vfs(),
            Some("workspace"),
            r#"*** Begin Patch
*** Update File: workspace://src/old.rs
*** Move to: canvas://src/new.rs
@@
 old
*** End Patch"#,
        )
        .expect_err("cross-mount move should fail");

        assert!(err.contains("跨 mount move"));
    }
}
