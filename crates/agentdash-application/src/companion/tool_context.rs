use agentdash_platform_spi::hooks::{
    HookRuntimeAccess, RuntimeAdapterProvenance, SharedHookRuntime,
};
use agentdash_platform_spi::{AgentToolError, AuthIdentity, ExecutionContext};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(crate) struct CompanionLifecycleAnchor {
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
}

#[derive(Clone)]
pub(crate) struct CompanionToolContext {
    canonical_runtime_thread_id: Option<String>,
    delivery_runtime_thread_id: Option<String>,
    turn_id: String,
    identity: Option<AuthIdentity>,
    hook_runtime: Option<SharedHookRuntime>,
    owner: Option<agentdash_platform_spi::PlatformToolExecutionContext>,
}

impl CompanionToolContext {
    pub(crate) fn from_execution_context(context: &ExecutionContext) -> Self {
        let owner = context.turn.platform_tool_execution.clone();
        let canonical_runtime_thread_id = owner
            .as_ref()
            .map(|owner| owner.runtime_thread_id.to_string());
        let delivery_runtime_thread_id = owner
            .as_ref()
            .map(|owner| owner.runtime_thread_id.to_string());

        Self {
            canonical_runtime_thread_id,
            delivery_runtime_thread_id,
            turn_id: context.session.turn_id.clone(),
            identity: context.session.identity.clone(),
            hook_runtime: context.turn.hook_runtime.clone(),
            owner,
        }
    }

    pub(crate) fn from_product_runtime(
        runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId,
        turn_id: String,
        owner: agentdash_platform_spi::PlatformToolExecutionContext,
        hook_runtime: SharedHookRuntime,
    ) -> Self {
        let runtime_thread_id = runtime_thread_id.to_string();
        Self {
            canonical_runtime_thread_id: Some(runtime_thread_id.clone()),
            delivery_runtime_thread_id: Some(runtime_thread_id),
            turn_id,
            identity: None,
            hook_runtime: Some(hook_runtime),
            owner: Some(owner),
        }
    }

    pub(crate) fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub(crate) fn canonical_runtime_thread_id(&self) -> Option<&str> {
        self.canonical_runtime_thread_id.as_deref()
    }

    pub(crate) fn delivery_runtime_thread_id(&self) -> Option<&str> {
        self.delivery_runtime_thread_id.as_deref()
    }

    pub(crate) fn wait_owner_scope(&self) -> Option<crate::wait_activity::WaitActivityOwnerScope> {
        self.owner
            .as_ref()
            .map(|owner| crate::wait_activity::WaitActivityOwnerScope {
                run_id: owner.run_id,
                agent_id: owner.agent_id,
                frame_id: owner.current_surface_frame_id,
            })
    }

    pub(crate) fn hook_runtime(&self) -> Option<&SharedHookRuntime> {
        self.hook_runtime.as_ref()
    }

    pub(crate) fn identity(&self) -> Option<&AuthIdentity> {
        self.identity.as_ref()
    }

    pub(crate) fn require_hook_runtime(
        &self,
        action: &str,
    ) -> Result<&SharedHookRuntime, AgentToolError> {
        self.hook_runtime.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(format!("当前缺少 hook runtime，无法{action}"))
        })
    }

    pub(crate) fn require_delivery_runtime_thread_id(
        &self,
        action: &str,
    ) -> Result<&str, AgentToolError> {
        self.delivery_runtime_thread_id.as_deref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(format!(
                "当前缺少 delivery runtime session id，无法{action}"
            ))
        })
    }

    pub(crate) fn require_lifecycle_anchor(
        &self,
        action: &str,
    ) -> Result<CompanionLifecycleAnchor, AgentToolError> {
        let owner = self.owner.as_ref().ok_or_else(|| {
            AgentToolError::ExecutionFailed(format!(
                "当前缺少 Platform Tool typed owner context，无法{action}"
            ))
        })?;
        Ok(CompanionLifecycleAnchor {
            project_id: owner.project_id,
            run_id: owner.run_id,
            agent_id: owner.agent_id,
            frame_id: owner.current_surface_frame_id,
        })
    }
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
    runtime_thread_id: String,
    turn_id: Option<String>,
}

impl CompanionHookProvenance {
    pub(crate) fn from_hook_runtime(
        hook_runtime: &dyn HookRuntimeAccess,
        turn_id: Option<String>,
    ) -> Self {
        Self {
            runtime_thread_id: hook_runtime.session_id().to_string(),
            turn_id,
        }
    }

    pub(crate) fn runtime_thread(
        &self,
        source: CompanionHookProvenanceSource,
    ) -> RuntimeAdapterProvenance {
        RuntimeAdapterProvenance::runtime_thread(
            self.runtime_thread_id.clone(),
            self.turn_id.clone(),
            source.as_str(),
        )
    }
}
