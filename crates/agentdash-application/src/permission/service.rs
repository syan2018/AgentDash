//! PermissionGrantService — Grant 生命周期编排。
//!
//! 协调 policy evaluation → user approval → compile → capability apply 全链路。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::ApplicationError;
use crate::agent_run::RuntimeSurfaceUpdateRequest;
use agentdash_domain::DomainError;
use agentdash_domain::permission::{
    GrantScope, PermissionGrant, PermissionGrantRepository, PolicyOutcome, ScopeEscalationIntent,
};
use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository, ToolCapabilityPath};
use agentdash_spi::RuntimeCapabilityTransition;

use super::policy::PermissionPolicyService;
use super::runtime_surface_update::{
    PermissionRuntimeSurfaceAdopter, PermissionRuntimeSurfaceUpdateService,
};

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
        effect_frame: Option<Box<AgentFrame>>,
    },
    /// 需要用户审批（grant 已持久化为 PendingUserApproval）
    PendingUserApproval { grant: PermissionGrant },
    /// Policy 直接拒绝
    Rejected { grant: PermissionGrant },
}

/// Permission Grant 生命周期服务。
pub struct PermissionGrantService {
    repo: Arc<dyn PermissionGrantRepository>,
    runtime_surface_updates: PermissionRuntimeSurfaceUpdateService,
}

impl PermissionGrantService {
    pub fn new(
        repo: Arc<dyn PermissionGrantRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
    ) -> Self {
        Self {
            repo,
            runtime_surface_updates: PermissionRuntimeSurfaceUpdateService::new(frame_repo),
        }
    }

