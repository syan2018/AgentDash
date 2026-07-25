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
    pub content: &'a [agentdash_agent::ContentPart],
    pub details: Option<&'a serde_json::Value>,
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
    let content_items = result
        .as_ref()
        .map(|result| tool_content_items(result.content));
    let success = result.as_ref().map(|result| !result.is_error);

    match projector {
        ToolProtocolProjector::Command => {
            let command = string_arg(&arguments, "command").unwrap_or_default();
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
                .and_then(|result| result.details)
                .and_then(|output| integer_field(output, &["exit_code", "exitCode"]))
                .or_else(|| {
                    result
                        .as_ref()
                        .and_then(|result| parse_result_json(&tool_content_text(result.content)))
                        .and_then(|output| integer_field(&output, &["exit_code", "exitCode"]))
                })
                .and_then(|value| i32::try_from(value).ok());
            Ok(AgentDashNativeThreadItem::ShellExec {
                id: item_id.to_owned(),
                command,
                cwd,
                execution_mode,
                arguments,
                status,
                aggregated_output: result.map(|result| tool_content_text(result.content)),
                exit_code,
                success,
            }
            .into())
        }
        ToolProtocolProjector::FileChange => {
            let changes = string_arg(&arguments, "patch")
                .as_deref()
                .and_then(|patch| parse_apply_patch_specs(patch).ok())
                .unwrap_or_default();
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
            path: string_arg(&arguments, "path")
                .or_else(|| string_arg(&arguments, "file_path"))
                .unwrap_or_default(),
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
            pattern: string_arg(&arguments, "pattern").unwrap_or_default(),
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
            pattern: string_arg(&arguments, "pattern").unwrap_or_default(),
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
                    serde_json::json!({"content": result.content, "details": result.details})
                }),
                "error": result.as_ref().filter(|result| result.is_error).map(|result| {
                    serde_json::json!({"message": tool_content_text(result.content), "details": result.details})
                }),
            }))
            .map(Into::into)
            .map_err(|error| ToolPresentationError::Invalid {
                tool: tool_name.to_owned(),
                family: "mcp",
                reason: error.to_string(),
            })
        }
        ToolProtocolProjector::Dynamic => Ok(thread_item::dynamic_tool_call(
            item_id,
            tool_name,
            arguments,
            status,
            content_items,
            success,
        )
        .into()),
    }
}

fn tool_content_items(
    content: &[agentdash_agent::ContentPart],
) -> Vec<codex::DynamicToolCallOutputContentItem> {
    content
        .iter()
        .filter_map(|part| match part {
            agentdash_agent::ContentPart::Text { text } => {
                Some(codex::DynamicToolCallOutputContentItem::InputText { text: text.clone() })
            }
            agentdash_agent::ContentPart::Image { mime_type, data } => {
                Some(codex::DynamicToolCallOutputContentItem::InputImage {
                    image_url: format!("data:{mime_type};base64,{data}"),
                })
            }
            agentdash_agent::ContentPart::Reasoning { .. } => None,
        })
        .collect()
}

fn tool_content_text(content: &[agentdash_agent::ContentPart]) -> String {
    content
        .iter()
        .filter_map(agentdash_agent::ContentPart::extract_text)
        .collect::<Vec<_>>()
        .join("\n")
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
                content: &[agentdash_agent::ContentPart::Text {
                    text: "hello".to_owned(),
                }],
                details: None,
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

    #[test]
    fn dynamic_tool_matches_main_without_owner_namespace() {
        let item = project_tool_item(
            "tool-2",
            "workspace_module_list",
            serde_json::json!({"kind": "canvas"}),
            &ToolProtocolProjector::Dynamic,
            false,
            false,
            Some(ToolPresentationResult {
                content: &[agentdash_agent::ContentPart::Text {
                    text: "listed".to_owned(),
                }],
                details: None,
                is_error: false,
            }),
        )
        .expect("project dynamic tool");

        assert!(matches!(
            item,
            AgentDashThreadItem::Codex(codex::ThreadItem::DynamicToolCall {
                namespace: None,
                tool,
                ..
            }) if tool == "workspace_module_list"
        ));
    }
}
