use agentdash_spi::SessionHookSnapshot;

use super::snapshot_helpers::snapshot_workspace_root;

pub(super) fn extract_tool_arg<'a>(
    payload: Option<&'a serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    payload
        .and_then(|value| value.get("args"))
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_str)
}

pub(super) fn extract_payload_str<'a>(
    payload: Option<&'a serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    payload
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_str)
}

pub(super) fn extract_payload_string_list(
    payload: Option<&serde_json::Value>,
    key: &str,
) -> Vec<String> {
    payload
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub(super) struct SubagentResult<'a> {
    pub subagent_type: &'a str,
    pub summary: &'a str,
    pub status: &'a str,
    pub dispatch_id: &'a str,
    pub companion_session_id: &'a str,
    pub findings: &'a [String],
    pub follow_ups: &'a [String],
    pub artifact_refs: &'a [String],
    pub is_blocking: bool,
}

pub(super) fn build_subagent_result_context(r: &SubagentResult<'_>) -> String {
    let mut sections = vec![if r.is_blocking {
        "## Companion Blocking Review".to_string()
    } else {
        "## Companion Follow-up".to_string()
    }];
    sections.push(format!("- 类型: {}", r.subagent_type));
    sections.push(format!("- status: {}", r.status));
    sections.push(format!("- dispatch_id: {}", r.dispatch_id));
    sections.push(format!("- companion_session_id: {}", r.companion_session_id));
    sections.push(format!("- 摘要: {}", r.summary));

    if !r.findings.is_empty() {
        sections.push("\n### 关键发现".to_string());
        sections.extend(r.findings.iter().map(|item| format!("- {item}")));
    }
    if !r.follow_ups.is_empty() {
        sections.push("\n### 建议后续动作".to_string());
        sections.extend(r.follow_ups.iter().map(|item| format!("- {item}")));
    }
    if !r.artifact_refs.is_empty() {
        sections.push("\n### 相关产物".to_string());
        sections.extend(r.artifact_refs.iter().map(|item| format!("- {item}")));
    }

    sections.push(if r.is_blocking {
        "\n请先明确这份 companion 结果如何被主 session 采纳、拒绝或拆解，不要直接忽略后继续结束本轮。"
            .to_string()
    } else {
        "\n请把这份 companion 结果吸收进主 session 的下一步行动中，再继续推进。".to_string()
    });
    sections.join("\n")
}

pub(super) fn shell_exec_rewritten_args(
    snapshot: &SessionHookSnapshot,
    payload: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    let workspace_root = snapshot_workspace_root(snapshot)?;
    let cwd = extract_tool_arg(payload, "cwd")?;
    if !std::path::Path::new(cwd).is_absolute() {
        return None;
    }

    let rewritten_cwd = absolutize_cwd_to_workspace_relative(workspace_root, cwd)?;
    let mut args = payload?.get("args")?.clone();
    args.as_object_mut()?
        .insert("cwd".to_string(), serde_json::Value::String(rewritten_cwd));
    Some(args)
}

fn absolutize_cwd_to_workspace_relative(workspace_root: &str, cwd: &str) -> Option<String> {
    let display_root = normalize_path_display_for_hook(workspace_root);
    let display_cwd = normalize_path_display_for_hook(cwd);
    let normalized_root = display_root.to_ascii_lowercase();
    let normalized_cwd = display_cwd.to_ascii_lowercase();
    if normalized_root.is_empty() || normalized_cwd.is_empty() {
        return None;
    }
    if normalized_cwd == normalized_root {
        return Some(".".to_string());
    }

    let prefix = format!("{normalized_root}/");
    normalized_cwd.strip_prefix(&prefix).and_then(|_| {
        display_cwd
            .get(prefix.len()..)
            .map(|suffix| suffix.trim_matches('/').to_string())
            .filter(|value| !value.is_empty())
    })
}

fn normalize_path_display_for_hook(path: &str) -> String {
    path.replace('\\', "/")
        .trim()
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn tool_call_failed(payload: Option<&serde_json::Value>) -> bool {
    payload
        .and_then(|value| value.get("is_error"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub(super) fn is_update_task_status_tool(tool_name: &str) -> bool {
    tool_name.ends_with("update_task_status")
}

pub(super) fn is_report_workflow_artifact_tool(tool_name: &str) -> bool {
    tool_name.ends_with("report_workflow_artifact")
}
