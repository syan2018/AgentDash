use agentdash_agent_protocol::codex_app_server_protocol as codex;
use agentdash_agent_protocol::{
    AgentDashNativeThreadItem, AgentDashThreadItem, ShellExecExecutionMode, ToolProtocolProjector,
    backbone::thread_item,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ToolPresentationError {
    #[error("tool `{tool}` cannot be projected as {family}: {reason}")]
    Invalid {
        tool: String,
        family: &'static str,
        reason: String,
    },
}

pub(crate) struct ToolPresentationResult<'a> {
    pub content: &'a str,
    pub is_error: bool,
}

pub(crate) fn project_tool_item(
    item_id: &str,
    tool_name: &str,
    arguments: serde_json::Value,
    projector: &ToolProtocolProjector,
    in_progress: bool,
    failed: bool,
    result: Option<ToolPresentationResult<'_>>,
) -> Result<AgentDashThreadItem, ToolPresentationError> {
    let status = if in_progress {
        codex::DynamicToolCallStatus::InProgress
    } else if failed || result.as_ref().is_some_and(|result| result.is_error) {
        codex::DynamicToolCallStatus::Failed
    } else {
        codex::DynamicToolCallStatus::Completed
    };
    let content_items = result.as_ref().map(|result| {
        vec![codex::DynamicToolCallOutputContentItem::InputText {
            text: result.content.to_owned(),
        }]
    });
    let success = result.as_ref().map(|result| !result.is_error);

    match projector {
        ToolProtocolProjector::Command => {
            let command = required_string(&arguments, "command", tool_name, "command")?;
            let raw_cwd = string_arg(&arguments, "cwd")
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            let (cwd, execution_mode) = match raw_cwd {
                None => (
                    Some("platform://".to_owned()),
                    ShellExecExecutionMode::Platform,
                ),
                Some(cwd) if cwd.starts_with("platform://") => {
                    (Some(cwd), ShellExecExecutionMode::Platform)
                }
                Some(cwd) => (Some(cwd), ShellExecExecutionMode::MountExec),
            };
            let exit_code = result
                .as_ref()
                .and_then(|result| parse_result_json(result.content))
                .and_then(|output| integer_field(&output, &["exit_code", "exitCode"]))
                .and_then(|value| i32::try_from(value).ok());
            Ok(AgentDashNativeThreadItem::ShellExec {
                id: item_id.to_owned(),
                command,
                cwd,
                execution_mode,
                arguments,
                status,
                aggregated_output: result.map(|result| result.content.to_owned()),
                exit_code,
                success,
            }
            .into())
        }
        ToolProtocolProjector::FileChange => {
            let patch = required_string(&arguments, "patch", tool_name, "file_change")?;
            let changes = parse_apply_patch_specs(&patch).map_err(|reason| {
                ToolPresentationError::Invalid {
                    tool: tool_name.to_owned(),
                    family: "file_change",
                    reason,
                }
            })?;
            let patch_status = match status {
                codex::DynamicToolCallStatus::InProgress => codex::PatchApplyStatus::InProgress,
                codex::DynamicToolCallStatus::Completed => codex::PatchApplyStatus::Completed,
                codex::DynamicToolCallStatus::Failed => codex::PatchApplyStatus::Failed,
            };
            thread_item::file_change(item_id, changes, patch_status)
                .map(Into::into)
                .map_err(|error| ToolPresentationError::Invalid {
                    tool: tool_name.to_owned(),
                    family: "file_change",
                    reason: error.to_string(),
                })
        }
        ToolProtocolProjector::FsRead => Ok(AgentDashNativeThreadItem::FsRead {
            id: item_id.to_owned(),
            path: required_string(&arguments, "path", tool_name, "fs_read")?,
            offset: usize_arg(&arguments, "offset"),
            limit: usize_arg(&arguments, "limit"),
            arguments,
            status,
            content_items,
            success,
        }
        .into()),
        ToolProtocolProjector::FsGrep => Ok(AgentDashNativeThreadItem::FsGrep {
            id: item_id.to_owned(),
            pattern: required_string(&arguments, "pattern", tool_name, "fs_grep")?,
            path: string_arg(&arguments, "path"),
            glob: string_arg(&arguments, "glob"),
            file_type: string_arg(&arguments, "type"),
            output_mode: string_arg(&arguments, "output_mode"),
            head_limit: usize_arg(&arguments, "head_limit"),
            offset: usize_arg(&arguments, "offset"),
            arguments,
            status,
            content_items,
            success,
        }
        .into()),
        ToolProtocolProjector::FsGlob => Ok(AgentDashNativeThreadItem::FsGlob {
            id: item_id.to_owned(),
            pattern: required_string(&arguments, "pattern", tool_name, "fs_glob")?,
            path: string_arg(&arguments, "path"),
            max_results: usize_arg(&arguments, "max_results")
                .or_else(|| usize_arg(&arguments, "maxResults")),
            arguments,
            status,
            content_items,
            success,
        }
        .into()),
        ToolProtocolProjector::Mcp { server_key } => {
            serde_json::from_value::<codex::ThreadItem>(serde_json::json!({
                "type": "mcpToolCall",
                "id": item_id,
                "server": server_key,
                "tool": tool_name,
                "arguments": arguments,
                "status": status,
                "result": result.as_ref().filter(|result| !result.is_error).map(|result| {
                    serde_json::json!({"content": [{"type": "text", "text": result.content}]})
                }),
                "error": result.as_ref().filter(|result| result.is_error).map(|result| {
                    serde_json::json!({"message": result.content})
                }),
            }))
            .map(Into::into)
            .map_err(|error| ToolPresentationError::Invalid {
                tool: tool_name.to_owned(),
                family: "mcp",
                reason: error.to_string(),
            })
        }
        ToolProtocolProjector::Dynamic { namespace } => Ok(codex::ThreadItem::DynamicToolCall {
            id: item_id.to_owned(),
            namespace: Some(namespace.clone()),
            tool: tool_name.to_owned(),
            arguments,
            status,
            content_items: Some(content_items),
            success: Some(success),
            duration_ms: None,
        }
        .into()),
    }
}

