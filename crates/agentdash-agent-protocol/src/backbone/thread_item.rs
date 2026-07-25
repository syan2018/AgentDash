//! ThreadItem builder：类型安全地构造 codex `ThreadItem`。
//!
//! ## 背景
//!
//! codex `ThreadItem` 的若干 variant（`CommandExecution`、`FileChange`、
//! `ImageView` 等）字段类型为 `AbsolutePathBuf`，但 codex_app_server_protocol
//! crate 没有 re-export `AbsolutePathBuf`，且其 `Deserialize` 实现要求
//! "已是绝对路径或 thread-local 设了 base"——直接用结构体字面量在外部构造行不通。
//!
//! connector mapper 曾各自用
//! `serde_json::json!(...)` + `from_value` 绕过此限制，导致 hack 散落两处、
//! 状态转换重复实现。
//!
//! 本模块把这层 hack 集中：内部用 serde JSON 中转（且把相对 cwd 自动转成
//! 绝对路径），对外暴露类型安全 builder API。任何 connector 都应该通过这里
//! 构造 ThreadItem，不应再自行拼接 JSON。
//!
//! ## API 形状
//!
//! - 每个有专用语义的 variant 提供独立构造函数
//! - 失败返回 `Result<ThreadItem, ThreadItemBuildError>`，调用方决定是降级到
//!   `dynamic_tool_call` 还是直接传播错误
//! - 文件变更通过 [`FileChangeSpec`] 表达不同子语义，避免调用方接触 codex 内部
//!   `PatchChangeKind` 等类型

use crate::codex_app_server_protocol as codex;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThreadItemBuildError {
    #[error("failed to construct ThreadItem via serde: {0}")]
    Serde(#[from] serde_json::Error),
}

/// 文件变更规格——connector 用此表达 add/delete/edit/rename，builder 内部翻译为
/// codex `FileUpdateChange { path, kind: PatchChangeKind, diff }`。
#[derive(Debug, Clone)]
pub enum FileChangeSpec {
    /// 新建文件。`diff` 通常为空字符串；前端根据 kind 渲染"新建"语义。
    Add { path: String, diff: String },
    /// 删除文件。
    Delete { path: String },
    /// 修改文件，携带 unified diff。
    Edit { path: String, unified_diff: String },
    /// 重命名文件（path → new_path）。
    Rename {
        path: String,
        new_path: String,
        diff: String,
    },
}

/// 构造 [`codex::ThreadItem::CommandExecution`]。
///
/// `cwd` 必须是绝对路径，否则会被自动用 `std::env::current_dir()` 拼接到绝对。
/// 这是因为 codex `AbsolutePathBuf::Deserialize` 拒绝相对路径。
pub fn command_execution(
    id: impl Into<String>,
    command: impl Into<String>,
    cwd: impl AsRef<Path>,
    status: codex::CommandExecutionStatus,
    aggregated_output: Option<String>,
    exit_code: Option<i32>,
) -> Result<codex::ThreadItem, ThreadItemBuildError> {
    let cwd_abs = ensure_absolute(cwd.as_ref());
    let json_val = serde_json::json!({
        "type": "commandExecution",
        "id": id.into(),
        "command": command.into(),
        "cwd": cwd_abs.to_string_lossy(),
        "processId": null,
        "source": "agent",
        "status": status,
        "commandActions": [],
        "aggregatedOutput": aggregated_output,
        "exitCode": exit_code,
        "durationMs": null,
    });
    serde_json::from_value(json_val).map_err(Into::into)
}

/// 构造 [`codex::ThreadItem::FileChange`]。
pub fn file_change(
    id: impl Into<String>,
    changes: Vec<FileChangeSpec>,
    status: codex::PatchApplyStatus,
) -> Result<codex::ThreadItem, ThreadItemBuildError> {
    let change_values: Vec<serde_json::Value> = changes.iter().map(file_change_to_json).collect();
    let json_val = serde_json::json!({
        "type": "fileChange",
        "id": id.into(),
        "changes": change_values,
        "status": status,
    });
    serde_json::from_value(json_val).map_err(Into::into)
}

/// 构造 [`codex::ThreadItem::WebSearch`]。
pub fn web_search(
    id: impl Into<String>,
    query: impl Into<String>,
) -> Result<codex::ThreadItem, ThreadItemBuildError> {
    let json_val = serde_json::json!({
        "type": "webSearch",
        "id": id.into(),
        "query": query.into(),
        "action": null,
    });
    serde_json::from_value(json_val).map_err(Into::into)
}

/// 构造 [`codex::ThreadItem::DynamicToolCall`]——所有未对应专用 variant 的工具
/// 都走这里。`tool` 名建议尽量规范化（如 `Read`/`WebFetch`），让前端二级分发能识别。
pub fn dynamic_tool_call(
    id: impl Into<String>,
    tool: impl Into<String>,
    arguments: serde_json::Value,
    status: codex::DynamicToolCallStatus,
    content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
    success: Option<bool>,
) -> codex::ThreadItem {
    // DynamicToolCall 字段不含 AbsolutePathBuf，可直接构造，不走 JSON。
    codex::ThreadItem::DynamicToolCall {
        id: id.into(),
        namespace: None,
        tool: tool.into(),
        arguments,
        status,
        content_items: content_items.map(Some),
        success: success.map(Some),
        duration_ms: None,
    }
}

/// 构造 [`codex::ThreadItem::ContextCompaction`]。
pub fn context_compaction(id: impl Into<String>) -> codex::ThreadItem {
    codex::ThreadItem::ContextCompaction { id: id.into() }
}

fn ensure_absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        return p.to_path_buf();
    }
    match std::env::current_dir() {
        Ok(cd) => cd.join(p),
        // 极端情况：当前目录不可读。返回 `/p`（POSIX）或 `C:\p`（Windows），
        // 仅为让 AbsolutePathBuf::Deserialize 通过；调用方实际不应依赖该路径。
        Err(_) => {
            #[cfg(windows)]
            {
                PathBuf::from("C:\\").join(p)
            }
            #[cfg(not(windows))]
            {
                PathBuf::from("/").join(p)
            }
        }
    }
}

