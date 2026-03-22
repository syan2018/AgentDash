use std::path::PathBuf;

use agent_client_protocol::McpServer;
use agentdash_domain::project::Project;
use agentdash_domain::story::Story;
use agentdash_domain::workspace::Workspace;
use agentdash_executor::{AgentDashExecutorConfig, ExecutionAddressSpace};

use crate::session_context::{
    SessionContextSnapshot, SessionEffectiveContext, SessionExecutorSummary, SessionOwnerContext,
    SessionProjectDefaults, SessionStoryOverrides, SharedContextMount,
    build_session_executor_summary, normalize_optional_string,
};
use crate::session_plan::{
    SessionRuntimePolicySummary, SessionToolVisibilitySummary, summarize_runtime_policy,
    summarize_tool_visibility,
};
use crate::workflow::ActiveWorkflowProjection;

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
    pub resolved_config: Option<AgentDashExecutorConfig>,
    pub address_space: Option<ExecutionAddressSpace>,
    pub mcp_servers: Vec<McpServer>,
    pub working_dir: Option<String>,
    pub workspace_root: Option<PathBuf>,
    pub tool_visibility: SessionToolVisibilitySummary,
    pub runtime_policy: SessionRuntimePolicySummary,
    pub workflow: Option<ActiveWorkflowProjection>,
}

/// Bootstrap plan 中的 owner 摘要信息。
#[derive(Debug, Clone)]
pub struct BootstrapOwnerSummary {
    pub variant: BootstrapOwnerVariant,
    pub project: Project,
    pub story: Option<Story>,
    pub workspace: Option<Workspace>,
    pub workspace_attached: bool,
}

