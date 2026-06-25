use agentdash_domain::project::Project;
use agentdash_domain::story::Story;

use super::context::{
    SessionContextSnapshot, SessionEffectiveContext, SessionExecutorSummary, SessionOwnerContext,
    SessionProjectDefaults, SessionStoryOverrides, build_session_executor_summary,
    normalize_optional_string,
};
use super::plan::{
    SessionRuntimePolicySummary, SessionToolVisibilitySummary, summarize_runtime_policy,
};
use agentdash_spi::{AgentConfig, Vfs};

use crate::runtime::McpServerSummary;

/// 统一的 session bootstrap 计划。
///
/// 目标是让以下消费者都基于同一份 plan 派生：
/// - Agent 实际收到的 prompt（bootstrap path）
/// - 前端查询到的 session context snapshot（query path）
/// - hook runtime 看到的 session runtime 元信息
#[derive(Debug, Clone)]
pub struct SessionBootstrapPlan {
    pub owner: BootstrapOwnerSummary,
    pub executor: SessionExecutorSummary,
    pub vfs: Option<Vfs>,
    pub tool_visibility: SessionToolVisibilitySummary,
    pub runtime_policy: SessionRuntimePolicySummary,
}

/// Bootstrap plan 中的 owner 摘要信息。
#[derive(Debug, Clone)]
pub struct BootstrapOwnerSummary {
    pub project: Project,
    pub story: Option<Story>,
    pub story_overrides: SessionStoryOverrides,
}

/// 构建 `SessionBootstrapPlan` 的输入参数。
pub struct BootstrapPlanInput {
    pub project: Project,
    pub story: Option<Story>,
    pub workspace_attached: bool,
    pub resolved_config: Option<AgentConfig>,
    pub vfs: Option<Vfs>,
    pub mcp_servers: Vec<McpServerSummary>,
    pub executor_preset_name: Option<String>,
    pub executor_resolution: super::context::ExecutorResolution,
    pub story_overrides: SessionStoryOverrides,
}

/// 从输入构建统一 bootstrap plan。
pub fn build_bootstrap_plan(input: BootstrapPlanInput) -> SessionBootstrapPlan {
    let tool_visibility = super::plan::summarize_tool_visibility_with_context(
        input.vfs.as_ref(),
        &input.mcp_servers,
        Some(agentdash_spi::CapabilityScope::Task),
    );
    let runtime_policy = summarize_runtime_policy(
        input.workspace_attached,
        input.vfs.as_ref(),
        &input.mcp_servers,
        &tool_visibility.tool_names,
    );
    let executor = build_session_executor_summary(
        input.resolved_config.as_ref(),
        input.executor_preset_name,
        input.executor_resolution,
    );

    SessionBootstrapPlan {
        owner: BootstrapOwnerSummary {
            project: input.project,
            story: input.story,
            story_overrides: input.story_overrides,
        },
        executor,
        vfs: input.vfs,
        tool_visibility,
        runtime_policy,
    }
}

/// 从 bootstrap plan 派生前端可用的 `SessionContextSnapshot`。
///
/// 这确保 query path 与 bootstrap path 产出一致的 snapshot，
/// 而不是各自独立推导 executor / tool visibility / runtime policy。
pub fn derive_session_context_snapshot(plan: &SessionBootstrapPlan) -> SessionContextSnapshot {
    let project = &plan.owner.project;
    let story = plan.owner.story.as_ref();

    let effective_session_composition =
        super::plan::resolve_story_session_composition(story).unwrap_or_default();

    let owner_context = SessionOwnerContext::Task {
        story_overrides: plan.owner.story_overrides.clone(),
    };

    SessionContextSnapshot {
        executor: plan.executor.clone(),
        project_defaults: SessionProjectDefaults {
            default_agent_type: normalize_optional_string(
                project.config.default_agent_type.clone(),
            ),
            context_containers: project.config.context_containers.clone(),
        },
        effective: SessionEffectiveContext {
            session_composition: effective_session_composition,
            tool_visibility: plan.tool_visibility.clone(),
            runtime_policy: plan.runtime_policy.clone(),
        },
        owner_context,
        session_capabilities: None,
    }
}
