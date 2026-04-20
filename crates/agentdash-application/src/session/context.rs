use agentdash_domain::context_container::ContextContainerDefinition;
use agentdash_domain::session_composition::SessionComposition;
use agentdash_domain::story::Story;
use serde::Serialize;

use agentdash_spi::session_capabilities::SessionBaselineCapabilities;

use super::plan::{SessionRuntimePolicySummary, SessionToolVisibilitySummary};
use crate::runtime::{AgentConfig, ThinkingLevel};

// ─── Unified DTO ─────────────────────────────────────

/// 统一的 session context snapshot，通过 `owner_context` 区分 owner 级别差异。
#[derive(Debug, Clone, Serialize)]
pub struct SessionContextSnapshot {
    pub executor: SessionExecutorSummary,
    pub project_defaults: SessionProjectDefaults,
    pub effective: SessionEffectiveContext,
    pub owner_context: SessionOwnerContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_capabilities: Option<SessionBaselineCapabilities>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "owner_level", rename_all = "snake_case")]
pub enum SessionOwnerContext {
    Task {
        story_overrides: SessionStoryOverrides,
    },
    Story {
        story_overrides: SessionStoryOverrides,
    },
    Project {
        agent_key: String,
        agent_display_name: String,
    },
}

// ─── Shared sub-structs ──────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SessionExecutorSummary {
    pub executor: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub agent_id: Option<String>,
    pub thinking_level: Option<ThinkingLevel>,
    pub permission_policy: Option<String>,
    pub preset_name: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionProjectDefaults {
    pub default_agent_type: Option<String>,
    pub context_containers: Vec<ContextContainerDefinition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStoryOverrides {
    pub context_containers: Vec<ContextContainerDefinition>,
    pub disabled_container_ids: Vec<String>,
    pub session_composition: Option<SessionComposition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionEffectiveContext {
    pub session_composition: SessionComposition,
    pub tool_visibility: SessionToolVisibilitySummary,
    pub runtime_policy: SessionRuntimePolicySummary,
}

// ─── Shared utility functions ────────────────────────

pub fn build_session_executor_summary(
    resolved_config: Option<&AgentConfig>,
    preset_name: Option<String>,
    resolution: ExecutorResolution,
) -> SessionExecutorSummary {
    let (source, resolution_error) = resolution.into_parts();
    SessionExecutorSummary {
        executor: resolved_config.map(|c| c.executor.clone()),
        provider_id: resolved_config.and_then(|c| c.provider_id.clone()),
        model_id: resolved_config.and_then(|c| c.model_id.clone()),
        agent_id: resolved_config.and_then(|c| c.agent_id.clone()),
        thinking_level: resolved_config.and_then(|c| c.thinking_level),
        permission_policy: resolved_config.and_then(|c| c.permission_policy.clone()),
        preset_name,
        source,
        resolution_error,
    }
}

/// Executor config 解析结果：来源 + 可选错误。
///
/// 替代了原来分散的 `(executor_source: String, executor_resolution_error: Option<String>)`
/// 双字段——两者始终成对出现，且"有 error"与"没 error"是两条互斥路径，
/// 用 enum 表达可以让类型层拒绝"err 为 Some 但 source 为空"这类非法组合。
#[derive(Debug, Clone)]
pub enum ExecutorResolution {
    /// 成功解析到 AgentConfig。`source` 描述使用的来源
    /// （如 `task.agent_binding.preset_name` / `project.config.default_agent_type`）。
    Resolved { source: String },
    /// 解析失败但被上游容忍（`strict_config_resolution=false` 场景）。
    /// `source` 描述尝试过的来源，`error` 记录失败原因。
    Failed { source: String, error: String },
}

impl ExecutorResolution {
    pub fn resolved(source: impl Into<String>) -> Self {
        Self::Resolved { source: source.into() }
    }

    pub fn failed(source: impl Into<String>, error: impl Into<String>) -> Self {
        Self::Failed {
            source: source.into(),
            error: error.into(),
        }
    }

    pub fn source(&self) -> &str {
        match self {
            Self::Resolved { source } | Self::Failed { source, .. } => source.as_str(),
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Resolved { .. } => None,
            Self::Failed { error, .. } => Some(error.as_str()),
        }
    }

    /// 用于持久化/序列化到 `SessionExecutorSummary` 的旧双字段形状。
    pub fn into_parts(self) -> (String, Option<String>) {
        match self {
            Self::Resolved { source } => (source, None),
            Self::Failed { source, error } => (source, Some(error)),
        }
    }
}

pub fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(normalize_string)
}

/// 适用于 `.and_then(normalize_string)` 模式。
pub fn normalize_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// 从 Story 的 context 中提取 `SessionStoryOverrides`。
pub fn extract_story_overrides(story: &Story) -> SessionStoryOverrides {
    SessionStoryOverrides {
        context_containers: story.context.context_containers.clone(),
        disabled_container_ids: story.context.disabled_container_ids.clone(),
        session_composition: story.context.session_composition.clone(),
    }
}

// ─── Bootstrap helpers ───────────────────────────────

use agentdash_domain::workspace::Workspace;

use agentdash_spi::Vfs;

use crate::vfs::build_workspace_vfs;

/// 将 workspace 相关的默认值注入到 `PromptSessionRequest` 的可变字段中。
/// 仅在字段为 None 时填充，不覆盖已有值。
pub fn apply_workspace_defaults(
    working_dir: &mut Option<String>,
    vfs: &mut Option<Vfs>,
    workspace: Option<&Workspace>,
) {
    if working_dir.is_none() && workspace.is_some() {
        *working_dir = Some(".".to_string());
    }
    if vfs.is_none() {
        if let Some(space) = workspace.and_then(|item| build_workspace_vfs(item).ok()) {
            *vfs = Some(space);
        }
    }
}