    pub fn new_with_runtime_surface_adopter(
        repo: Arc<dyn PermissionGrantRepository>,
        frame_repo: Arc<dyn AgentFrameRepository>,
        adopter: Arc<dyn PermissionRuntimeSurfaceAdopter>,
    ) -> Self {
        Self {
            repo,
            runtime_surface_updates: PermissionRuntimeSurfaceUpdateService::with_adopter(
                frame_repo, adopter,
            ),
        }
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
                let outcome = self
                    .runtime_surface_updates
                    .apply_update_request(
                        &grant,
                        RuntimeSurfaceUpdateRequest::PermissionGrantApplied { grant_id: grant.id },
                    )
                    .await?;
                grant.mark_applied().map_err(map_grant_transition_error)?;
                self.repo
                    .update(&grant)
                    .await
                    .map_err(ApplicationError::from)?;
                self.runtime_surface_updates
                    .adopt_update_outcome(&grant, &outcome)
                    .await?;
                Ok(GrantRequestResult::AutoApproved {
                    grant,
                    transition: outcome.transition,
                    effect_frame: outcome.effect_frame.map(Box::new),
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

        let outcome = self
            .runtime_surface_updates
            .apply_update_request(
                &grant,
                RuntimeSurfaceUpdateRequest::PermissionGrantApplied { grant_id },
            )
            .await?;

        grant.mark_applied().map_err(map_grant_transition_error)?;

        self.repo
            .update(&grant)
            .await
            .map_err(ApplicationError::from)?;

        self.runtime_surface_updates
            .adopt_update_outcome(&grant, &outcome)
            .await?;

        Ok(PermissionGrantEffectResult {
            grant,
            transition: outcome.transition,
            effect_frame: outcome.effect_frame,
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

        let outcome = self
            .runtime_surface_updates
            .apply_update_request(
                &grant,
                RuntimeSurfaceUpdateRequest::PermissionGrantRevoked { grant_id },
            )
            .await?;

        grant.revoke().map_err(map_grant_transition_error)?;

        self.repo
            .update(&grant)
            .await
            .map_err(ApplicationError::from)?;

        self.runtime_surface_updates
            .adopt_update_outcome(&grant, &outcome)
            .await?;

        Ok(PermissionGrantEffectResult {
            grant,
            transition: outcome.transition,
            effect_frame: outcome.effect_frame,
        })
    }

    /// 将单个已到期的 active grant 过期，并按 CE02 分类撤销其 AgentRun effect。
    pub async fn expire(
        &self,
        grant_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<PermissionGrantEffectResult, ApplicationError> {
        let mut grant = self
            .repo
            .find_by_id(grant_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or_else(|| ApplicationError::NotFound(format!("grant not found: {grant_id}")))?;

        let expires_at = grant.expires_at.ok_or_else(|| {
            ApplicationError::BadRequest(format!("grant {grant_id} has no expiry"))
        })?;
        if expires_at > now {
            return Err(ApplicationError::BadRequest(format!(
                "grant {grant_id} is not overdue"
            )));
        }

        let outcome = self
            .runtime_surface_updates
            .apply_update_request(
                &grant,
                RuntimeSurfaceUpdateRequest::PermissionGrantRevoked { grant_id },
            )
            .await?;

        grant.expire().map_err(map_grant_transition_error)?;

        self.repo
            .update(&grant)
            .await
            .map_err(ApplicationError::from)?;

        self.runtime_surface_updates
            .adopt_update_outcome(&grant, &outcome)
            .await?;

        Ok(PermissionGrantEffectResult {
            grant,
            transition: outcome.transition,
            effect_frame: outcome.effect_frame,
        })
    }

    /// 批量过期已到期 grant，并逐条复用单 grant 的 AgentRun effect 分类路径。
    pub async fn expire_overdue_with_agent_run_effects(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<PermissionGrantEffectResult>, ApplicationError> {
        let overdue = self
            .repo
            .list_overdue_active(now)
            .await
            .map_err(ApplicationError::from)?;
        let mut results = Vec::with_capacity(overdue.len());
        for grant in overdue {
            results.push(self.expire(grant.id, now).await?);
        }
        Ok(results)
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
}

#[derive(Debug)]
pub struct PermissionGrantEffectResult {
    pub grant: PermissionGrant,
    pub transition: RuntimeCapabilityTransition,
    pub effect_frame: Option<AgentFrame>,
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
    use crate::agent_run::{
        AgentFrameBuilder, AgentFrameSurfaceExt, AgentRunFrameSurfaceError,
        AgentRunGrantProjection, AgentRunSurfaceProjectionContext,
        AgentRunSurfaceProjectionContextResolver, AgentRunSurfaceProjectionContextSource,
    };
    use crate::session::AgentFrameRuntimeTarget;
    use crate::session::capability_state::project_capability_state_from_frame;
    use agentdash_domain::permission::{
        GrantStatus, PermissionGrantRepository, PermissionGrantStatusFilter, PolicyDecision,
        PolicyOutcome,
    };
    use agentdash_domain::workflow::AgentFrameRepository;
    use agentdash_spi::{ToolCapability, ToolCluster};
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

        async fn list_by_frame(
            &self,
            effect_frame_id: Uuid,
            status_filter: Option<PermissionGrantStatusFilter>,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|grant| grant.effect_frame_id == Some(effect_frame_id))
                .filter(|grant| grant_matches_status_filter(grant.status, status_filter))
                .cloned()
                .collect())
        }

        async fn list_by_run(
            &self,
            run_id: Uuid,
            status_filter: Option<PermissionGrantStatusFilter>,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|grant| grant.run_id == run_id)
                .filter(|grant| grant_matches_status_filter(grant.status, status_filter))
                .cloned()
                .collect())
        }

        async fn list_active_by_frame(
            &self,
            effect_frame_id: Uuid,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            self.list_by_frame(effect_frame_id, Some(PermissionGrantStatusFilter::Active))
                .await
        }

        async fn list_active_by_run(
            &self,
            run_id: Uuid,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            self.list_by_run(run_id, Some(PermissionGrantStatusFilter::Active))
                .await
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

        async fn list_overdue_active(
            &self,
            now: DateTime<Utc>,
        ) -> Result<Vec<PermissionGrant>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|grant| grant.status.is_active())
                .filter(|grant| grant.expires_at.is_some_and(|expires_at| expires_at < now))
                .cloned()
                .collect())
        }
    }

    fn grant_matches_status_filter(
        status: GrantStatus,
        status_filter: Option<PermissionGrantStatusFilter>,
    ) -> bool {
        match status_filter {
            Some(PermissionGrantStatusFilter::Exact(expected)) => status == expected,
            Some(PermissionGrantStatusFilter::Pending) => matches!(
                status,
                GrantStatus::Created
                    | GrantStatus::PendingPolicy
                    | GrantStatus::PendingUserApproval
                    | GrantStatus::Approved
            ),
            Some(PermissionGrantStatusFilter::Active) => status.is_active(),
            Some(PermissionGrantStatusFilter::Terminal) => status.is_terminal(),
            None => true,
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

    struct TestSurfaceBoundary {
        frame_repo: Arc<InMemoryFrameRepo>,
        fail_adoption: bool,
    }

    impl TestSurfaceBoundary {
        fn new(frame_repo: Arc<InMemoryFrameRepo>) -> Self {
            Self {
                frame_repo,
                fail_adoption: false,
            }
        }

        fn failing(frame_repo: Arc<InMemoryFrameRepo>) -> Self {
            Self {
                frame_repo,
                fail_adoption: true,
            }
        }
    }

    #[async_trait::async_trait]
    impl AgentRunSurfaceProjectionContextResolver for TestSurfaceBoundary {
        async fn resolve_surface_projection_context(
            &self,
            source: AgentRunSurfaceProjectionContextSource,
        ) -> Result<AgentRunSurfaceProjectionContext, AgentRunFrameSurfaceError> {
            let (effect_frame_id, runtime_session_id) = match source {
                AgentRunSurfaceProjectionContextSource::EffectFrame {
                    effect_frame_id,
                    delivery_runtime_session_id,
                } => (effect_frame_id, delivery_runtime_session_id),
                other => {
                    return Err(AgentRunFrameSurfaceError::ProjectionContextUnavailable(
                        format!("test resolver expected effect frame source, got {other:?}"),
                    ));
                }
            };
            let effect_frame = self
                .frame_repo
                .get(effect_frame_id)
                .await
                .map_err(|error| {
                    AgentRunFrameSurfaceError::ProjectionContextUnavailable(error.to_string())
                })?
                .ok_or_else(|| {
                    AgentRunFrameSurfaceError::ProjectionContextUnavailable(format!(
                        "effect frame not found: {effect_frame_id}"
                    ))
                })?;
            let current_frame = self
                .frame_repo
                .get_current(effect_frame.agent_id)
                .await
                .map_err(|error| {
                    AgentRunFrameSurfaceError::ProjectionContextUnavailable(error.to_string())
                })?
                .unwrap_or(effect_frame);
            let capability_state = project_capability_state_from_frame(&current_frame);
            Ok(AgentRunSurfaceProjectionContext {
                target: AgentFrameRuntimeTarget {
                    frame_id: current_frame.id,
                    delivery_runtime_session_id: runtime_session_id.clone(),
                },
                delivery_runtime_session_id: runtime_session_id,
                active_turn_id: Some("turn-test".to_string()),
                current_frame: current_frame.clone(),
                identity: Some(agentdash_spi::AuthIdentity::system_routine(
                    "permission-test",
                )),
                active_vfs: capability_state.vfs.active.clone(),
                mcp_servers: current_frame.typed_mcp_servers(),
                runtime_backend_anchor: None,
                capability_state,
                skill_discovery_provider_count: 0,
                extra_skill_dirs: Vec::new(),
            })
        }
    }

    #[async_trait::async_trait]
    impl PermissionRuntimeSurfaceAdopter for TestSurfaceBoundary {
        async fn adopt_permission_runtime_surface(
            &self,
            _target: AgentFrameRuntimeTarget,
        ) -> Result<(), String> {
            if self.fail_adoption {
                Err("connector refresh failed".to_string())
            } else {
                Ok(())
            }
        }
    }

    fn permission_service(
        grant_repo: Arc<InMemoryGrantRepo>,
        frame_repo: Arc<InMemoryFrameRepo>,
    ) -> PermissionGrantService {
        PermissionGrantService::new_with_runtime_surface_adopter(
            grant_repo,
            frame_repo.clone(),
            Arc::new(TestSurfaceBoundary::new(frame_repo)),
        )
    }

    async fn pending_grant(
        repo: &InMemoryGrantRepo,
        frame_id: Uuid,
        path: &str,
    ) -> PermissionGrant {
        pending_grant_with_ttl(repo, frame_id, path, None).await
    }

    async fn pending_grant_with_ttl(
        repo: &InMemoryGrantRepo,
        frame_id: Uuid,
        path: &str,
        ttl_seconds: Option<u64>,
    ) -> PermissionGrant {
        let mut grant = PermissionGrant::new(
            Uuid::new_v4(),
            "runtime-session-1",
            vec![ToolCapabilityPath::parse(path).expect("path")],
            "需要临时能力",
            GrantScope::AgentFrame,
            ttl_seconds,
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

        let result = permission_service(grant_repo.clone(), frame_repo.clone())
            .approve(grant.id, "user-1")
            .await
            .expect("approve");

        assert_eq!(result.grant.status, GrantStatus::Applied);
        let effect_frame = result.effect_frame.expect("toolset effect frame");
        assert_eq!(effect_frame.revision, 2);
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
        let service = permission_service(grant_repo.clone(), frame_repo.clone());

        service.approve(grant.id, "user-1").await.expect("approve");
        let result = service.revoke(grant.id).await.expect("revoke");

        assert_eq!(result.grant.status, GrantStatus::Revoked);
        let effect_frame = result.effect_frame.expect("toolset revoke frame");
        assert_eq!(effect_frame.revision, 3);
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

    #[tokio::test]
    async fn approve_returns_visible_error_after_grant_state_success_when_adoption_fails() {
        let grant_repo = Arc::new(InMemoryGrantRepo::default());
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let agent_id = Uuid::new_v4();
        let initial_frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("runtime-session-1")
            .build(frame_repo.as_ref())
            .await
            .expect("initial frame");
        let grant = pending_grant(&grant_repo, initial_frame.id, "file_write").await;

        let error = PermissionGrantService::new_with_runtime_surface_adopter(
            grant_repo.clone(),
            frame_repo.clone(),
            Arc::new(TestSurfaceBoundary::failing(frame_repo.clone())),
        )
        .approve(grant.id, "user-1")
        .await
        .expect_err("adoption failure must be visible");

        assert!(
            error
                .to_string()
                .contains("PermissionGrant active-runtime adoption failed"),
            "unexpected error: {error}"
        );
        let stored = grant_repo
            .find_by_id(grant.id)
            .await
            .expect("grant lookup")
            .expect("grant");
        assert_eq!(stored.status, GrantStatus::Applied);
        assert_eq!(
            frame_repo
                .list_by_agent(agent_id)
                .await
                .expect("frames")
                .len(),
            2,
            "surface revision is persisted before live adoption is attempted"
        );
    }

    #[tokio::test]
    async fn approve_tool_level_grant_uses_admission_projection_without_frame_revision() {
        let grant_repo = Arc::new(InMemoryGrantRepo::default());
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let agent_id = Uuid::new_v4();
        let initial_frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("runtime-session-1")
            .build(frame_repo.as_ref())
            .await
            .expect("initial frame");
        let grant = pending_grant(
            &grant_repo,
            initial_frame.id,
            "workflow_management::upsert_workflow_tool",
        )
        .await;

        let result = permission_service(grant_repo.clone(), frame_repo.clone())
            .approve(grant.id, "user-1")
            .await
            .expect("approve");

        assert_eq!(result.grant.status, GrantStatus::Applied);
        assert!(result.effect_frame.is_none());
        assert!(
            result.transition.effects.is_empty(),
            "admission-only grants must not rewrite AgentFrame capability surface"
        );

        let current = frame_repo
            .get_current(agent_id)
            .await
            .expect("current lookup")
            .expect("current frame");
        assert_eq!(current.revision, 1);

        let active_grants = grant_repo
            .list_active_by_run(result.grant.run_id)
            .await
            .expect("active grants");
        let projection = AgentRunGrantProjection::from_active_grants(&active_grants);
        let admission_request = crate::agent_run::AgentRunAdmissionRequest::tool(
            "workflow_management",
            "upsert_workflow_tool",
            Some(ToolCluster::Workflow),
        );
        assert!(projection.admits_tool(&admission_request));
    }

    #[tokio::test]
    async fn expire_tool_level_grant_revokes_admission_without_frame_revision() {
        let grant_repo = Arc::new(InMemoryGrantRepo::default());
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let agent_id = Uuid::new_v4();
        let initial_frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("runtime-session-1")
            .build(frame_repo.as_ref())
            .await
            .expect("initial frame");
        let mut grant = PermissionGrant::new(
            Uuid::new_v4(),
            "runtime-session-1",
            vec![
                ToolCapabilityPath::parse("workflow_management::upsert_workflow_tool")
                    .expect("path"),
            ],
            "temporary tool admission",
            GrantScope::AgentFrame,
            Some(1),
        )
        .with_effect_frame(initial_frame.id);
        grant.submit_for_policy().expect("submit");
        grant
            .apply_policy_decision(PolicyDecision {
                outcome: PolicyOutcome::NeedsUserApproval,
                matched_rules: Vec::new(),
                reason: "manual".to_string(),
            })
            .expect("policy");
        grant_repo.create(&grant).await.expect("persist");

        let service = permission_service(grant_repo.clone(), frame_repo.clone());
        service.approve(grant.id, "user-1").await.expect("approve");
        let result = service
            .expire(grant.id, Utc::now() + chrono::Duration::seconds(2))
            .await
            .expect("expire");

        assert_eq!(result.grant.status, GrantStatus::Expired);
        assert!(result.effect_frame.is_none());
        assert!(result.transition.effects.is_empty());

        let active_grants = grant_repo
            .list_active_by_run(result.grant.run_id)
            .await
            .expect("active grants");
        let projection = AgentRunGrantProjection::from_active_grants(&active_grants);
        let admission_request = crate::agent_run::AgentRunAdmissionRequest::tool(
            "workflow_management",
            "upsert_workflow_tool",
            Some(ToolCluster::Workflow),
        );
        assert!(!projection.admits_tool(&admission_request));
    }

    #[tokio::test]
    async fn expire_overdue_with_agent_run_effects_applies_each_grant_classification() {
        let grant_repo = Arc::new(InMemoryGrantRepo::default());
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let agent_id = Uuid::new_v4();
        let initial_frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("runtime-session-1")
            .build(frame_repo.as_ref())
            .await
            .expect("initial frame");
        let service = permission_service(grant_repo.clone(), frame_repo.clone());
        let surface_grant =
            pending_grant_with_ttl(&grant_repo, initial_frame.id, "file_write", Some(1)).await;
        let admission_grant = pending_grant_with_ttl(
            &grant_repo,
            initial_frame.id,
            "workflow_management::upsert_workflow_tool",
            Some(1),
        )
        .await;
        service
            .approve(surface_grant.id, "user-1")
            .await
            .expect("approve surface grant");
        service
            .approve(admission_grant.id, "user-1")
            .await
            .expect("approve admission grant");

        let results = service
            .expire_overdue_with_agent_run_effects(Utc::now() + chrono::Duration::seconds(2))
            .await
            .expect("bulk expire");

        assert_eq!(results.len(), 2);
        assert!(
            results.iter().any(|result| result.effect_frame.is_some()),
            "capability-level overdue grant must write remove surface revision"
        );
        assert!(
            results.iter().any(|result| result.effect_frame.is_none()),
            "tool-level overdue grant must stay admission-only"
        );
        assert!(
            results
                .iter()
                .all(|result| result.grant.status == GrantStatus::Expired)
        );

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

        let stored_surface = grant_repo
            .find_by_id(surface_grant.id)
            .await
            .expect("surface lookup")
            .expect("surface grant");
        let stored_admission = grant_repo
            .find_by_id(admission_grant.id)
            .await
            .expect("admission lookup")
            .expect("admission grant");
        assert_eq!(stored_surface.status, GrantStatus::Expired);
        assert_eq!(stored_admission.status, GrantStatus::Expired);
        assert!(
            grant_repo
                .list_active_by_run(stored_surface.run_id)
                .await
                .expect("surface active grants")
                .is_empty()
        );
        assert!(
            grant_repo
                .list_active_by_run(stored_admission.run_id)
                .await
                .expect("admission active grants")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn expire_overdue_with_agent_run_effects_expires_scope_escalated_grant() {
        let grant_repo = Arc::new(InMemoryGrantRepo::default());
        let frame_repo = Arc::new(InMemoryFrameRepo::default());
        let agent_id = Uuid::new_v4();
        let initial_frame = AgentFrameBuilder::new(agent_id)
            .with_runtime_session("runtime-session-1")
            .build(frame_repo.as_ref())
            .await
            .expect("initial frame");
        let mut grant =
            pending_grant_with_ttl(&grant_repo, initial_frame.id, "file_write", Some(1)).await;
        grant.user_approve("user-1").expect("approve");
        grant.mark_applied().expect("applied");
        grant.mark_scope_escalated().expect("scope escalated");
        grant_repo.update(&grant).await.expect("update grant");

        let results = permission_service(grant_repo.clone(), frame_repo.clone())
            .expire_overdue_with_agent_run_effects(Utc::now() + chrono::Duration::seconds(2))
            .await
            .expect("bulk expire");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].grant.status, GrantStatus::Expired);
        let stored = grant_repo
            .find_by_id(grant.id)
            .await
            .expect("lookup")
            .expect("grant");
        assert_eq!(stored.status, GrantStatus::Expired);
    }
}
