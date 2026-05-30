use serde::{Deserialize, Serialize};

use crate::workflow::{RunLinkSubjectKind, ToolCapabilityPath};

/// Grant 的生效范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantScope {
    /// 仅当前 turn 有效（turn 结束后自动撤销）
    Turn,
    /// 整个 session 生命周期有效
    Session,
    /// 绑定到 workflow step（step 完成后自动撤销）
    WorkflowStep,
}

impl GrantScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::Session => "session",
            Self::WorkflowStep => "workflow_step",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim() {
            "turn" => Some(Self::Turn),
            "session" => Some(Self::Session),
            "workflow_step" => Some(Self::WorkflowStep),
            _ => None,
        }
    }
}

/// Grant 状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantStatus {
    Created,
    PendingPolicy,
    PendingUserApproval,
    Approved,
    Rejected,
    Applied,
    Failed,
    Expired,
    Revoked,
    ScopeEscalated,
}

impl GrantStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::PendingPolicy => "pending_policy",
            Self::PendingUserApproval => "pending_user_approval",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Applied => "applied",
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
            Self::ScopeEscalated => "scope_escalated",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim() {
            "created" => Some(Self::Created),
            "pending_policy" => Some(Self::PendingPolicy),
            "pending_user_approval" => Some(Self::PendingUserApproval),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "applied" => Some(Self::Applied),
            "failed" => Some(Self::Failed),
            "expired" => Some(Self::Expired),
            "revoked" => Some(Self::Revoked),
            "scope_escalated" => Some(Self::ScopeEscalated),
            _ => None,
        }
    }

    /// 当前状态是否表示 grant 仍然活跃（工具可用）
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Applied | Self::ScopeEscalated)
    }

    /// 当前状态是否为终态
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Rejected | Self::Failed | Self::Expired | Self::Revoked
        )
    }
}

/// Scope Escalation 意图 — 预声明 grant 批准后 Agent 将要操作的目标 scope。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeEscalationIntent {
    /// 目标 scope 类型（如 Story → Agent 将创建/管理 Story）
    pub target_subject_kind: RunLinkSubjectKind,
    /// 期望获取的具体 capability paths（escalation 后额外解锁的部分）
    pub unlocked_paths: Vec<ToolCapabilityPath>,
}

/// Policy 评估结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub outcome: PolicyOutcome,
    pub matched_rules: Vec<String>,
    pub reason: String,
}

/// Policy 评估结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOutcome {
    AutoApproved,
    NeedsUserApproval,
    Rejected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_scope_roundtrip() {
        for scope in [
            GrantScope::Turn,
            GrantScope::Session,
            GrantScope::WorkflowStep,
        ] {
            assert_eq!(GrantScope::from_str(scope.as_str()), Some(scope));
        }
    }

    #[test]
    fn grant_status_roundtrip() {
        let all = [
            GrantStatus::Created,
            GrantStatus::PendingPolicy,
            GrantStatus::PendingUserApproval,
            GrantStatus::Approved,
            GrantStatus::Rejected,
            GrantStatus::Applied,
            GrantStatus::Failed,
            GrantStatus::Expired,
            GrantStatus::Revoked,
            GrantStatus::ScopeEscalated,
        ];
        for status in all {
            assert_eq!(GrantStatus::from_str(status.as_str()), Some(status));
        }
    }

    #[test]
    fn active_and_terminal_states() {
        assert!(GrantStatus::Applied.is_active());
        assert!(GrantStatus::ScopeEscalated.is_active());
        assert!(!GrantStatus::Approved.is_active());

        assert!(GrantStatus::Rejected.is_terminal());
        assert!(GrantStatus::Expired.is_terminal());
        assert!(GrantStatus::Revoked.is_terminal());
        assert!(!GrantStatus::Applied.is_terminal());
    }
}