/// Owner 级别差异（与 `SessionOwnerVariant` 映射）。
#[derive(Debug, Clone)]
pub enum BootstrapOwnerVariant {
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

/// 构建 `SessionBootstrapPlan` 的输入参数。
pub struct BootstrapPlanInput {
    pub project: Project,
    pub story: Option<Story>,
    pub workspace: Option<Workspace>,
    pub resolved_config: Option<AgentDashExecutorConfig>,
    pub address_space: Option<ExecutionAddressSpace>,
    pub mcp_servers: Vec<McpServer>,
    pub working_dir: Option<String>,
    pub workspace_root: Option<PathBuf>,
    pub executor_preset_name: Option<String>,
    pub executor_source: String,
    pub executor_resolution_error: Option<String>,
    pub owner_variant: BootstrapOwnerVariant,
    pub workflow: Option<ActiveWorkflowProjection>,
}

/// 从输入构建统一 bootstrap plan。
pub fn build_bootstrap_plan(input: BootstrapPlanInput) -> SessionBootstrapPlan {
    let workspace_attached = input.workspace.is_some();

    let mut working_dir = input.working_dir;
    let mut workspace_root = input.workspace_root;
    if working_dir.is_none() && input.workspace.is_some() {
        working_dir = Some(".".to_string());
    }
    if workspace_root.is_none() {
        workspace_root = input
            .workspace
            .as_ref()
            .map(|ws| PathBuf::from(ws.container_ref.clone()));
    }

    let tool_visibility =
        summarize_tool_visibility(input.address_space.as_ref(), &input.mcp_servers);
    let runtime_policy = summarize_runtime_policy(
        workspace_attached,
        input.address_space.as_ref(),
        &input.mcp_servers,
        &tool_visibility.tool_names,
    );
    let executor = build_session_executor_summary(
        input.resolved_config.as_ref(),
        input.executor_preset_name,
        input.executor_source,
        input.executor_resolution_error,
    );

    SessionBootstrapPlan {
        owner: BootstrapOwnerSummary {
            variant: input.owner_variant,
            project: input.project,
            story: input.story,
            workspace: input.workspace,
            workspace_attached,
        },
        executor,
        resolved_config: input.resolved_config,
        address_space: input.address_space,
        mcp_servers: input.mcp_servers,
        working_dir,
        workspace_root,
        tool_visibility,
        runtime_policy,
        workflow: input.workflow,
    }
}

/// 从 bootstrap plan 派生前端可用的 `SessionContextSnapshot`。
///
/// 这确保 query path 与 bootstrap path 产出一致的 snapshot，
/// 而不是各自独立推导 executor / tool visibility / runtime policy。
pub fn derive_session_context_snapshot(plan: &SessionBootstrapPlan) -> SessionContextSnapshot {
    let project = &plan.owner.project;
    let story = plan.owner.story.as_ref();

    let effective_mount_policy = story
        .and_then(|s| s.context.mount_policy_override.clone())
        .unwrap_or_else(|| project.config.mount_policy.clone());
    let effective_session_composition =
        crate::session_plan::resolve_effective_session_composition(project, story);

    let owner_context = match &plan.owner.variant {
        BootstrapOwnerVariant::Task { story_overrides } => SessionOwnerContext::Task {
            story_overrides: story_overrides.clone(),
        },
        BootstrapOwnerVariant::Story { story_overrides } => SessionOwnerContext::Story {
            story_overrides: story_overrides.clone(),
        },
        BootstrapOwnerVariant::Project {
            agent_key,
            agent_display_name,
            shared_context_mounts,
        } => SessionOwnerContext::Project {
            agent_key: agent_key.clone(),
            agent_display_name: agent_display_name.clone(),
            shared_context_mounts: shared_context_mounts.clone(),
        },
    };

    SessionContextSnapshot {
        executor: plan.executor.clone(),
        project_defaults: SessionProjectDefaults {
            default_agent_type: normalize_optional_string(
                project.config.default_agent_type.clone(),
            ),
            context_containers: project.config.context_containers.clone(),
            mount_policy: project.config.mount_policy.clone(),
            session_composition: project.config.session_composition.clone(),
        },
        effective: SessionEffectiveContext {
            mount_policy: effective_mount_policy,
            session_composition: effective_session_composition,
            tool_visibility: plan.tool_visibility.clone(),
            runtime_policy: plan.runtime_policy.clone(),
        },
        owner_context,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::project::Project;

    #[test]
    fn build_plan_applies_workspace_defaults() {
        use agentdash_domain::workspace::{Workspace, WorkspaceType};

        let project = Project::new(
            "test".to_string(),
            "desc".to_string(),
            "backend".to_string(),
        );
        let workspace = Workspace::new(
            project.id,
            "backend".to_string(),
            "test-ws".to_string(),
            "/workspace/test".to_string(),
            WorkspaceType::Static,
        );

        let plan = build_bootstrap_plan(BootstrapPlanInput {
            project: project.clone(),
            story: None,
            workspace: Some(workspace),
            resolved_config: None,
            address_space: None,
            mcp_servers: vec![],
            working_dir: None,
            workspace_root: None,
            executor_preset_name: None,
            executor_source: "test".to_string(),
            executor_resolution_error: None,
            owner_variant: BootstrapOwnerVariant::Project {
                agent_key: "default".to_string(),
                agent_display_name: "Default Agent".to_string(),
                shared_context_mounts: vec![],
            },
            workflow: None,
        });

        assert_eq!(plan.working_dir.as_deref(), Some("."));
        assert_eq!(
            plan.workspace_root,
            Some(PathBuf::from("/workspace/test"))
        );
        assert!(plan.owner.workspace_attached);
    }

    #[test]
    fn derive_snapshot_from_plan_produces_consistent_output() {
        let project = Project::new(
            "test".to_string(),
            "desc".to_string(),
            "backend".to_string(),
        );

        let plan = build_bootstrap_plan(BootstrapPlanInput {
            project: project.clone(),
            story: None,
            workspace: None,
            resolved_config: None,
            address_space: None,
            mcp_servers: vec![],
            working_dir: None,
            workspace_root: None,
            executor_preset_name: None,
            executor_source: "test".to_string(),
            executor_resolution_error: None,
            owner_variant: BootstrapOwnerVariant::Project {
                agent_key: "default".to_string(),
                agent_display_name: "Default Agent".to_string(),
                shared_context_mounts: vec![],
            },
            workflow: None,
        });

        let snapshot = derive_session_context_snapshot(&plan);
        assert_eq!(snapshot.executor.source, "test");
        assert!(!snapshot.effective.runtime_policy.workspace_attached);
        assert!(!snapshot.effective.runtime_policy.address_space_attached);
    }
}
