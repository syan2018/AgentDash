use agentdash_domain::{project::Project, task::Task};
use agentdash_executor::AgentDashExecutorConfig;

use super::execution::TaskExecutionError;

/// 确定 Task 最终使用的 executor 配置
///
/// 优先级：显式传入 > Task.agent_binding > Preset → Project default
pub fn resolve_task_executor_config(
    explicit: Option<AgentDashExecutorConfig>,
    task: &Task,
    project: &Project,
) -> Result<Option<AgentDashExecutorConfig>, TaskExecutionError> {
    if explicit.is_some() {
        return Ok(explicit);
    }
    resolve_task_agent_config(task, project)
}

pub fn resolve_task_agent_config(
    task: &Task,
    project: &Project,
) -> Result<Option<AgentDashExecutorConfig>, TaskExecutionError> {
    if let Some(agent_type) = normalize_option_string(task.agent_binding.agent_type.clone()) {
        return Ok(Some(AgentDashExecutorConfig::new(agent_type)));
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
        let agent_type = normalize_option_string(Some(preset.agent_type.clone()));
        let Some(agent_type) = agent_type else {
            return Ok(None);
        };
        let mut config = AgentDashExecutorConfig::new(agent_type);
        if let Some(obj) = preset.config.as_object() {
            if let Some(v) = obj.get("variant").and_then(|v| v.as_str()) {
                config.variant = normalize_option_string(Some(v.to_string()));
            }
            if let Some(v) = obj.get("model_id").and_then(|v| v.as_str()) {
                config.model_id = normalize_option_string(Some(v.to_string()));
            }
            if let Some(v) = obj.get("agent_id").and_then(|v| v.as_str()) {
                config.agent_id = normalize_option_string(Some(v.to_string()));
            }
            if let Some(v) = obj.get("reasoning_id").and_then(|v| v.as_str()) {
                config.reasoning_id = normalize_option_string(Some(v.to_string()));
            }
            if let Some(v) = obj.get("permission_policy").and_then(|v| v.as_str()) {
                config.permission_policy = normalize_option_string(Some(v.to_string()));
            }
        }
        return Ok(Some(config));
    }

    Ok(normalize_option_string(project.config.default_agent_type.clone())
        .map(AgentDashExecutorConfig::new))
}

fn normalize_option_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}
