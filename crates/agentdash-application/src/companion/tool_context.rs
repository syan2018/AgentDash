use agentdash_spi::hooks::{HookRuntimeAccess, RuntimeAdapterProvenance, SharedHookRuntime};
use agentdash_spi::{AgentToolError, ExecutionContext};
use uuid::Uuid;

use crate::lifecycle::resolve_current_frame_from_delivery_trace_ref;
use crate::repository_set::RepositorySet;

#[derive(Clone, Copy)]
pub(crate) struct CompanionLifecycleAnchor {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
}

#[derive(Clone)]
pub(crate) struct CompanionToolContext {
    delivery_runtime_session_id: Option<String>,
    turn_id: String,
    hook_runtime: Option<SharedHookRuntime>,
}

impl CompanionToolContext {
    pub(crate) fn from_execution_context(context: &ExecutionContext) -> Self {
        let delivery_runtime_session_id = context
            .turn
            .hook_runtime
            .as_ref()
            .map(|session| session.session_id().to_string());

        Self {
            delivery_runtime_session_id,
            turn_id: context.session.turn_id.clone(),
            hook_runtime: context.turn.hook_runtime.clone(),
        }
    }

    pub(crate) fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub(crate) fn hook_runtime(&self) -> Option<&SharedHookRuntime> {
        self.hook_runtime.as_ref()
    }

    pub(crate) fn require_hook_runtime(
        &self,
        action: &str,
    ) -> Result<&SharedHookRuntime, AgentToolError> {
        self.hook_runtime.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(format!("当前缺少 hook runtime，无法{action}"))
        })
    }

    pub(crate) fn require_delivery_runtime_session_id(
        &self,
        action: &str,
    ) -> Result<&str, AgentToolError> {
        self.delivery_runtime_session_id.as_deref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(format!(
                "当前缺少 delivery runtime session id，无法{action}"
            ))
        })
    }

    pub(crate) async fn require_lifecycle_anchor(
        &self,
        action: &str,
        repos: &RepositorySet,
    ) -> Result<CompanionLifecycleAnchor, AgentToolError> {
        let session_id = self
            .require_delivery_runtime_session_id(action)?
            .to_string();
        resolve_lifecycle_anchor(&session_id, repos)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(format!("{error}，无法{action}")))
    }
}

async fn resolve_lifecycle_anchor(
    runtime_session_id: &str,
    repos: &RepositorySet,
) -> Result<CompanionLifecycleAnchor, String> {
    let (_anchor, agent, frame) = resolve_current_frame_from_delivery_trace_ref(
        runtime_session_id,
        repos.execution_anchor_repo.as_ref(),
        repos.lifecycle_agent_repo.as_ref(),
        repos.agent_frame_repo.as_ref(),
    )
    .await
    .map_err(|error| {
        format!(
            "通过 RuntimeSessionExecutionAnchor 查询 runtime session `{runtime_session_id}` 当前 AgentFrame 失败: {error}"
        )
    })?
    .ok_or_else(|| {
        format!("runtime session `{runtime_session_id}` 缺少可用 RuntimeSessionExecutionAnchor/AgentFrame")
    })?;

    Ok(CompanionLifecycleAnchor {
        project_id: agent.project_id,
        run_id: agent.run_id,
        agent_id: agent.id,
        frame_id: frame.id,
    })
}

#[derive(Clone, Copy)]
pub(crate) enum CompanionHookProvenanceSource {
    SubagentHookEvaluate,
    SubagentHookRefresh,
}

impl CompanionHookProvenanceSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::SubagentHookEvaluate => "companion_subagent_hook_evaluate",
            Self::SubagentHookRefresh => "companion_subagent_hook_refresh",
        }
    }
}

pub(crate) struct CompanionHookProvenance {
    runtime_session_id: String,
    turn_id: Option<String>,
}

impl CompanionHookProvenance {
    pub(crate) fn from_hook_runtime(
        hook_runtime: &dyn HookRuntimeAccess,
        turn_id: Option<String>,
    ) -> Self {
        Self {
            runtime_session_id: hook_runtime.session_id().to_string(),
            turn_id,
        }
    }

    pub(crate) fn runtime_session(
        &self,
        source: CompanionHookProvenanceSource,
    ) -> RuntimeAdapterProvenance {
        RuntimeAdapterProvenance::runtime_session(
            self.runtime_session_id.clone(),
            self.turn_id.clone(),
            source.as_str(),
        )
    }
}
