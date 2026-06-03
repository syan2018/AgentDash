//! PermissionGrantService — Grant 生命周期编排。
//!
//! 协调 policy evaluation → user approval → compile → capability apply 全链路。

use std::sync::Arc;

use uuid::Uuid;

use crate::ApplicationError;
use crate::session::capability_state::{
    ToolCapabilityDimensionModule, project_capability_state_from_frame,
};
use crate::workflow::AgentFrameBuilder;
use agentdash_domain::DomainError;
use agentdash_domain::permission::{
    GrantScope, PermissionGrant, PermissionGrantRepository, PolicyOutcome, ScopeEscalationIntent,
};
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentProcedureRef, ToolCapabilityPath,
};
use agentdash_spi::platform::tool_capability::capability_to_tool_clusters;
use agentdash_spi::{
    CapabilityState, RuntimeCapabilityTransition, SetToolAccessEffect, ToolCapability,
};

use super::compiler::PermissionGrantCompiler;
use super::policy::PermissionPolicyService;

/// Grant 创建请求参数。
#[derive(Debug, Clone)]
pub struct GrantRequest {
    pub run_id: Uuid,
    pub effect_frame_id: Option<Uuid>,
    pub source_runtime_session_id: String,
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
        transition: RuntimeCapabilityTransition,
        effect_frame: AgentFrame,
    },
    /// 需要用户审批（grant 已持久化为 PendingUserApproval）
    PendingUserApproval { grant: PermissionGrant },
    /// Policy 直接拒绝
    Rejected { grant: PermissionGrant },
}

/// Permission Grant 生命周期服务。
pub struct PermissionGrantService {
    repo: Arc<dyn PermissionGrantRepository>,
    frame_repo: Arc<dyn AgentFrameRepository>,
}

impl PermissionGrantService {
    pub fn new(
        repo: Arc<dyn PermissionGrantRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
    ) -> Self {
        Self { repo, frame_repo }
    }

