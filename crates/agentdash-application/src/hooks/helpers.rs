pub(super) fn extract_tool_arg<'a>(
    payload: Option<&'a serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    payload
        .and_then(|value| value.get("args"))
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_str)
}

pub(super) fn shell_exec_rewritten_args(
    payload: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    let default_mount_root_ref = payload
        .and_then(|value| value.get("default_mount_root_ref"))
        .and_then(serde_json::Value::as_str)?;
    let cwd = extract_tool_arg(payload, "cwd")?;
    if !cwd.starts_with('/') && !std::path::Path::new(cwd).is_absolute() {
        return None;
    }

    let rewritten_cwd = absolutize_cwd_to_mount_relative(default_mount_root_ref, cwd)?;
    let mut args = payload?.get("args")?.clone();
    args.as_object_mut()?
        .insert("cwd".to_string(), serde_json::Value::String(rewritten_cwd));
    Some(args)
}

fn absolutize_cwd_to_mount_relative(default_mount_root_ref: &str, cwd: &str) -> Option<String> {
    let display_root = normalize_path_display_for_hook(default_mount_root_ref);
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

pub(super) fn is_report_workflow_artifact_tool(tool_name: &str) -> bool {
    tool_name.ends_with("report_workflow_artifact")
}
