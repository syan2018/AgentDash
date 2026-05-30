use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::error::DomainError;
use crate::workflow::ToolCapabilityPath;

use super::value_objects::{
    GrantScope, GrantStatus, PolicyDecision, PolicyOutcome, ScopeEscalationIntent,
};

/// Agent 权限授予记录 — Permission System 的核心聚合根。
///
/// 表达一个 Agent（通过 LifecycleRun）对特定 capability paths 的权限申请及其生命周期。
/// 状态转换由 domain 层方法强制校验。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGrant {
    pub id: Uuid,
    /// 来源 LifecycleRun
    pub run_id: Uuid,
    /// 来源 session
    pub session_id: String,
    /// 触发该申请的 turn（审计追溯用）
    pub source_turn_id: Option<String>,
    /// 触发该申请的 tool call（审计追溯用）
    pub source_tool_call_id: Option<String>,
    /// 申请的 capability paths
    pub requested_paths: Vec<ToolCapabilityPath>,
    /// Agent 给出的申请理由
    pub reason: String,
    /// 生效范围
    pub grant_scope: GrantScope,
    /// 过期时间（由 TTL 计算）
    pub expires_at: Option<DateTime<Utc>>,
    /// Scope Escalation 预声明（如果有）
    pub scope_escalation_intent: Option<ScopeEscalationIntent>,
    /// 当前状态
    pub status: GrantStatus,
    /// Policy 评估结果
    pub policy_decision: Option<PolicyDecision>,
    /// 批准人（user_id 或 "system"）
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PermissionGrant {
    /// 创建新的权限申请。初始状态为 Created。
    pub fn new(
        run_id: Uuid,
        session_id: impl Into<String>,
        requested_paths: Vec<ToolCapabilityPath>,
        reason: impl Into<String>,
        grant_scope: GrantScope,
        ttl_seconds: Option<u64>,
    ) -> Self {
        let now = Utc::now();
        let expires_at = ttl_seconds.map(|ttl| now + chrono::Duration::seconds(ttl as i64));

        Self {
            id: Uuid::new_v4(),
            run_id,
            session_id: session_id.into(),
            source_turn_id: None,
            source_tool_call_id: None,
            requested_paths,
            reason: reason.into(),
            grant_scope,
            expires_at,
            scope_escalation_intent: None,
            status: GrantStatus::Created,
            policy_decision: None,
            approved_by: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_source(mut self, turn_id: Option<String>, tool_call_id: Option<String>) -> Self {
        self.source_turn_id = turn_id;
        self.source_tool_call_id = tool_call_id;
        self
    }

    pub fn with_escalation_intent(mut self, intent: ScopeEscalationIntent) -> Self {
        self.scope_escalation_intent = Some(intent);
        self
    }

    // ── 状态转换方法（强制校验） ──

    /// Created → PendingPolicy：提交给 policy engine 评估。
    pub fn submit_for_policy(&mut self) -> Result<(), DomainError> {
        self.require_status(GrantStatus::Created, "submit_for_policy")?;
        self.transition_to(GrantStatus::PendingPolicy);
        Ok(())
    }

    /// PendingPolicy → Approved / PendingUserApproval / Rejected：policy 评估完成。
    pub fn apply_policy_decision(&mut self, decision: PolicyDecision) -> Result<(), DomainError> {
        self.require_status(GrantStatus::PendingPolicy, "apply_policy_decision")?;
        let next = match decision.outcome {
            PolicyOutcome::AutoApproved => GrantStatus::Approved,
            PolicyOutcome::NeedsUserApproval => GrantStatus::PendingUserApproval,
            PolicyOutcome::Rejected => GrantStatus::Rejected,
        };
        self.policy_decision = Some(decision);
        if next == GrantStatus::Approved {
            self.approved_by = Some("system".to_string());
        }
        self.transition_to(next);
        Ok(())
    }

    /// PendingUserApproval → Approved：用户批准。
    pub fn user_approve(&mut self, user_id: impl Into<String>) -> Result<(), DomainError> {
        self.require_status(GrantStatus::PendingUserApproval, "user_approve")?;
        self.approved_by = Some(user_id.into());
        self.transition_to(GrantStatus::Approved);
        Ok(())
    }

    /// PendingUserApproval → Rejected：用户拒绝。
    pub fn user_reject(&mut self) -> Result<(), DomainError> {
        self.require_status(GrantStatus::PendingUserApproval, "user_reject")?;
        self.transition_to(GrantStatus::Rejected);
        Ok(())
    }

    /// Approved → Applied：capability transition 成功应用。
    pub fn mark_applied(&mut self) -> Result<(), DomainError> {
        self.require_status(GrantStatus::Approved, "mark_applied")?;
        self.transition_to(GrantStatus::Applied);
        Ok(())
    }

    /// Approved → Failed：capability transition 应用失败。
    pub fn mark_failed(&mut self) -> Result<(), DomainError> {
        self.require_status(GrantStatus::Approved, "mark_failed")?;
        self.transition_to(GrantStatus::Failed);
        Ok(())
    }

    /// Applied → Expired：TTL 到期。
    pub fn expire(&mut self) -> Result<(), DomainError> {
        self.require_status(GrantStatus::Applied, "expire")?;
        self.transition_to(GrantStatus::Expired);
        Ok(())
    }

    /// Applied → Revoked：显式撤销。
    pub fn revoke(&mut self) -> Result<(), DomainError> {
        self.require_status(GrantStatus::Applied, "revoke")?;
        self.transition_to(GrantStatus::Revoked);
        Ok(())
    }

    /// Applied → ScopeEscalated：scope escalation 完成。
    pub fn mark_scope_escalated(&mut self) -> Result<(), DomainError> {
        self.require_status(GrantStatus::Applied, "mark_scope_escalated")?;
        self.transition_to(GrantStatus::ScopeEscalated);
        Ok(())
    }

    /// 检查 grant 是否已过期（基于当前时间）。
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| Utc::now() > exp)
            .unwrap_or(false)
    }

    // ── 内部辅助 ──

    fn require_status(&self, expected: GrantStatus, _action: &str) -> Result<(), DomainError> {
        if self.status != expected {
            return Err(DomainError::InvalidTransition {
                from: self.status.as_str().to_string(),
                to: expected.as_str().to_string(),
            });
        }
        Ok(())
    }

    fn transition_to(&mut self, next: GrantStatus) {
        self.status = next;
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::RunLinkSubjectKind;

    fn sample_grant() -> PermissionGrant {
        PermissionGrant::new(
            Uuid::new_v4(),
            "session-1",
            vec![ToolCapabilityPath::parse("story_management").unwrap()],
            "需要创建 Story",
            GrantScope::Session,
            Some(3600),
        )
    }

    #[test]
    fn happy_path_auto_approve() {
        let mut grant = sample_grant();
        grant.submit_for_policy().unwrap();
        assert_eq!(grant.status, GrantStatus::PendingPolicy);

        grant
            .apply_policy_decision(PolicyDecision {
                outcome: PolicyOutcome::AutoApproved,
                matched_rules: vec!["agent_role:patrol".into()],
                reason: "auto".into(),
            })
            .unwrap();
        assert_eq!(grant.status, GrantStatus::Approved);
        assert_eq!(grant.approved_by.as_deref(), Some("system"));

        grant.mark_applied().unwrap();
        assert_eq!(grant.status, GrantStatus::Applied);
        assert!(grant.status.is_active());
    }

    #[test]
    fn happy_path_user_approve() {
        let mut grant = sample_grant();
        grant.submit_for_policy().unwrap();
        grant
            .apply_policy_decision(PolicyDecision {
                outcome: PolicyOutcome::NeedsUserApproval,
                matched_rules: vec![],
                reason: "no matching rule".into(),
            })
            .unwrap();
        assert_eq!(grant.status, GrantStatus::PendingUserApproval);

        grant.user_approve("user-42").unwrap();
        assert_eq!(grant.status, GrantStatus::Approved);
        assert_eq!(grant.approved_by.as_deref(), Some("user-42"));
    }

    #[test]
    fn reject_path() {
        let mut grant = sample_grant();
        grant.submit_for_policy().unwrap();
        grant
            .apply_policy_decision(PolicyDecision {
                outcome: PolicyOutcome::Rejected,
                matched_rules: vec!["deny_all".into()],
                reason: "denied".into(),
            })
            .unwrap();
        assert_eq!(grant.status, GrantStatus::Rejected);
        assert!(grant.status.is_terminal());
    }

    #[test]
    fn scope_escalation_path() {
        let mut grant = sample_grant()
            .with_escalation_intent(ScopeEscalationIntent {
                target_subject_kind: RunLinkSubjectKind::Story,
                unlocked_paths: vec![ToolCapabilityPath::parse("task_management").unwrap()],
            });

        grant.submit_for_policy().unwrap();
        grant
            .apply_policy_decision(PolicyDecision {
                outcome: PolicyOutcome::AutoApproved,
                matched_rules: vec![],
                reason: "auto".into(),
            })
            .unwrap();
        grant.mark_applied().unwrap();
        grant.mark_scope_escalated().unwrap();
        assert_eq!(grant.status, GrantStatus::ScopeEscalated);
        assert!(grant.status.is_active());
    }

    #[test]
    fn invalid_transition_errors() {
        let mut grant = sample_grant();
        assert!(grant.mark_applied().is_err());
        assert!(grant.user_approve("x").is_err());

        grant.submit_for_policy().unwrap();
        assert!(grant.submit_for_policy().is_err());
    }

    #[test]
    fn revoke_applied_grant() {
        let mut grant = sample_grant();
        grant.submit_for_policy().unwrap();
        grant
            .apply_policy_decision(PolicyDecision {
                outcome: PolicyOutcome::AutoApproved,
                matched_rules: vec![],
                reason: "auto".into(),
            })
            .unwrap();
        grant.mark_applied().unwrap();
        grant.revoke().unwrap();
        assert_eq!(grant.status, GrantStatus::Revoked);
        assert!(grant.status.is_terminal());
    }
}