fn file_change_to_json(change: &FileChangeSpec) -> serde_json::Value {
    match change {
        FileChangeSpec::Edit { path, unified_diff } => serde_json::json!({
            "path": path,
            "kind": { "type": "update", "move_path": null },
            "diff": unified_diff,
        }),
        FileChangeSpec::Add { path, diff } => serde_json::json!({
            "path": path,
            "kind": { "type": "add" },
            "diff": diff,
        }),
        FileChangeSpec::Delete { path } => serde_json::json!({
            "path": path,
            "kind": { "type": "delete" },
            "diff": "",
        }),
        FileChangeSpec::Rename {
            path,
            new_path,
            diff,
        } => serde_json::json!({
            "path": path,
            "kind": { "type": "update", "move_path": new_path },
            "diff": diff,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_execution_with_absolute_cwd_round_trips() {
        let cwd = std::env::current_dir().expect("cwd");
        let item = command_execution(
            "id-1",
            "echo hi",
            &cwd,
            codex::CommandExecutionStatus::Completed,
            Some("hi\n".to_string()),
            Some(0),
        )
        .expect("build");
        match item {
            codex::ThreadItem::CommandExecution {
                command,
                exit_code,
                aggregated_output,
                ..
            } => {
                assert_eq!(command, "echo hi");
                assert_eq!(exit_code, Some(Some(0)));
                assert_eq!(
                    aggregated_output.as_ref().and_then(Option::as_deref),
                    Some("hi\n")
                );
            }
            other => panic!("expected CommandExecution, got {other:?}"),
        }
    }

    #[test]
    fn command_execution_with_relative_cwd_is_resolved() {
        let item = command_execution(
            "id-1",
            "echo hi",
            ".",
            codex::CommandExecutionStatus::Completed,
            None,
            None,
        )
        .expect("build");
        assert!(matches!(item, codex::ThreadItem::CommandExecution { .. }));
    }

    #[test]
    fn file_change_edit_has_update_kind() {
        let item = file_change(
            "id-1",
            vec![FileChangeSpec::Edit {
                path: "src/lib.rs".to_string(),
                unified_diff: "@@ -1 +1 @@\n-a\n+b".to_string(),
            }],
            codex::PatchApplyStatus::Completed,
        )
        .expect("build");
        match item {
            codex::ThreadItem::FileChange { changes, .. } => {
                assert_eq!(changes.len(), 1);
                assert!(matches!(
                    changes[0].kind,
                    codex::PatchChangeKind::Update { move_path: None }
                ));
                assert_eq!(changes[0].diff, "@@ -1 +1 @@\n-a\n+b");
            }
            other => panic!("expected FileChange, got {other:?}"),
        }
    }

    #[test]
    fn file_change_add_uses_add_kind() {
        let item = file_change(
            "id-1",
            vec![FileChangeSpec::Add {
                path: "src/new.rs".to_string(),
                diff: String::new(),
            }],
            codex::PatchApplyStatus::Completed,
        )
        .expect("build");
        match item {
            codex::ThreadItem::FileChange { changes, .. } => {
                assert!(matches!(changes[0].kind, codex::PatchChangeKind::Add));
            }
            other => panic!("expected FileChange, got {other:?}"),
        }
    }

    #[test]
    fn file_change_delete_uses_delete_kind() {
        let item = file_change(
            "id-1",
            vec![FileChangeSpec::Delete {
                path: "src/gone.rs".to_string(),
            }],
            codex::PatchApplyStatus::Completed,
        )
        .expect("build");
        match item {
            codex::ThreadItem::FileChange { changes, .. } => {
                assert!(matches!(changes[0].kind, codex::PatchChangeKind::Delete));
            }
            other => panic!("expected FileChange, got {other:?}"),
        }
    }

    #[test]
    fn file_change_rename_carries_move_path() {
        let item = file_change(
            "id-1",
            vec![FileChangeSpec::Rename {
                path: "src/old.rs".to_string(),
                new_path: "src/new.rs".to_string(),
                diff: String::new(),
            }],
            codex::PatchApplyStatus::Completed,
        )
        .expect("build");
        match item {
            codex::ThreadItem::FileChange { changes, .. } => {
                let move_path = match &changes[0].kind {
                    codex::PatchChangeKind::Update { move_path } => move_path.clone(),
                    other => panic!("expected Update kind, got {other:?}"),
                };
                assert_eq!(move_path.expect("Rename move_path"), "src/new.rs");
            }
            other => panic!("expected FileChange, got {other:?}"),
        }
    }

    #[test]
    fn web_search_round_trips() {
        let item = web_search("id-1", "rust async").expect("build");
        match item {
            codex::ThreadItem::WebSearch { query, .. } => assert_eq!(query, "rust async"),
            other => panic!("expected WebSearch, got {other:?}"),
        }
    }

    #[test]
    fn dynamic_tool_call_preserves_tool_name() {
        let item = dynamic_tool_call(
            "id-1",
            "Read",
            serde_json::json!({ "path": "src/main.rs" }),
            codex::DynamicToolCallStatus::Completed,
            None,
            Some(true),
        );
        match item {
            codex::ThreadItem::DynamicToolCall { tool, .. } => assert_eq!(tool, "Read"),
            other => panic!("expected DynamicToolCall, got {other:?}"),
        }
    }
}