    /// 创建 grant 请求并执行 policy 评估。
    ///
    /// 调用方需要传入从 ProjectAgent.config 和 AgentProcedureContract 提取的 policy 数据。
    pub async fn request(
        &self,
        req: GrantRequest,
        agent_auto_grantable: &[ToolCapabilityPath],
        lifecycle_requestable: &[ToolCapabilityPath],
    ) -> Result<GrantRequestResult, ApplicationError> {
        let mut grant = PermissionGrant::new(
            req.run_id,
            req.source_runtime_session_id,
            req.requested_paths.clone(),
            req.reason,
            req.grant_scope,
            req.ttl_seconds,
        )
        .with_source(req.source_turn_id, req.source_tool_call_id);

        if let Some(frame_id) = req.effect_frame_id {
            grant = grant.with_effect_frame(frame_id);
        }

        if let Some(intent) = req.scope_escalation_intent {
            grant = grant.with_escalation_intent(intent);
        }

        // Submit for policy
        grant
            .submit_for_policy()
            .map_err(map_grant_transition_error)?;

        // Evaluate policy
        let decision = PermissionPolicyService::evaluate(
            &req.requested_paths,
            agent_auto_grantable,
            lifecycle_requestable,
        );

        grant
            .apply_policy_decision(decision.clone())
            .map_err(map_grant_transition_error)?;

        // Persist grant
        self.repo
            .create(&grant)
            .await
            .map_err(ApplicationError::from)?;

        match decision.outcome {
            PolicyOutcome::AutoApproved => {
                let (transition, effect_frame) = self.apply_frame_effect(&grant, true).await?;
                grant.mark_applied().map_err(map_grant_transition_error)?;
                self.repo
                    .update(&grant)
                    .await
                    .map_err(ApplicationError::from)?;
                Ok(GrantRequestResult::AutoApproved {
                    grant,
                    transition,
                    effect_frame,
                })
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
    ) -> Result<PermissionGrantEffectResult, ApplicationError> {
        let mut grant = self
            .repo
            .find_by_id(grant_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| ApplicationError::NotFound(format!("grant not found: {grant_id}")))?;

        grant
            .user_approve(user_id)
            .map_err(map_grant_transition_error)?;

        let (transition, effect_frame) = self.apply_frame_effect(&grant, true).await?;

        grant.mark_applied().map_err(map_grant_transition_error)?;

        self.repo
            .update(&grant)
            .await
            .map_err(ApplicationError::from)?;

        Ok(PermissionGrantEffectResult {
            grant,
            transition,
            effect_frame,
        })
    }

    /// 用户拒绝 grant。
    pub async fn reject(&self, grant_id: Uuid) -> Result<PermissionGrant, ApplicationError> {
        let mut grant = self
            .repo
            .find_by_id(grant_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| ApplicationError::NotFound(format!("grant not found: {grant_id}")))?;

        grant.user_reject().map_err(map_grant_transition_error)?;

        self.repo
            .update(&grant)
            .await
            .map_err(ApplicationError::from)?;

        Ok(grant)
    }

    /// 显式撤销活跃 grant。
    pub async fn revoke(
        &self,
        grant_id: Uuid,
    ) -> Result<PermissionGrantEffectResult, ApplicationError> {
        let mut grant = self
            .repo
            .find_by_id(grant_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| ApplicationError::NotFound(format!("grant not found: {grant_id}")))?;

        let (transition, effect_frame) = self.apply_frame_effect(&grant, false).await?;

        grant.revoke().map_err(map_grant_transition_error)?;

        self.repo
            .update(&grant)
            .await
            .map_err(ApplicationError::from)?;

        Ok(PermissionGrantEffectResult {
            grant,
            transition,
            effect_frame,
        })
    }

    /// 查询 effect_frame_id 下活跃 grants。
    pub async fn list_active_by_frame(
        &self,
        effect_frame_id: Uuid,
    ) -> Result<Vec<PermissionGrant>, String> {
        self.repo
            .list_active_by_frame(effect_frame_id)
            .await
            .map_err(|e| format!("list_active_by_frame failed: {e}"))
    }

    /// 查询是否有匹配的 scope escalation grant（用于 post-action hook）。
    pub async fn find_active_escalation_grant(
        &self,
        effect_frame_id: Uuid,
        target_subject_kind: &str,
    ) -> Result<Option<PermissionGrant>, String> {
        self.repo
            .find_active_escalation_grant(effect_frame_id, target_subject_kind)
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

    async fn apply_frame_effect(
        &self,
        grant: &PermissionGrant,
        approve: bool,
    ) -> Result<(RuntimeCapabilityTransition, AgentFrame), ApplicationError> {
        let effect_frame_id = grant.effect_frame_id.ok_or_else(|| {
            ApplicationError::BadRequest(format!(
                "permission grant {} missing effect_frame_id; cannot apply capability effect",
                grant.id
            ))
        })?;
        let anchor_frame = self
            .frame_repo
            .get(effect_frame_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| {
                ApplicationError::NotFound(format!("effect frame not found: {effect_frame_id}"))
            })?;
        let current_frame = self
            .frame_repo
            .get_current(anchor_frame.agent_id)
            .await
            .map_err(ApplicationError::from)?
            .unwrap_or(anchor_frame);

        let mut next_state = project_capability_state_from_frame(&current_frame);
        apply_requested_paths(&mut next_state, &grant.requested_paths, approve)?;

        let transition = transition_for_state(grant, &next_state, approve)?;
        let mut builder = AgentFrameBuilder::new(current_frame.agent_id)
            .with_capability_state(&next_state)
            .with_created_by(
                if approve {
                    "permission_grant_approve"
                } else {
                    "permission_grant_revoke"
                },
                Some(grant.id.to_string()),
            );

        if let Some(procedure_id) = current_frame.procedure_id {
            builder = builder.with_procedure(AgentProcedureRef::ById(procedure_id));
        }
        if let (Some(graph_instance_id), Some(activity_key)) = (
            current_frame.graph_instance_id,
            current_frame.activity_key.clone(),
        ) {
            builder = builder.with_graph_instance(graph_instance_id, activity_key);
        }
        if let Some(context) = current_frame.context_slice_json.clone() {
            builder = builder.with_context(context);
        }
        if let Some(profile) = current_frame.execution_profile_json.clone() {
            builder = builder.with_execution_profile_raw(profile);
        }
        let effect_frame = builder
            .build(self.frame_repo.as_ref())
            .await
            .map_err(ApplicationError::from)?;
        Ok((transition, effect_frame))
    }
}

#[derive(Debug)]
pub struct PermissionGrantEffectResult {
    pub grant: PermissionGrant,
    pub transition: RuntimeCapabilityTransition,
    pub effect_frame: AgentFrame,
}

fn transition_for_state(
    grant: &PermissionGrant,
    state: &CapabilityState,
    approve: bool,
) -> Result<RuntimeCapabilityTransition, ApplicationError> {
    let mut transition = if approve {
        PermissionGrantCompiler::compile(grant)
    } else {
        PermissionGrantCompiler::compile_revoke(grant)
    };
    transition.effects.push(
        ToolCapabilityDimensionModule::set_tool_access_effect(SetToolAccessEffect {
            capabilities: state.tool.capabilities.clone(),
            enabled_clusters: state.tool.enabled_clusters.clone(),
            tool_policy: state.tool.tool_policy.clone(),
        })
        .map_err(ApplicationError::Internal)?,
    );
    Ok(transition)
}

fn apply_requested_paths(
    state: &mut CapabilityState,
    paths: &[ToolCapabilityPath],
    approve: bool,
) -> Result<(), ApplicationError> {
    for path in paths {
        if approve {
            apply_add_path(state, path);
        } else {
            apply_remove_path(state, path);
        }
    }
    recompute_tool_clusters(state);
    Ok(())
}

fn apply_add_path(state: &mut CapabilityState, path: &ToolCapabilityPath) {
    let capability = ToolCapability::new(path.capability.clone());
    let already_full = state.tool.capabilities.contains(&capability)
        && state
            .tool
            .tool_policy
            .get(&path.capability)
            .is_none_or(|filter| filter.include_only.is_empty());
    state.tool.capabilities.insert(capability);

    match &path.tool {
        Some(tool) if !already_full => {
            state
                .tool
                .tool_policy
                .entry(path.capability.clone())
                .or_default()
                .include_only
                .insert(tool.clone());
        }
        None => {
            if let Some(filter) = state.tool.tool_policy.get_mut(&path.capability) {
                filter.include_only.clear();
                filter.exclude.clear();
            }
            state
                .tool
                .tool_policy
                .retain(|_, filter| !filter.is_empty());
        }
        Some(_) => {}
    }
}

fn apply_remove_path(state: &mut CapabilityState, path: &ToolCapabilityPath) {
    match &path.tool {
        Some(tool) => {
            if let Some(filter) = state.tool.tool_policy.get_mut(&path.capability) {
                filter.include_only.remove(tool);
                if filter.include_only.is_empty() && filter.exclude.is_empty() {
                    state.tool.tool_policy.remove(&path.capability);
                    state
                        .tool
                        .capabilities
                        .remove(&ToolCapability::new(path.capability.clone()));
                }
            }
        }
        None => {
            state
                .tool
                .capabilities
                .remove(&ToolCapability::new(path.capability.clone()));
            state.tool.tool_policy.remove(&path.capability);
        }
    }
}

fn recompute_tool_clusters(state: &mut CapabilityState) {
    state.tool.enabled_clusters.clear();
    for capability in &state.tool.capabilities {
        state
            .tool
            .enabled_clusters
            .extend(capability_to_tool_clusters(capability));
    }
}

fn map_grant_transition_error(error: DomainError) -> ApplicationError {
    match error {
        DomainError::InvalidTransition { .. } => ApplicationError::BadRequest(error.to_string()),
        other => ApplicationError::from(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::permission::{
        GrantStatus, PermissionGrantRepository, PolicyDecision, PolicyOutcome,
    };
    use agentdash_domain::workflow::AgentFrameRepository;
    use agentdash_spi::ToolCluster;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct InMemoryGrantRepo {
        items: Mutex<Vec<PermissionGrant>>,
    }

    #[async_trait::async_trait]
    impl PermissionGrantRepository for InMemoryGrantRepo {
        async fn create(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
            self.items.lock().await.push(grant.clone());
            Ok(())
        }

        async fn update(&self, grant: &PermissionGrant) -> Result<(), DomainError> {
            let mut items = self.items.lock().await;
            let existing = items
                .iter_mut()
                .find(|item| item.id == grant.id)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "permission_grant",
                    id: grant.id.to_string(),
                })?;
            *existing = grant.clone();
            Ok(())
        }

        async fn find_by_id(&self, id: Uuid) -> Result<Option<PermissionGrant>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .find(|grant| grant.id == id)
                .cloned())
        }

        async fn list_active_by_frame(
            &self,
            effect_frame_id: Uuid,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|grant| {
                    grant.effect_frame_id == Some(effect_frame_id) && grant.status.is_active()
                })
                .cloned()
                .collect())
        }

