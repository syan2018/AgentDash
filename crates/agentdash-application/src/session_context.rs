use agentdash_domain::context_container::{ContextContainerDefinition, MountDerivationPolicy};
use agentdash_domain::session_composition::SessionComposition;
use agentdash_domain::story::Story;
use serde::Serialize;

use crate::runtime::{AgentConfig, ThinkingLevel};
use crate::session_plan::{SessionRuntimePolicySummary, SessionToolVisibilitySummary};

// ─── Unified DTO ─────────────────────────────────────

/// 统一的 session context snapshot，通过 `owner_context` 区分 owner 级别差异。
#[derive(Debug, Clone, Serialize)]
pub struct SessionContextSnapshot {
    pub executor: SessionExecutorSummary,
    pub project_defaults: SessionProjectDefaults,
    pub effective: SessionEffectiveContext,
    pub owner_context: SessionOwnerContext,
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
        shared_context_mounts: Vec<SharedContextMount>,
    },
}

// ─── Shared sub-structs ──────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SessionExecutorSummary {
    pub executor: Option<String>,
    pub variant: Option<String>,
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
    pub mount_policy: MountDerivationPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStoryOverrides {
    pub context_containers: Vec<ContextContainerDefinition>,
    pub disabled_container_ids: Vec<String>,
    pub mount_policy_override: Option<MountDerivationPolicy>,
    pub session_composition: Option<SessionComposition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionEffectiveContext {
    pub mount_policy: MountDerivationPolicy,
    pub session_composition: SessionComposition,
    pub tool_visibility: SessionToolVisibilitySummary,
    pub runtime_policy: SessionRuntimePolicySummary,
}

/// Project agent 级别 session 可见的 context mount 条目。
#[derive(Debug, Clone, Serialize)]
pub struct SharedContextMount {
    pub container_id: String,
    pub mount_id: String,
    pub display_name: String,
    pub writable: bool,
}

// ─── Shared utility functions ────────────────────────

pub fn build_session_executor_summary(
    resolved_config: Option<&AgentConfig>,
    preset_name: Option<String>,
    source: impl Into<String>,
    resolution_error: Option<String>,
) -> SessionExecutorSummary {
    SessionExecutorSummary {
        executor: resolved_config.map(|c| c.executor.clone()),
        variant: resolved_config.and_then(|c| c.variant.clone()),
        provider_id: resolved_config.and_then(|c| c.provider_id.clone()),
        model_id: resolved_config.and_then(|c| c.model_id.clone()),
        agent_id: resolved_config.and_then(|c| c.agent_id.clone()),
        thinking_level: resolved_config.and_then(|c| c.thinking_level),
        permission_policy: resolved_config.and_then(|c| c.permission_policy.clone()),
        preset_name,
        source: source.into(),
        resolution_error,
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
        mount_policy_override: story.context.mount_policy_override.clone(),
        session_composition: story.context.session_composition.clone(),
    }
}

// ─── Bootstrap helpers ───────────────────────────────

use agentdash_domain::workspace::Workspace;
use std::path::PathBuf;

use crate::address_space::selected_workspace_binding;

/// 将 workspace 相关的默认值注入到 `PromptSessionRequest` 的可变字段中。
/// 仅在字段为 None 时填充，不覆盖已有值。
pub fn apply_workspace_defaults(
    working_dir: &mut Option<String>,
    workspace_root: &mut Option<PathBuf>,
    workspace: Option<&Workspace>,
) {
    if working_dir.is_none() && workspace.is_some() {
        *working_dir = Some(".".to_string());
    }
    if workspace_root.is_none() {
        *workspace_root = workspace
            .and_then(selected_workspace_binding)
            .map(|binding| PathBuf::from(binding.root_ref.clone()));
    }
}
