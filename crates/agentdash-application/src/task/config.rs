//! Task executor configuration resolution — static config → dispatch-time policy bridge.
//!
//! This module resolves the `AgentConfig` that will be used when dispatching a Task.
//! It bridges the gap between **Task static config** (the `AgentBinding` preference the
//! user sets at authoring time) and **dispatch-time policy** (the `AgentConfig` that
//! `SubjectExecutionIntent` consumes to create `LifecycleAgent` / `AgentFrame`).
//!
//! **Resolution priority** (first match wins):
//! 1. Explicit `AgentConfig` passed by the caller (e.g. API override)
//! 2. `Task.agent_binding.agent_type` — direct agent type preference
//! 3. `Task.agent_binding.preset_name` → resolved against `ProjectConfig.agent_presets`
//! 4. `Project.config.default_agent_type` — project-level fallback
//!
//! Once resolved, the result feeds into dispatch and is **consumed** — the Task entity
//! does not participate in runtime decisions after this point. Runtime truth lives in
//! `LifecycleAgent → AgentFrame`.

use agentdash_domain::{
    common::AgentPresetConfig,
    project::{AgentPreset, Project},
    task::Task,
};

use crate::runtime::AgentConfig;

use super::execution::TaskExecutionError;

/// 诊断用标签：描述本次 task executor config 从哪个来源解析而来。
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

    let preset_config = AgentPresetConfig::from_json(&preset.config)
        .map_err(|error| TaskExecutionError::BadRequest(error.to_string()))?;
    Ok(preset_config.to_agent_config(&agent_type))
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
