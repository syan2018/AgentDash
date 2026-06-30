use serde::{Deserialize, Serialize};

use crate::common::error::DomainError;
use crate::workflow::ToolCapabilityPath;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantVfsOperation {
    Read,
    List,
    Search,
    Write,
    Exec,
    ApplyPatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantVfsPathScope {
    All,
    Prefix(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionGrantVfsAccessRule {
    pub surface_ref: Option<String>,
    pub mount_id: String,
    pub path_scope: PermissionGrantVfsPathScope,
    pub operations: Vec<PermissionGrantVfsOperation>,
}

impl PermissionGrantVfsAccessRule {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.mount_id.trim().is_empty() {
            return Err(DomainError::InvalidConfig(
                "permission grant VFS access rule mount_id cannot be empty".to_string(),
            ));
        }
        if self
            .surface_ref
            .as_ref()
            .is_some_and(|surface_ref| surface_ref.trim().is_empty())
        {
            return Err(DomainError::InvalidConfig(
                "permission grant VFS access rule surface_ref cannot be empty".to_string(),
            ));
        }
        if self.operations.is_empty() {
            return Err(DomainError::InvalidConfig(
                "permission grant VFS access rule operations cannot be empty".to_string(),
            ));
        }
        if let PermissionGrantVfsPathScope::Prefix(prefix) = &self.path_scope {
            validate_vfs_prefix(prefix)?;
        }
        Ok(())
    }
}

fn validate_vfs_prefix(prefix: &str) -> Result<(), DomainError> {
    if prefix.starts_with('/') || prefix.starts_with('\\') {
        return Err(DomainError::InvalidConfig(
            "permission grant VFS prefix must be mount-relative".to_string(),
        ));
    }
    for segment in prefix.replace('\\', "/").split('/') {
        if segment == ".." {
            return Err(DomainError::InvalidConfig(
                "permission grant VFS prefix cannot escape the mount root".to_string(),
            ));
        }
    }
    Ok(())
}

/// Grant 的生效范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantScope {
    /// 仅当前 turn 有效（turn 结束后自动撤销）
    Turn,
    /// AgentFrame 生命周期有效（frame revision 更替前持续生效）
    AgentFrame,
    /// 绑定到 Activity（activity 完成后自动撤销）
    Activity,
}

impl GrantScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::AgentFrame => "agent_frame",
            Self::Activity => "activity",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "turn" => Some(Self::Turn),
            "agent_frame" => Some(Self::AgentFrame),
            "activity" => Some(Self::Activity),
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

    pub fn parse(s: &str) -> Option<Self> {
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
    pub target_subject_kind: String,
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
            GrantScope::AgentFrame,
            GrantScope::Activity,
        ] {
            assert_eq!(GrantScope::parse(scope.as_str()), Some(scope));
        }
    }

    #[test]
    fn grant_scope_rejects_legacy_values() {
        assert_eq!(GrantScope::parse("session"), None);
        assert_eq!(GrantScope::parse("workflow_step"), None);
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
            assert_eq!(GrantStatus::parse(status.as_str()), Some(status));
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
