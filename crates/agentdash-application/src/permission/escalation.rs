//! Scope Escalation 协调器。
//!
//! 当 Agent 使用 granted capability 执行 scope-creating action（如创建 Story）后，
//! 本模块负责验证 escalation intent、创建 control-scope subject association、触发 scope upgrade。

use std::sync::Arc;

use uuid::Uuid;

use agentdash_domain::permission::PermissionGrantRepository;
use agentdash_domain::workflow::{
    LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository, SubjectRef,
};

/// Scope Escalation 成功后的结果。
#[derive(Debug)]
pub struct EscalationResult {
    pub grant_id: Uuid,
    pub association: LifecycleSubjectAssociation,
    /// Escalation 后新增解锁的 capability paths（来自 intent.unlocked_paths）
    pub unlocked_paths: Vec<agentdash_domain::workflow::ToolCapabilityPath>,
}

/// Scope Escalation 协调服务。
///
/// 典型调用时机：Agent 通过 granted tool 创建 Story/Task 后，平台 MCP handler
/// 调用 `try_escalate` 检查是否需要升级 scope。
pub struct ScopeEscalationCoordinator {
    grant_repo: Arc<dyn PermissionGrantRepository>,
    association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
}

impl ScopeEscalationCoordinator {
    pub fn new(
        grant_repo: Arc<dyn PermissionGrantRepository>,
        association_repo: Arc<dyn LifecycleSubjectAssociationRepository>,
    ) -> Self {
        Self {
            grant_repo,
            association_repo,
        }
    }

    /// 尝试执行 scope escalation。
    ///
    /// - `effect_frame_id`: 当前 agent frame
    /// - `created_subject_kind`: 刚刚创建的对象类型（如 Story）
    /// - `created_subject_id`: 刚刚创建的对象 ID
    ///
    /// 如果存在匹配的 active escalation grant，执行：
    /// 1. 创建 LifecycleSubjectAssociation(role=control_scope)
    /// 2. 标记 grant 为 ScopeEscalated
    /// 3. 返回 escalation 结果（包含 unlocked_paths）
    pub async fn try_escalate(
        &self,
        effect_frame_id: Uuid,
        created_subject_kind: &str,
        created_subject_id: Uuid,
    ) -> Result<Option<EscalationResult>, String> {
        let grant = self
            .grant_repo
            .find_active_escalation_grant(effect_frame_id, created_subject_kind)
            .await
            .map_err(|e| format!("find escalation grant: {e}"))?;

        let Some(mut grant) = grant else {
            return Ok(None);
        };

        // 验证 intent 匹配
        let intent = match &grant.scope_escalation_intent {
            Some(intent) if intent.target_subject_kind == created_subject_kind => intent.clone(),
            _ => return Ok(None),
        };

        let subject = SubjectRef::new(created_subject_kind, created_subject_id);
        let association = LifecycleSubjectAssociation::new_run_scoped(
            grant.run_id,
            &subject,
            "control_scope",
            None,
        );

        self.association_repo
            .create(&association)
            .await
            .map_err(|e| format!("create control_scope association: {e}"))?;

        // 标记 grant 为 ScopeEscalated
        grant
            .mark_scope_escalated()
            .map_err(|e| format!("mark_scope_escalated: {e}"))?;

        self.grant_repo
            .update(&grant)
            .await
            .map_err(|e| format!("update grant: {e}"))?;

        Ok(Some(EscalationResult {
            grant_id: grant.id,
            association,
            unlocked_paths: intent.unlocked_paths,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalation_result_carries_unlocked_paths() {
        let result = EscalationResult {
            grant_id: Uuid::new_v4(),
            association: LifecycleSubjectAssociation::new_run_scoped(
                Uuid::new_v4(),
                &SubjectRef::new("story", Uuid::new_v4()),
                "control_scope",
                None,
            ),
            unlocked_paths: vec![
                agentdash_domain::workflow::ToolCapabilityPath::parse("task_management").unwrap(),
            ],
        };
        assert_eq!(result.unlocked_paths.len(), 1);
        assert_eq!(result.association.role, "control_scope");
    }
}