fn required_string(
    arguments: &serde_json::Value,
    key: &'static str,
    tool: &str,
    family: &'static str,
) -> Result<String, ToolPresentationError> {
    string_arg(arguments, key).ok_or_else(|| ToolPresentationError::Invalid {
        tool: tool.to_owned(),
        family,
        reason: format!("typed arguments require a string `{key}` field"),
    })
}

fn string_arg(arguments: &serde_json::Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn usize_arg(arguments: &serde_json::Value, key: &str) -> Option<usize> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn parse_result_json(content: &str) -> Option<serde_json::Value> {
    serde_json::from_str(content).ok()
}

fn integer_field(value: &serde_json::Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_i64))
}

fn parse_apply_patch_specs(patch: &str) -> Result<Vec<thread_item::FileChangeSpec>, String> {
    let lines: Vec<&str> = patch.lines().collect();
    let mut index = lines
        .iter()
        .position(|line| line.trim_end() == "*** Begin Patch")
        .ok_or_else(|| "missing begin marker".to_owned())?
        + 1;
    let mut specs = Vec::new();

    while index < lines.len() {
        let line = lines[index].trim_end();
        if line == "*** End Patch" {
            break;
        }
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            index += 1;
            let mut diff_lines = Vec::new();
            while index < lines.len() && !is_apply_patch_file_op_or_end(lines[index].trim_end()) {
                let next = lines[index].trim_end();
                if next != "*** End of File" {
                    diff_lines.push(next.to_owned());
                }
                index += 1;
            }
            specs.push(thread_item::FileChangeSpec::Add {
                path: path.to_owned(),
                diff: diff_lines.join("\n"),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            specs.push(thread_item::FileChangeSpec::Delete {
                path: path.to_owned(),
            });
            index += 1;
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            index += 1;
            let mut move_path = None;
            let mut diff_lines = Vec::new();
            while index < lines.len() && !is_apply_patch_file_op_or_end(lines[index].trim_end()) {
                let next = lines[index].trim_end();
                if let Some(target) = next.strip_prefix("*** Move to: ") {
                    move_path = Some(target.to_owned());
                } else if next != "*** End of File" {
                    diff_lines.push(next.to_owned());
                }
                index += 1;
            }
            let diff = diff_lines.join("\n");
            specs.push(match move_path {
                Some(new_path) => thread_item::FileChangeSpec::Rename {
                    path: path.to_owned(),
                    new_path,
                    diff,
                },
                None => thread_item::FileChangeSpec::Edit {
                    path: path.to_owned(),
                    unified_diff: diff,
                },
            });
            continue;
        }
        index += 1;
    }

    if specs.is_empty() {
        return Err("apply-patch payload contains no file changes".to_owned());
    }
    Ok(specs)
}

fn is_apply_patch_file_op_or_end(line: &str) -> bool {
    line == "*** End Patch"
        || line.starts_with("*** Add File: ")
        || line.starts_with("*** Delete File: ")
        || line.starts_with("*** Update File: ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_declared_fs_read_projects_native_thread_item() {
        let item = project_tool_item(
            "tool-1",
            "renamed_runtime_reader",
            serde_json::json!({"path": "main://README.md", "offset": 2, "limit": 8}),
            &ToolProtocolProjector::FsRead,
            false,
            false,
            Some(ToolPresentationResult {
                content: "hello",
                is_error: false,
            }),
        )
        .expect("project fs read");

        assert!(matches!(
            item,
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::FsRead {
                path,
                offset: Some(2),
                limit: Some(8),
                ..
            }) if path == "main://README.md"
        ));
    }
}
