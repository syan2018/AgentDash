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

impl ProductExecutionProfileRef {
    pub fn calculated_digest(&self) -> String {
        canonical_digest(&serde_json::json!({
            "schema": "agentdash.product-execution-profile/v1",
            "profile_key": self.profile_key,
            "profile_revision": self.profile_revision,
            "configuration": self.configuration,
        }))
    }

    pub fn refresh_digest(&mut self) {
        self.profile_digest = self.calculated_digest();
    }

    pub fn validate(&self) -> bool {
        self.profile_revision > 0
            && !self.profile_key.trim().is_empty()
            && self.profile_digest == self.calculated_digest()
            && self.credential_scope.as_ref().is_none_or(|scope| {
                !scope.owner_kind.trim().is_empty()
                    && !scope.owner_id.trim().is_empty()
                    && !scope.credential_ref.trim().is_empty()
            })
    }
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

impl ProductAgentSurfaceFacts {
    pub fn from_frame(frame: &agentdash_domain::workflow::AgentFrame) -> Self {
        let surface = frame.surface_document();
        let mut facts = Self {
            surface_revision: u64::try_from(frame.revision).unwrap_or_default(),
            surface_digest: String::new(),
            capability: surface.capability_state,
            context: surface.context_slice,
            context_source: surface.context_source_snapshot,
            vfs: surface.vfs_surface,
            mcp: surface.mcp_surface,
            hook_plan: surface.hook_plan,
        };
        facts.surface_digest = facts.calculated_digest();
        facts
    }

    pub fn calculated_digest(&self) -> String {
        canonical_digest(&serde_json::json!({
            "schema": "agentdash.product-agent-surface-facts/v1",
            "surface_revision": self.surface_revision,
            "capability": self.capability,
            "context": self.context,
            "context_source": self.context_source,
            "vfs": self.vfs,
            "mcp": self.mcp,
            "hook_plan": self.hook_plan,
        }))
    }

    pub fn validate(&self) -> bool {
        self.surface_revision > 0 && self.surface_digest == self.calculated_digest()
    }
}

fn canonical_digest(value: &serde_json::Value) -> String {
    agentdash_agent_runtime_contract::canonical_json_sha256(value)
        .expect("Product provisioning facts are serializable")
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
        if self.frame.revision == 0 {
            return Err(AgentRunProductRuntimeProvisioningError::InvalidRequest {
                reason: "AgentFrame revision must be positive".to_owned(),
            });
        }
        if !self.execution_profile.validate() {
            return Err(AgentRunProductRuntimeProvisioningError::InvalidRequest {
                reason: "execution profile digest does not cover its immutable facts".to_owned(),
            });
        }
        if !self.surface_facts.validate()
            || self.surface_facts.surface_revision != self.frame.revision
        {
            return Err(AgentRunProductRuntimeProvisioningError::InvalidRequest {
                reason: "surface digest does not cover the pinned AgentFrame facts".to_owned(),
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

/// Product-owned request for replacing the applied surface of an existing Runtime thread.
///
/// The caller has already persisted `frame`; the implementation compiles its immutable surface,
/// advances the Host binding generation exactly once, and leaves Managed Runtime Rebind/Activate
/// to the Product convergence saga.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunProductRuntimeSurfaceRebindRequest {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub idempotency_key: String,
    pub frame: ProductAgentFrameRef,
    pub execution_profile_digest: String,
    pub execution_configuration: serde_json::Value,
    pub surface_facts: ProductAgentSurfaceFacts,
}

impl AgentRunProductRuntimeSurfaceRebindRequest {
    pub fn validate(&self) -> Result<(), AgentRunProductRuntimeProvisioningError> {
        if self.frame.agent_id != self.target.agent_id
            || self.frame.revision == 0
            || self.execution_profile_digest.trim().is_empty()
            || self.idempotency_key.trim().is_empty()
            || self.surface_facts.surface_revision != self.frame.revision
            || !self.surface_facts.validate()
        {
            return Err(AgentRunProductRuntimeProvisioningError::InvalidRequest {
                reason: "surface rebind request does not pin one valid Product frame".to_owned(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunProductRuntimeSurfaceRebindEvidence {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub idempotency_key: String,
    pub previous_generation: u64,
    pub prepared_generation: u64,
    pub frame: ProductAgentFrameRef,
    pub surface_facts_digest: String,
}

#[async_trait]
pub trait AgentRunProductRuntimeSurfaceRebindPort: Send + Sync {
    async fn prepare_runtime_surface_rebind(
        &self,
        request: AgentRunProductRuntimeSurfaceRebindRequest,
    ) -> Result<AgentRunProductRuntimeSurfaceRebindEvidence, AgentRunProductRuntimeProvisioningError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_profile_digest_is_independent_of_json_object_key_order() {
        let mut left = ProductExecutionProfileRef {
            profile_key: "dash".to_owned(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::from_str(r#"{"nested":{"b":2,"a":1},"z":3}"#)
                .expect("left json"),
            credential_scope: None,
        };
        let mut right = ProductExecutionProfileRef {
            configuration: serde_json::from_str(r#"{"z":3,"nested":{"a":1,"b":2}}"#)
                .expect("right json"),
            ..left.clone()
        };

        left.refresh_digest();
        right.refresh_digest();

        assert_eq!(left.profile_digest, right.profile_digest);
        assert!(left.validate());
        assert!(right.validate());
    }

    #[test]
    fn surface_digest_is_independent_of_json_object_key_order_and_detects_tampering() {
        let mut left = ProductAgentSurfaceFacts {
            surface_revision: 3,
            surface_digest: String::new(),
            capability: Some(
                serde_json::from_str(r#"{"tools":{"b":false,"a":true}}"#).expect("left json"),
            ),
            context: None,
            context_source: None,
            vfs: Some(
                serde_json::from_str(r#"{"mount":{"root":"workspace","metadata":{"z":2,"a":1}}}"#)
                    .expect("left vfs"),
            ),
            mcp: None,
            hook_plan: None,
        };
        let mut right = ProductAgentSurfaceFacts {
            capability: Some(
                serde_json::from_str(r#"{"tools":{"a":true,"b":false}}"#).expect("right json"),
            ),
            vfs: Some(
                serde_json::from_str(r#"{"mount":{"metadata":{"a":1,"z":2},"root":"workspace"}}"#)
                    .expect("right vfs"),
            ),
            ..left.clone()
        };
        left.surface_digest = left.calculated_digest();
        right.surface_digest = right.calculated_digest();

        assert_eq!(left.surface_digest, right.surface_digest);
        assert!(left.validate());
        right.capability = Some(serde_json::json!({"tools":{"a":false,"b":false}}));
        assert!(!right.validate());
    }

    #[test]
    fn request_rejects_cross_agent_frame() {
        let mut execution_profile = ProductExecutionProfileRef {
            profile_key: "dash".to_owned(),
            profile_revision: 1,
            profile_digest: String::new(),
            configuration: serde_json::json!({"executor": "DASH_AGENT"}),
            credential_scope: None,
        };
        execution_profile.refresh_digest();
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
            execution_profile,
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
