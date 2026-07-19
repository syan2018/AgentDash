use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Product 选择 Complete Agent 时使用的稳定执行配置引用。
///
/// Product 只声明希望使用的执行器配置及其不可变摘要；具体 service instance、Host
/// generation、placement 与 callback route 由 provisioning adapter 解析和持久化。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductExecutionProfileRef {
    pub profile_key: String,
    pub profile_revision: u64,
    pub profile_digest: String,
    pub configuration: serde_json::Value,
    /// Stable Product authorization reference used by the Complete Agent
    /// adapter to resolve credentials at execution time. Secrets never enter
    /// AgentFrame or Managed Runtime persistence.
    pub credential_scope: Option<ProductCredentialScopeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductCredentialScopeRef {
    pub owner_kind: String,
    pub owner_id: String,
    pub credential_ref: String,
}

/// Product-owned AgentFrame revision 引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductAgentFrameRef {
    pub frame_id: Uuid,
    pub agent_id: Uuid,
    pub revision: u64,
}

/// Product 从 immutable AgentFrame 读取的 surface 事实。
///
/// Product 不编译 Complete Agent `AgentSurfaceSnapshot`。Provisioning adapter 把这些
/// 平台业务事实交给 Runtime Surface compiler，再完成 offer intersection 与 apply；
/// Complete Agent capability profile、Bound/Applied surface 与 Host route 不会泄漏回
/// Product。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductAgentSurfaceFacts {
    pub surface_revision: u64,
    pub surface_digest: String,
    pub capability: Option<serde_json::Value>,
    pub context: Option<serde_json::Value>,
    pub context_source: Option<serde_json::Value>,
    pub vfs: Option<serde_json::Value>,
    pub mcp: Option<serde_json::Value>,
    pub hook_plan: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunProductRuntimeProvisioningRequest {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub idempotency_key: String,
    pub frame: ProductAgentFrameRef,
    pub execution_profile: ProductExecutionProfileRef,
    pub surface_facts: ProductAgentSurfaceFacts,
}

impl AgentRunProductRuntimeProvisioningRequest {
    pub fn validate(&self) -> Result<(), AgentRunProductRuntimeProvisioningError> {
        if self.frame.agent_id != self.target.agent_id {
            return Err(AgentRunProductRuntimeProvisioningError::InvalidRequest {
                reason: "AgentFrame agent_id does not match AgentRun target".to_owned(),
            });
        }
        for (field, value) in [
            ("idempotency_key", self.idempotency_key.as_str()),
            ("profile_key", self.execution_profile.profile_key.as_str()),
            (
                "profile_digest",
                self.execution_profile.profile_digest.as_str(),
            ),
            ("surface_digest", self.surface_facts.surface_digest.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(AgentRunProductRuntimeProvisioningError::InvalidRequest {
                    reason: format!("{field} cannot be empty"),
                });
            }
        }
        Ok(())
    }
}

/// Host target 已按 Product 输入完成幂等注册的 Product 可见证据。
///
/// 该证据只允许 Product 再向 Managed Runtime 发 Create；Agent source binding 仍必须由
/// Create receipt/inspect 返回，不能由 provisioning 阶段伪造。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunProductRuntimeProvisioningEvidence {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub idempotency_key: String,
    pub frame: ProductAgentFrameRef,
    pub profile_digest: String,
    pub surface_facts_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunProductRuntimeProvisioningError {
    #[error("Product Runtime provisioning request is invalid: {reason}")]
    InvalidRequest { reason: String },
    #[error("Product Runtime provisioning conflicts with existing target registration: {reason}")]
    Conflict { reason: String },
    #[error(
        "Product Runtime provisioning is incompatible with the selected Complete Agent: {reason}"
    )]
    Incompatible { reason: String },
    #[error("Product Runtime provisioning failed: {reason}")]
    Failed { reason: String },
}

#[async_trait]
pub trait AgentRunProductRuntimeProvisioningPort: Send + Sync {
    /// 幂等注册 Runtime target，并完成 Complete Agent selection 与 surface admission。
    ///
    /// 相同 idempotency_key 必须返回完全相同的 evidence；不同请求占用相同 key 时返回
    /// Conflict。方法不会发送 Create/Activate/SubmitInput。
    async fn provision_runtime_target(
        &self,
        request: AgentRunProductRuntimeProvisioningRequest,
    ) -> Result<AgentRunProductRuntimeProvisioningEvidence, AgentRunProductRuntimeProvisioningError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_rejects_cross_agent_frame() {
        let request = AgentRunProductRuntimeProvisioningRequest {
            target: AgentRunTarget {
                run_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
            },
            runtime_thread_id: RuntimeThreadId::new("thread-a").expect("thread"),
            idempotency_key: "provision-a".to_owned(),
            frame: ProductAgentFrameRef {
                frame_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                revision: 1,
            },
            execution_profile: ProductExecutionProfileRef {
                profile_key: "dash".to_owned(),
                profile_revision: 1,
                profile_digest: "sha256:profile".to_owned(),
                configuration: serde_json::json!({"executor": "DASH_AGENT"}),
                credential_scope: None,
            },
            surface_facts: ProductAgentSurfaceFacts {
                surface_revision: 1,
                surface_digest: "sha256:surface".to_owned(),
                capability: None,
                context: None,
                context_source: None,
                vfs: None,
                mcp: None,
                hook_plan: None,
            },
        };

        assert!(matches!(
            request.validate(),
            Err(AgentRunProductRuntimeProvisioningError::InvalidRequest { .. })
        ));
    }
}
