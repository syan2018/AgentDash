use agentdash_agent_runtime_contract::{
    HookDefinitionId, HookExecutionSite, HookPlanDigest, HookPlanRevision, HookRequirement,
};
use agentdash_platform_spi::{HookControlTarget, RuntimeAdapterProvenance};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Immutable Hook requirements owned by one AgentFrame revision.
///
/// Frame construction selects the canonical execution site together with each
/// business requirement. Runtime admission consumes that immutable route and
/// only intersects Driver-owned sites with the selected offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFrameHookPlan {
    pub revision: HookPlanRevision,
    pub digest: HookPlanDigest,
    pub requirements: Vec<AgentFrameHookRequirement>,
}

impl AgentFrameHookPlan {
    pub fn compile(
        revision: HookPlanRevision,
        requirements: Vec<AgentFrameHookRequirement>,
    ) -> Result<Self, AgentFrameHookPlanCompileError> {
        let encoded = serde_json::to_vec(&(revision, &requirements)).map_err(|error| {
            AgentFrameHookPlanCompileError::Digest {
                message: error.to_string(),
            }
        })?;
        let digest = HookPlanDigest::new(format!("sha256:{:x}", Sha256::digest(encoded))).map_err(
            |error| AgentFrameHookPlanCompileError::Digest {
                message: error.to_string(),
            },
        )?;
        Ok(Self {
            revision,
            digest,
            requirements,
        })
    }

    pub fn validate(&self) -> Result<(), AgentFrameHookPlanCompileError> {
        if self.revision.0 == 0 {
            return Err(AgentFrameHookPlanCompileError::Digest {
                message: "HookPlan revision must be positive".to_string(),
            });
        }
        let compiled = Self::compile(self.revision, self.requirements.clone())?;
        if compiled.digest != self.digest {
            return Err(AgentFrameHookPlanCompileError::Digest {
                message: "HookPlan requirements do not match the persisted digest".to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFrameHookRequirement {
    pub definition_id: HookDefinitionId,
    pub requirement: HookRequirement,
    pub site: HookExecutionSite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameHookPlanCompileQuery {
    pub target: HookControlTarget,
    pub provenance: RuntimeAdapterProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentFrameHookPlanCompileError {
    #[error("Hook policy source is unavailable: {message}")]
    SourceUnavailable { message: String },
    #[error("Hook policy source cannot produce an immutable requirement: {message}")]
    UnsupportedPolicy { message: String },
    #[error("HookPlan digest construction failed: {message}")]
    Digest { message: String },
}

#[async_trait]
pub trait AgentFrameHookPlanCompiler: Send + Sync {
    async fn compile_agent_frame_hook_plan(
        &self,
        query: AgentFrameHookPlanCompileQuery,
    ) -> Result<AgentFrameHookPlan, AgentFrameHookPlanCompileError>;
}

#[derive(Clone, Default)]
pub struct SharedAgentFrameHookPlanCompiler {
    inner: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<dyn AgentFrameHookPlanCompiler>>>,
}

impl SharedAgentFrameHookPlanCompiler {
    pub fn set(
        &self,
        compiler: std::sync::Arc<dyn AgentFrameHookPlanCompiler>,
    ) -> Result<(), std::sync::Arc<dyn AgentFrameHookPlanCompiler>> {
        self.inner.set(compiler)
    }
}

#[async_trait]
impl AgentFrameHookPlanCompiler for SharedAgentFrameHookPlanCompiler {
    async fn compile_agent_frame_hook_plan(
        &self,
        query: AgentFrameHookPlanCompileQuery,
    ) -> Result<AgentFrameHookPlan, AgentFrameHookPlanCompileError> {
        let compiler =
            self.inner
                .get()
                .ok_or_else(|| AgentFrameHookPlanCompileError::SourceUnavailable {
                    message: "AgentFrame HookPlan compiler composition is not bound".to_string(),
                })?;
        compiler.compile_agent_frame_hook_plan(query).await
    }
}