        async fn list_active_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|grant| grant.run_id == run_id && grant.status.is_active())
                .cloned()
                .collect())
        }

        async fn find_active_escalation_grant(
            &self,
            effect_frame_id: Uuid,
            target_subject_kind: &str,
        ) -> Result<Option<PermissionGrant>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .find(|grant| {
                    grant.effect_frame_id == Some(effect_frame_id)
                        && grant.status == GrantStatus::Applied
                        && grant
                            .scope_escalation_intent
                            .as_ref()
                            .is_some_and(|intent| intent.target_subject_kind == target_subject_kind)
                })
                .cloned())
        }

        async fn expire_overdue(&self) -> Result<u64, DomainError> {
            Ok(0)
        }
    }

    #[derive(Default)]
    struct InMemoryFrameRepo {
        items: Mutex<Vec<AgentFrame>>,
    }

    #[async_trait::async_trait]
    impl AgentFrameRepository for InMemoryFrameRepo {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.items.lock().await.push(frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            let mut frames = self
                .items
                .lock()
                .await
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect::<Vec<_>>();
            frames.sort_by_key(|frame| frame.revision);
            Ok(frames.pop())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn append_visible_canvas_mount(
            &self,
            _frame_id: Uuid,
            _mount_id: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    async fn pending_grant(
        repo: &InMemoryGrantRepo,
        frame_id: Uuid,
        path: &str,
    ) -> PermissionGrant {
        let mut grant = PermissionGrant::new(
            Uuid::new_v4(),
            "runtime-session-1",
            vec![ToolCapabilityPath::parse(path).expect("path")],
            "需要临时能力",
            GrantScope::AgentFrame,
            None,
        )
        .with_effect_frame(frame_id);
        grant.submit_for_policy().expect("submit");
        grant
            .apply_policy_decision(PolicyDecision {
                outcome: PolicyOutcome::NeedsUserApproval,
                matched_rules: Vec::new(),
                reason: "需要人工确认".to_string(),
            })
            .expect("policy");
        repo.create(&grant).await.expect("create grant");
        grant
    }

    #[tokio::test]
    async fn approve_writes_agent_frame_capability_revision() {
        let grant_repo = Arc::new(InMemoryGrantRepo::default());
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let agent_id = Uuid::new_v4();
        let initial_frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("runtime-session-1")
            .build(frame_repo.as_ref())
            .await
            .expect("initial frame");
        let grant = pending_grant(&grant_repo, initial_frame.id, "file_write").await;

        let result = PermissionGrantService::new(grant_repo.clone(), frame_repo.clone())
            .approve(grant.id, "user-1")
            .await
            .expect("approve");

        assert_eq!(result.grant.status, GrantStatus::Applied);
        assert_eq!(result.effect_frame.revision, 2);
        assert_eq!(result.transition.effects.len(), 1);

        let current = frame_repo
            .get_current(agent_id)
            .await
            .expect("current lookup")
            .expect("current frame");
        let state = project_capability_state_from_frame(&current);
        assert!(
            state
                .tool
                .capabilities
                .contains(&ToolCapability::new("file_write"))
        );
        assert!(state.tool.enabled_clusters.contains(&ToolCluster::Write));
        assert_eq!(current.agent_id, agent_id);
    }

    /// Grants from different runtime sessions targeting the same `effect_frame_id`
    /// are all returned by `list_active_by_frame` — proving `effect_frame_id` is the
    /// primary query anchor, while `source_runtime_session_id` is audit-only provenance.
    #[tokio::test]
    async fn list_active_by_frame_groups_by_effect_frame_not_session() {
        let grant_repo = Arc::new(InMemoryGrantRepo::default());
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let agent_id = Uuid::new_v4();
        let initial_frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("session-a")
            .build(frame_repo.as_ref())
            .await
            .expect("initial frame");
        let shared_frame_id = initial_frame.id;
        let shared_run_id = Uuid::new_v4();

        for session in ["session-a", "session-b", "session-c"] {
            let mut grant = PermissionGrant::new(
                shared_run_id,
                session,
                vec![ToolCapabilityPath::parse("file_write").expect("path")],
                "test grant",
                GrantScope::AgentFrame,
                None,
            )
            .with_effect_frame(shared_frame_id);
            grant.submit_for_policy().expect("submit");
            grant
                .apply_policy_decision(PolicyDecision {
                    outcome: PolicyOutcome::AutoApproved,
                    matched_rules: vec![],
                    reason: "auto".into(),
                })
                .expect("policy");
            grant.mark_applied().expect("apply");
            grant_repo.create(&grant).await.expect("persist");
        }

        let by_frame = grant_repo
            .list_active_by_frame(shared_frame_id)
            .await
            .expect("list_active_by_frame");
        assert_eq!(
            by_frame.len(),
            3,
            "all three grants share the same effect_frame_id regardless of source session"
        );

        let by_run = grant_repo
            .list_active_by_run(shared_run_id)
            .await
            .expect("list_active_by_run");
        assert_eq!(by_run.len(), 3, "run_id is the other primary query entry");

        let sessions: Vec<&str> = by_frame
            .iter()
            .map(|g| g.source_runtime_session_id.as_str())
            .collect();
        assert!(sessions.contains(&"session-a"));
        assert!(sessions.contains(&"session-b"));
        assert!(sessions.contains(&"session-c"));
    }

    #[tokio::test]
    async fn revoke_writes_agent_frame_capability_revision() {
        let grant_repo = Arc::new(InMemoryGrantRepo::default());
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let agent_id = Uuid::new_v4();
        let initial_frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("runtime-session-1")
            .build(frame_repo.as_ref())
            .await
            .expect("initial frame");
        let grant = pending_grant(&grant_repo, initial_frame.id, "file_write").await;
        let service = PermissionGrantService::new(grant_repo.clone(), frame_repo.clone());

        service.approve(grant.id, "user-1").await.expect("approve");
        let result = service.revoke(grant.id).await.expect("revoke");

        assert_eq!(result.grant.status, GrantStatus::Revoked);
        assert_eq!(result.effect_frame.revision, 3);
        assert_eq!(result.transition.effects.len(), 1);

        let current = frame_repo
            .get_current(agent_id)
            .await
            .expect("current lookup")
            .expect("current frame");
        let state = project_capability_state_from_frame(&current);
        assert!(
            !state
                .tool
                .capabilities
                .contains(&ToolCapability::new("file_write"))
        );
        assert!(!state.tool.enabled_clusters.contains(&ToolCluster::Write));
    }
}
