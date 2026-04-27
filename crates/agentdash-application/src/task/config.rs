use agentdash_domain::{
    project::{AgentPreset, Project},
    task::Task,
};

use crate::runtime::{AgentConfig, SystemPromptMode, ThinkingLevel};

use super::execution::TaskExecutionError;

/// 诊断用标签：描述本次 task executor config 从哪个来源解析而来。
///
/// 结果进入 `ExecutorResolution.source`，供运行时 metadata 展示。
/// 原先重复定义在 `task/session_runtime_inputs.rs` 与 `session/assembler.rs`,
/// M5 后收敛到此处作为唯一副本。
pub fn resolve_task_executor_source(
    task: &Task,
    project: &Project,
    explicit_config: Option<&AgentConfig>,
) -> String {
    if explicit_config.is_some() {
        return "explicit.executor_config".to_string();
    }
    if task
        .agent_binding
        .agent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "task.agent_binding.agent_type".to_string();
    }
    if task
        .agent_binding
        .preset_name
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "task.agent_binding.preset_name".to_string();
    }
    if project
        .config
        .default_agent_type
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "project.config.default_agent_type".to_string();
    }
    "unresolved".to_string()
}

/// 确定 Task 最终使用的 executor 配置
///
/// 优先级：显式传入 > Task.agent_binding > Preset → Project default
pub fn resolve_task_executor_config(
    explicit: Option<AgentConfig>,
    task: &Task,
    project: &Project,
) -> Result<Option<AgentConfig>, TaskExecutionError> {
    if explicit.is_some() {
        return Ok(explicit);
    }
    resolve_task_agent_config(task, project)
}

pub fn resolve_task_agent_config(
    task: &Task,
    project: &Project,
) -> Result<Option<AgentConfig>, TaskExecutionError> {
    if let Some(agent_type) = normalize_option_string(task.agent_binding.agent_type.clone()) {
        return Ok(Some(AgentConfig::new(agent_type)));
    }

    if let Some(preset_name) = normalize_option_string(task.agent_binding.preset_name.clone()) {
        let preset = project
            .config
            .agent_presets
            .iter()
            .find(|item| item.name == preset_name)
            .ok_or_else(|| {
                TaskExecutionError::BadRequest(format!("Project 中不存在预设: {preset_name}"))
            })?;
        return executor_config_from_preset(preset).map(Some);
    }

    Ok(normalize_option_string(project.config.default_agent_type.clone()).map(AgentConfig::new))
}

pub fn executor_config_from_preset(
    preset: &AgentPreset,
) -> Result<AgentConfig, TaskExecutionError> {
    let agent_type = normalize_option_string(Some(preset.agent_type.clone()));
    let agent_type = agent_type.ok_or_else(|| {
        TaskExecutionError::BadRequest(format!("Preset `{}` 缺少有效 agent_type", preset.name))
    })?;

    let mut config = AgentConfig::new(agent_type);
    if let Some(obj) = preset.config.as_object() {
        if let Some(v) = obj.get("provider_id").and_then(|v| v.as_str()) {
            config.provider_id = normalize_option_string(Some(v.to_string()));
        }
        if let Some(v) = obj.get("model_id").and_then(|v| v.as_str()) {
            config.model_id = normalize_option_string(Some(v.to_string()));
        }
        if let Some(v) = obj.get("agent_id").and_then(|v| v.as_str()) {
            config.agent_id = normalize_option_string(Some(v.to_string()));
        }
        if let Some(v) = obj.get("thinking_level").and_then(|v| v.as_str()) {
            let level =
                serde_json::from_value::<ThinkingLevel>(serde_json::Value::String(v.to_string()))
                    .map_err(|error| {
                    TaskExecutionError::BadRequest(format!(
                        "Preset `{}` 的 thinking_level 非法: {error}",
                        preset.name
                    ))
                })?;
            config.thinking_level = Some(level);
        }
        if let Some(v) = obj.get("permission_policy").and_then(|v| v.as_str()) {
            config.permission_policy = normalize_option_string(Some(v.to_string()));
        }
        if let Some(arr) = obj.get("tool_clusters").and_then(|v| v.as_array()) {
            let clusters: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if !clusters.is_empty() {
                config.tool_clusters = Some(clusters);
            }
        }
        if let Some(v) = obj.get("system_prompt").and_then(|v| v.as_str()) {
            config.system_prompt = normalize_option_string(Some(v.to_string()));
        }
        if let Some(v) = obj.get("system_prompt_mode").and_then(|v| v.as_str()) {
            if let Ok(mode) =
                serde_json::from_value::<SystemPromptMode>(serde_json::Value::String(v.to_string()))
            {
                config.system_prompt_mode = Some(mode);
            }
        }
    }

    Ok(config)
}

pub fn normalize_option_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}
