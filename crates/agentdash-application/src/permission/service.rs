//! PermissionGrantService — Grant 生命周期编排。
//!
//! 协调 policy evaluation → user approval → compile → capability apply 全链路。

use std::sync::Arc;

use uuid::Uuid;

use agentdash_domain::permission::{
    GrantScope, PermissionGrant, PermissionGrantRepository, PolicyOutcome, ScopeEscalationIntent,
};
use agentdash_domain::workflow::ToolCapabilityPath;

use super::compiler::PermissionGrantCompiler;
use super::policy::PermissionPolicyService;

/// Grant 创建请求参数。
#[derive(Debug, Clone)]
pub struct GrantRequest {
    pub run_id: Uuid,
    pub session_id: String,
    pub source_turn_id: Option<String>,
    pub source_tool_call_id: Option<String>,
    pub requested_paths: Vec<ToolCapabilityPath>,
    pub reason: String,
    pub grant_scope: GrantScope,
    pub ttl_seconds: Option<u64>,
    pub scope_escalation_intent: Option<ScopeEscalationIntent>,
}

/// Grant 创建结果。
#[derive(Debug)]
pub enum GrantRequestResult {
    /// Policy 自动批准并已编译 transition（调用方应 apply）
    AutoApproved {
        grant: PermissionGrant,
        transition: agentdash_spi::RuntimeCapabilityTransition,
    },
    /// 需要用户审批（grant 已持久化为 PendingUserApproval）
    PendingUserApproval { grant: PermissionGrant },
    /// Policy 直接拒绝
    Rejected { grant: PermissionGrant },
}

/// Permission Grant 生命周期服务。
pub struct PermissionGrantService {
    repo: Arc<dyn PermissionGrantRepository>,
}

impl PermissionGrantService {
    pub fn new(repo: Arc<dyn PermissionGrantRepository>) -> Self {
        Self { repo }
    }

    /// 创建 grant 请求并执行 policy 评估。
    ///
    /// 调用方需要传入从 ProjectAgent.config 和 WorkflowContract 提取的 policy 数据。
    pub async fn request(
        &self,
        req: GrantRequest,
        agent_auto_grantable: &[ToolCapabilityPath],
        lifecycle_requestable: &[ToolCapabilityPath],
    ) -> Result<GrantRequestResult, String> {
        let mut grant = PermissionGrant::new(
            req.run_id,
            req.session_id,
            req.requested_paths.clone(),
            req.reason,
            req.grant_scope,
            req.ttl_seconds,
        )
        .with_source(req.source_turn_id, req.source_tool_call_id);

        if let Some(intent) = req.scope_escalation_intent {
            grant = grant.with_escalation_intent(intent);
        }

        // Submit for policy
        grant
            .submit_for_policy()
            .map_err(|e| format!("submit_for_policy failed: {e}"))?;

        // Evaluate policy
        let decision = PermissionPolicyService::evaluate(
            &req.requested_paths,
            agent_auto_grantable,
            lifecycle_requestable,
        );

        grant
            .apply_policy_decision(decision.clone())
            .map_err(|e| format!("apply_policy_decision failed: {e}"))?;

        // Persist grant
        self.repo
            .create(&grant)
            .await
            .map_err(|e| format!("persist grant failed: {e}"))?;

        match decision.outcome {
            PolicyOutcome::AutoApproved => {
                let transition = PermissionGrantCompiler::compile(&grant);
                grant
                    .mark_applied()
                    .map_err(|e| format!("mark_applied failed: {e}"))?;
                self.repo
                    .update(&grant)
                    .await
                    .map_err(|e| format!("update grant failed: {e}"))?;
                Ok(GrantRequestResult::AutoApproved { grant, transition })
            }
            PolicyOutcome::NeedsUserApproval => {
                Ok(GrantRequestResult::PendingUserApproval { grant })
            }
            PolicyOutcome::Rejected => Ok(GrantRequestResult::Rejected { grant }),
        }
    }

    /// 用户批准 grant → compile → 返回 transition 供调用方 apply。
    pub async fn approve(
        &self,
        grant_id: Uuid,
        user_id: &str,
    ) -> Result<(PermissionGrant, agentdash_spi::RuntimeCapabilityTransition), String> {
        let mut grant = self
            .repo
            .find_by_id(grant_id)
            .await
            .map_err(|e| format!("find grant failed: {e}"))?
            .ok_or_else(|| format!("grant not found: {grant_id}"))?;

        grant
            .user_approve(user_id)
            .map_err(|e| format!("user_approve failed: {e}"))?;

        let transition = PermissionGrantCompiler::compile(&grant);

        grant
            .mark_applied()
            .map_err(|e| format!("mark_applied failed: {e}"))?;

        self.repo
            .update(&grant)
            .await
            .map_err(|e| format!("update grant failed: {e}"))?;

        Ok((grant, transition))
    }

    /// 用户拒绝 grant。
    pub async fn reject(&self, grant_id: Uuid) -> Result<PermissionGrant, String> {
        let mut grant = self
            .repo
            .find_by_id(grant_id)
            .await
            .map_err(|e| format!("find grant failed: {e}"))?
            .ok_or_else(|| format!("grant not found: {grant_id}"))?;

        grant
            .user_reject()
            .map_err(|e| format!("user_reject failed: {e}"))?;

        self.repo
            .update(&grant)
            .await
            .map_err(|e| format!("update grant failed: {e}"))?;

        Ok(grant)
    }

    /// 显式撤销活跃 grant。
    pub async fn revoke(&self, grant_id: Uuid) -> Result<PermissionGrant, String> {
        let mut grant = self
            .repo
            .find_by_id(grant_id)
            .await
            .map_err(|e| format!("find grant failed: {e}"))?
            .ok_or_else(|| format!("grant not found: {grant_id}"))?;

        grant
            .revoke()
            .map_err(|e| format!("revoke failed: {e}"))?;

        self.repo
            .update(&grant)
            .await
            .map_err(|e| format!("update grant failed: {e}"))?;

        Ok(grant)
    }

    /// 查询 session 下活跃 grants。
    pub async fn list_active_by_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<PermissionGrant>, String> {
        self.repo
            .list_active_by_session(session_id)
            .await
            .map_err(|e| format!("list_active_by_session failed: {e}"))
    }

    /// 查询是否有匹配的 scope escalation grant（用于 post-action hook）。
    pub async fn find_active_escalation_grant(
        &self,
        session_id: &str,
        target_subject_kind: &str,
    ) -> Result<Option<PermissionGrant>, String> {
        self.repo
            .find_active_escalation_grant(session_id, target_subject_kind)
            .await
            .map_err(|e| format!("find_active_escalation_grant failed: {e}"))
    }

    /// 标记 grant 已完成 scope escalation。
    pub async fn mark_scope_escalated(&self, grant_id: Uuid) -> Result<PermissionGrant, String> {
        let mut grant = self
            .repo
            .find_by_id(grant_id)
            .await
            .map_err(|e| format!("find grant failed: {e}"))?
            .ok_or_else(|| format!("grant not found: {grant_id}"))?;

        grant
            .mark_scope_escalated()
            .map_err(|e| format!("mark_scope_escalated failed: {e}"))?;

        self.repo
            .update(&grant)
            .await
            .map_err(|e| format!("update grant failed: {e}"))?;

        Ok(grant)
    }
}
