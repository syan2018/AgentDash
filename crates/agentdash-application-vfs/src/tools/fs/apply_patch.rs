use std::collections::BTreeSet;
use std::sync::Arc;

use agentdash_platform_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_platform_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::inline_persistence::InlineContentOverlay;
use crate::mutation_queue::MutationQueue;
use crate::service::VfsService;
use crate::tools::common::SharedRuntimeVfs;
use crate::{normalize_patch_entry_targets, parse_patch_text};

// ---------------------------------------------------------------------------
// fs_apply_patch — Codex-style description
// ---------------------------------------------------------------------------

const FS_APPLY_PATCH_DESCRIPTION: &str = "\
Apply edits to one or more files using the Codex apply_patch format.\n\
This is NOT a unified diff. Use this tool for all file modifications: \
creating new files, editing existing files, deleting files, and renaming.\n\
\n\
Usage:\n\
- Paths inside the patch MUST use `mount_id://relative/path` to target a specific mount; \
bare paths are rejected.\n\
- ALWAYS read the target file with fs_read before editing, so context lines are accurate.\n\
- To create a new file, use `*** Add File: mount_id://path` with every content line prefixed by `+`.\n\
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
*** Add File: main://src/util.rs\n\
+pub fn helper() -> &'static str {\n\
+    \"hello\"\n\
+}\n\
*** Update File: main://src/main.rs\n\
@@ fn main()\n\
 fn main() {\n\
-    println!(\"old\");\n\
+    println!(\"{}\", util::helper());\n\
 }\n\
*** Delete File: main://obsolete.rs\n\
*** End Patch\n\
```\n\
\n\
Important:\n\
- The patch MUST begin with `*** Begin Patch` and end with `*** End Patch`.\n\
- Every file path in Add/Delete/Update/Move headers must include a mount prefix.\n\
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
    identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
    mutation_queue: MutationQueue,
}
impl FsApplyPatchTool {
    pub fn new(
        service: Arc<VfsService>,
        vfs: SharedRuntimeVfs,
        overlay: Option<Arc<InlineContentOverlay>>,
        identity: Option<agentdash_platform_spi::platform::auth::AuthIdentity>,
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
#[serde(deny_unknown_fields)]
pub struct FsApplyPatchParams {
    /// The patch text in Codex apply_patch format. Every file path inside the patch must use `mount_id://relative/path`.
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
    fn protocol_projector(&self) -> Option<agentdash_agent_types::ToolProtocolProjector> {
        Some(agentdash_agent_types::ToolProtocolProjector::FileChange)
    }
    fn protocol_fixture_id(&self) -> Option<String> {
        Some("main_tool_fs_apply_patch_lifecycle".to_string())
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
        let state = self.vfs.snapshot_state().await;
        let vfs = state.vfs;
        let access_policy = state.access_policy;
        let mutation_keys = fs_apply_patch_mutation_keys(&params.patch)
            .map_err(|e| AgentToolError::ExecutionFailed(e.to_string()))?;
        let result = self
            .mutation_queue
            .with_locks(
                mutation_keys,
                self.service.apply_patch_multi_with_policy(
                    &vfs,
                    Some(&access_policy),
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
            details: Some(apply_patch_protocol_details(&result, &params.patch)),
        })
    }
}

fn apply_patch_protocol_details(
    result: &crate::MultiMountPatchResult,
    patch: &str,
) -> serde_json::Value {
    let parsed_changes = apply_patch_protocol_changes(patch).unwrap_or_default();
    let actual_paths = result
        .added
        .iter()
        .chain(result.modified.iter())
        .chain(result.deleted.iter())
        .collect::<BTreeSet<_>>();
    serde_json::json!({
        "changes": parsed_changes.into_iter().filter(|change| {
            change.get("path").and_then(serde_json::Value::as_str).is_some_and(|path| actual_paths.iter().any(|actual| actual.as_str() == path))
                || change.get("kind").and_then(|kind| kind.get("move_path")).and_then(serde_json::Value::as_str).is_some_and(|path| actual_paths.iter().any(|actual| actual.as_str() == path))
        }).collect::<Vec<_>>(),
        "errors": result.errors.iter().map(|error| serde_json::json!({
            "mount_id": error.mount_id,
            "path": error.path,
            "message": error.message,
        })).collect::<Vec<_>>(),
    })
}

fn apply_patch_protocol_changes(patch: &str) -> Result<Vec<serde_json::Value>, String> {
    let entries = parse_patch_text(patch).map_err(|error| error.to_string())?;
    let diffs = patch_entry_diffs(patch);
    entries
        .into_iter()
        .zip(diffs)
        .map(|(mut entry, diff)| {
            let entry_kind = match &entry {
                crate::PatchEntry::AddFile { .. } => serde_json::json!({"type":"add"}),
                crate::PatchEntry::DeleteFile { .. } => serde_json::json!({"type":"delete"}),
                crate::PatchEntry::UpdateFile { .. } => serde_json::json!({"type":"update"}),
            };
            let targets = normalize_patch_entry_targets(&mut entry)?;
            let kind = if entry_kind["type"] == "update" {
                serde_json::json!({
                    "type":"update",
                    "move_path": targets.move_target.as_ref().map(|target| format!("{}://{}", target.mount_id, target.relative_path)),
                })
            } else {
                entry_kind
            };
            Ok(serde_json::json!({
                "path": format!("{}://{}", targets.primary.mount_id, targets.primary.relative_path),
                "kind": kind,
                "diff": diff,
            }))
        })
        .collect()
}

fn patch_entry_diffs(patch: &str) -> Vec<String> {
    let mut diffs = Vec::new();
    let mut current = Vec::new();
    for line in patch.lines() {
        let starts_entry = line.starts_with("*** Add File: ")
            || line.starts_with("*** Delete File: ")
            || line.starts_with("*** Update File: ");
        if starts_entry && !current.is_empty() {
            diffs.push(current.join("\n"));
            current.clear();
        }
        if starts_entry || !current.is_empty() {
            if line != "*** End Patch" {
                current.push(line.to_string());
            }
        }
    }
    if !current.is_empty() {
        diffs.push(current.join("\n"));
    }
    diffs
}

fn fs_apply_patch_mutation_keys(patch: &str) -> Result<Vec<String>, String> {
    let entries = parse_patch_text(patch).map_err(|e| format!("patch 解析失败: {e}"))?;

    let mut keys = BTreeSet::new();
    for mut entry in entries {
        let targets = normalize_patch_entry_targets(&mut entry)?;
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

    #[test]
    fn apply_patch_owner_details_preserve_actual_changes() {
        let patch = "*** Begin Patch\n*** Add File: main://src/new.rs\n+new\n*** Update File: main://src/lib.rs\n*** Move to: main://src/moved.rs\n@@\n-old\n+new\n*** Delete File: main://src/old.rs\n*** End Patch";
        let details = apply_patch_protocol_details(
            &crate::MultiMountPatchResult {
                added: vec!["main://src/new.rs".into()],
                modified: vec!["main://src/moved.rs".into()],
                deleted: vec!["main://src/old.rs".into()],
                errors: Vec::new(),
            },
            patch,
        );
        assert_eq!(details["changes"].as_array().unwrap().len(), 3);
        assert_eq!(details["changes"][1]["path"], "main://src/lib.rs");
        assert_eq!(
            details["changes"][1]["kind"]["move_path"],
            "main://src/moved.rs"
        );
        for change in details["changes"].as_array().unwrap() {
            let diff = change["diff"].as_str().unwrap();
            let path = change["path"].as_str().unwrap();
            for other in [
                "main://src/new.rs",
                "main://src/lib.rs",
                "main://src/old.rs",
            ] {
                if other != path {
                    assert!(!diff.contains(other), "{path} diff leaked {other}: {diff}");
                }
            }
        }
    }

    #[test]
    fn apply_patch_mutation_keys_reject_bare_paths() {
        let err = fs_apply_patch_mutation_keys(
            r#"*** Begin Patch
*** Update File: src/old.rs
@@
 old
*** End Patch"#,
        )
        .expect_err("bare paths should be rejected");

        assert!(err.contains("缺少 mount 前缀"));
    }

    #[test]
    fn apply_patch_mutation_keys_reject_bare_move_target() {
        let err = fs_apply_patch_mutation_keys(
            r#"*** Begin Patch
*** Update File: workspace://src/old.rs
*** Move to: src/new.rs
@@
 old
*** End Patch"#,
        )
        .expect_err("bare move target should be rejected");

        assert!(err.contains("缺少 mount 前缀"));
    }

    #[test]
    fn apply_patch_mutation_keys_include_explicit_mount_and_move_target() {
        let keys = fs_apply_patch_mutation_keys(
            r#"*** Begin Patch
*** Update File: workspace://src/old.rs
*** Move to: workspace://src/new.rs
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
            r#"*** Begin Patch
*** Add File: cvs-demo://src/view.tsx
+export const value = 1;
*** Delete File: workspace://src/old.rs
*** End Patch"#,
        )
        .expect("keys should parse");

        assert_eq!(
            keys,
            vec!["cvs-demo://src/view.tsx", "workspace://src/old.rs"]
        );
    }

    #[test]
    fn apply_patch_mutation_keys_normalize_explicit_mount_paths() {
        let keys = fs_apply_patch_mutation_keys(
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
            r#"*** Begin Patch
*** Update File: workspace://src/old.rs
*** Move to: cvs-demo://src/new.rs
@@
 old
*** End Patch"#,
        )
        .expect_err("cross-mount move should fail");

        assert!(err.contains("跨 mount move"));
    }
}
