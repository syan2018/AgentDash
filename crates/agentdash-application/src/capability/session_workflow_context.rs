//! Session workflow context 解析
//!
//! 统一入口：给定 session owner 上下文，解析出 (has_active_workflow, workflow_capability_directives)，
//! 取代 session 创建路径上分散的 `false / None` 硬编码。
//!
//! 覆盖：
//! - `Project { project_id, agent_id }` —— 直接查 agent_link
//! - `Story { project_id }` —— 查 `is_default_for_story=true` 的 agent_link
//! - `Routine { project_id, agent_id }` —— 复用 Project 查询（routine 自带 agent 绑定）
//!
//! Task session 已在 session_runtime_inputs / turn_context 就地拿到 `ActiveWorkflowProjection`，
//! 无需走 helper；那边直接用 `capability_directives_from_active_workflow` 做单步计算即可。
//!
//! 错误处理哲学：容忍 & 向后兼容——repo 报错 / 未找到 / 未配置统一回退到
//! [`SessionWorkflowContext::NONE`]，只记录 `tracing::warn!`，不中断 session 创建。

use uuid::Uuid;

use agentdash_domain::agent::ProjectAgentLinkRepository;
use agentdash_domain::workflow::{
    CapabilityDirective, LifecycleDefinition, LifecycleDefinitionRepository,
    LifecycleStepDefinition, WorkflowDefinition, WorkflowDefinitionRepository,
};

/// session bootstrap 阶段要注入 resolver 的 workflow 上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWorkflowContext {
    pub has_active_workflow: bool,
    pub workflow_capability_directives: Option<Vec<CapabilityDirective>>,
}

impl SessionWorkflowContext {
    /// 未绑定 / 无法解析时的中性返回值，保持与现状一致的默认行为。
    pub const NONE: Self = Self {
        has_active_workflow: false,
        workflow_capability_directives: None,
    };
}

/// session owner 描述符。
#[derive(Debug, Clone, Copy)]
pub enum SessionWorkflowOwner {
    /// Project 级 session：agent_link 由 (project_id, agent_id) 唯一决定。
    Project { project_id: Uuid, agent_id: Uuid },
    /// Story 级 session：找 project 内 `is_default_for_story=true` 的 agent_link。
    Story { project_id: Uuid },
    /// Routine 触发的 session：routine 自带 project + agent 绑定，与 Project 路径等价。
    Routine { project_id: Uuid, agent_id: Uuid },
}

/// helper 所需的 repository 依赖。调用方从 AppState 传 `Arc<dyn _>` 的 `as_ref()` 即可。
pub struct SessionWorkflowRepos<'a> {
    pub agent_link: &'a dyn ProjectAgentLinkRepository,
    pub lifecycle_def: &'a dyn LifecycleDefinitionRepository,
    /// workflow_def 在解析链末端用于拉取 entry step 对应的 WorkflowDefinition,
    /// 其 `contract.capability_directives` 即 session bootstrap baseline。
    pub workflow_def: &'a dyn WorkflowDefinitionRepository,
}

/// 解析 session bootstrap workflow 上下文。
///
/// 规则：
/// - 成功解析到 lifecycle entry step → 对应 workflow → `contract.capability_directives` →
///   返回 `(true, Some(directives))`
/// - 其余情况（未绑定 / 查询失败 / 配置缺失）→ 返回 [`SessionWorkflowContext::NONE`]
pub async fn resolve_session_workflow_context(
    repos: SessionWorkflowRepos<'_>,
    owner: SessionWorkflowOwner,
) -> SessionWorkflowContext {
    match owner {
        SessionWorkflowOwner::Project {
            project_id,
            agent_id,
        }
        | SessionWorkflowOwner::Routine {
            project_id,
            agent_id,
        } => resolve_for_project_agent(repos, project_id, agent_id).await,
        SessionWorkflowOwner::Story { project_id } => resolve_for_story(repos, project_id).await,
    }
}

/// 从 (project_id, agent_id) 解析 → agent_link → lifecycle entry step → workflow capabilities。
async fn resolve_for_project_agent(
    repos: SessionWorkflowRepos<'_>,
    project_id: Uuid,
    agent_id: Uuid,
) -> SessionWorkflowContext {
    let link = match repos
        .agent_link
        .find_by_project_and_agent(project_id, agent_id)
        .await
    {
        Ok(Some(link)) => link,
        Ok(None) => return SessionWorkflowContext::NONE,
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                agent_id = %agent_id,
                error = %error,
                "resolve_session_workflow_context: 读取 agent_link 失败，回退到空 workflow 上下文"
            );
            return SessionWorkflowContext::NONE;
        }
    };

    let Some(lifecycle_key) = normalize_lifecycle_key(link.default_lifecycle_key.as_deref()) else {
        return SessionWorkflowContext::NONE;
    };

    resolve_from_lifecycle_key(
        repos.lifecycle_def,
        repos.workflow_def,
        project_id,
        &lifecycle_key,
    )
    .await
}

/// 从 project 内 `is_default_for_story=true` 的 agent_link 查 lifecycle。
async fn resolve_for_story(
    repos: SessionWorkflowRepos<'_>,
    project_id: Uuid,
) -> SessionWorkflowContext {
    let links = match repos.agent_link.list_by_project(project_id).await {
        Ok(links) => links,
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                error = %error,
                "resolve_session_workflow_context: Story - 读取 agent_link 列表失败"
            );
            return SessionWorkflowContext::NONE;
        }
    };

    let Some(link) = links.iter().find(|l| l.is_default_for_story) else {
        return SessionWorkflowContext::NONE;
    };

    let Some(lifecycle_key) = normalize_lifecycle_key(link.default_lifecycle_key.as_deref()) else {
        return SessionWorkflowContext::NONE;
    };

    resolve_from_lifecycle_key(
        repos.lifecycle_def,
        repos.workflow_def,
        project_id,
        &lifecycle_key,
    )
    .await
}

async fn resolve_from_lifecycle_key(
    lifecycle_def: &dyn LifecycleDefinitionRepository,
    workflow_def: &dyn WorkflowDefinitionRepository,
    project_id: Uuid,
    lifecycle_key: &str,
) -> SessionWorkflowContext {
    let lifecycle = match lifecycle_def
        .get_by_project_and_key(project_id, lifecycle_key)
        .await
    {
        Ok(Some(def)) => def,
        Ok(None) => {
            tracing::warn!(
                project_id = %project_id,
                lifecycle_key = %lifecycle_key,
                "resolve_session_workflow_context: agent_link 绑定的 lifecycle 不存在"
            );
            return SessionWorkflowContext::NONE;
        }
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                lifecycle_key = %lifecycle_key,
                error = %error,
                "resolve_session_workflow_context: 读取 lifecycle 定义失败"
            );
            return SessionWorkflowContext::NONE;
        }
    };

    let Some(entry_step) = find_entry_step(&lifecycle) else {
        tracing::warn!(
            project_id = %project_id,
            lifecycle_key = %lifecycle_key,
            entry_step_key = %lifecycle.entry_step_key,
            "resolve_session_workflow_context: lifecycle entry step 找不到对应的 step 定义"
        );
        return SessionWorkflowContext::NONE;
    };

    let Some(workflow_key) = entry_step.effective_workflow_key() else {
        // entry step 没有绑定 workflow，无法推断能力基线，但活跃标志仍成立。
        return SessionWorkflowContext {
            has_active_workflow: true,
            workflow_capability_directives: Some(Vec::new()),
        };
    };

    let workflow = match workflow_def
        .get_by_project_and_key(project_id, workflow_key)
        .await
    {
        Ok(Some(workflow)) => workflow,
        Ok(None) => {
            tracing::warn!(
                project_id = %project_id,
                lifecycle_key = %lifecycle_key,
                workflow_key = %workflow_key,
                "resolve_session_workflow_context: entry step 引用的 workflow 不存在"
            );
            return SessionWorkflowContext::NONE;
        }
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                lifecycle_key = %lifecycle_key,
                workflow_key = %workflow_key,
                error = %error,
                "resolve_session_workflow_context: 读取 workflow 定义失败"
            );
            return SessionWorkflowContext::NONE;
        }
    };

    SessionWorkflowContext {
        has_active_workflow: true,
        workflow_capability_directives: Some(capability_directives_from_active_workflow(&workflow)),
    }
}

fn find_entry_step(lifecycle: &LifecycleDefinition) -> Option<&LifecycleStepDefinition> {
    lifecycle
        .steps
        .iter()
        .find(|step| step.key == lifecycle.entry_step_key)
}

fn normalize_lifecycle_key(raw: Option<&str>) -> Option<String> {
    match raw {
        Some(key) if !key.trim().is_empty() => Some(key.trim().to_string()),
        _ => None,
    }
}

/// 从当前活跃 workflow 的 `contract.capability_directives` 构建 session 基线能力指令。
///
/// session bootstrap 阶段直接使用这些 directive；hook runtime 的动态增减（`CapabilityDirective`）
/// 由 workflow 级调用方在运行时叠加。
///
/// 新模型中 workflow.contract.capability_directives 本身就是 directive 序列，
/// 因此直接 clone 即可 —— 不再像旧模型那样需要把条目包装成 Add 指令。
pub fn capability_directives_from_active_workflow(
    workflow: &WorkflowDefinition,
) -> Vec<CapabilityDirective> {
    workflow.contract.capability_directives.clone()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use tokio::sync::Mutex;

    use agentdash_domain::agent::{ProjectAgentLink, ProjectAgentLinkRepository};
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::workflow::{
        CapabilityDirective, LifecycleDefinition, LifecycleDefinitionRepository,
        LifecycleStepDefinition, WorkflowBindingKind, WorkflowContract, WorkflowDefinition,
        WorkflowDefinitionRepository, WorkflowDefinitionSource,
    };

    use super::*;

    const ENTRY_WORKFLOW_KEY: &str = "builtin_workflow_admin_plan";

    // ── in-memory mocks ──────────────────────────────────────────────

    #[derive(Default, Clone)]
    struct MockAgentLinkRepo {
        links: Arc<Mutex<Vec<ProjectAgentLink>>>,
    }

    impl MockAgentLinkRepo {
        async fn insert(&self, link: ProjectAgentLink) {
            self.links.lock().await.push(link);
        }
    }

    #[async_trait]
    impl ProjectAgentLinkRepository for MockAgentLinkRepo {
        async fn create(&self, link: &ProjectAgentLink) -> Result<(), DomainError> {
            self.links.lock().await.push(link.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectAgentLink>, DomainError> {
            Ok(self.links.lock().await.iter().find(|l| l.id == id).cloned())
        }
        async fn find_by_project_and_agent(
            &self,
            project_id: Uuid,
            agent_id: Uuid,
        ) -> Result<Option<ProjectAgentLink>, DomainError> {
            Ok(self
                .links
                .lock()
                .await
                .iter()
                .find(|l| l.project_id == project_id && l.agent_id == agent_id)
                .cloned())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectAgentLink>, DomainError> {
            Ok(self
                .links
                .lock()
                .await
                .iter()
                .filter(|l| l.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn list_by_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<ProjectAgentLink>, DomainError> {
            Ok(self
                .links
                .lock()
                .await
                .iter()
                .filter(|l| l.agent_id == agent_id)
                .cloned()
                .collect())
        }
        async fn update(&self, link: &ProjectAgentLink) -> Result<(), DomainError> {
            let mut lock = self.links.lock().await;
            if let Some(existing) = lock.iter_mut().find(|l| l.id == link.id) {
                *existing = link.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.links.lock().await.retain(|l| l.id != id);
            Ok(())
        }
        async fn delete_by_project_and_agent(
            &self,
            project_id: Uuid,
            agent_id: Uuid,
        ) -> Result<(), DomainError> {
            self.links
                .lock()
                .await
                .retain(|l| !(l.project_id == project_id && l.agent_id == agent_id));
            Ok(())
        }
    }

    #[derive(Default, Clone)]
    struct MockLifecycleDefRepo {
        defs: Arc<Mutex<Vec<LifecycleDefinition>>>,
    }

    impl MockLifecycleDefRepo {
        async fn insert(&self, def: LifecycleDefinition) {
            self.defs.lock().await.push(def);
        }
    }

    #[async_trait]
    impl LifecycleDefinitionRepository for MockLifecycleDefRepo {
        async fn create(&self, def: &LifecycleDefinition) -> Result<(), DomainError> {
            self.defs.lock().await.push(def.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self.defs.lock().await.iter().find(|d| d.id == id).cloned())
        }
        async fn get_by_key(&self, key: &str) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .find(|d| d.key == key)
                .cloned())
        }
        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<LifecycleDefinition>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .find(|d| d.project_id == project_id && d.key == key)
                .cloned())
        }
        async fn list_all(&self) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self.defs.lock().await.clone())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .filter(|d| d.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn list_by_binding_kind(
            &self,
            binding_kind: WorkflowBindingKind,
        ) -> Result<Vec<LifecycleDefinition>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .filter(|d| d.binding_kind == binding_kind)
                .cloned()
                .collect())
        }
        async fn update(&self, def: &LifecycleDefinition) -> Result<(), DomainError> {
            let mut lock = self.defs.lock().await;
            if let Some(existing) = lock.iter_mut().find(|d| d.id == def.id) {
                *existing = def.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.defs.lock().await.retain(|d| d.id != id);
            Ok(())
        }
    }

    #[derive(Default, Clone)]
    struct MockWorkflowDefRepo {
        defs: Arc<Mutex<Vec<WorkflowDefinition>>>,
    }

    impl MockWorkflowDefRepo {
        async fn insert(&self, def: WorkflowDefinition) {
            self.defs.lock().await.push(def);
        }
    }

    #[async_trait]
    impl WorkflowDefinitionRepository for MockWorkflowDefRepo {
        async fn create(&self, def: &WorkflowDefinition) -> Result<(), DomainError> {
            self.defs.lock().await.push(def.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self.defs.lock().await.iter().find(|d| d.id == id).cloned())
        }
        async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .find(|d| d.key == key)
                .cloned())
        }
        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<WorkflowDefinition>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .find(|d| d.project_id == project_id && d.key == key)
                .cloned())
        }
        async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self.defs.lock().await.clone())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .filter(|d| d.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn list_by_binding_kind(
            &self,
            binding_kind: WorkflowBindingKind,
        ) -> Result<Vec<WorkflowDefinition>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .filter(|d| d.binding_kind == binding_kind)
                .cloned()
                .collect())
        }
        async fn update(&self, def: &WorkflowDefinition) -> Result<(), DomainError> {
            let mut lock = self.defs.lock().await;
            if let Some(existing) = lock.iter_mut().find(|d| d.id == def.id) {
                *existing = def.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.defs.lock().await.retain(|d| d.id != id);
            Ok(())
        }
    }

    // ── helpers ──────────────────────────────────────────────────────

    fn make_link(
        project_id: Uuid,
        agent_id: Uuid,
        lifecycle_key: Option<&str>,
    ) -> ProjectAgentLink {
        let mut link = ProjectAgentLink::new(project_id, agent_id);
        link.default_lifecycle_key = lifecycle_key.map(str::to_string);
        link
    }

    /// 构造 `builtin_workflow_admin` lifecycle 及其 entry step workflow。
    /// workflow.contract.capability_directives 放一个 Add("workflow_management"),对应 PRD 的新结构。
    fn admin_lifecycle(project_id: Uuid) -> LifecycleDefinition {
        let plan = LifecycleStepDefinition {
            key: "plan".to_string(),
            description: String::new(),
            workflow_key: Some(ENTRY_WORKFLOW_KEY.to_string()),
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
            task_id: None,
        };
        LifecycleDefinition::new(
            project_id,
            "builtin_workflow_admin",
            "Workflow Admin",
            "",
            WorkflowBindingKind::Project,
            WorkflowDefinitionSource::BuiltinSeed,
            "plan",
            vec![plan],
            vec![],
        )
        .expect("build lifecycle")
    }

    fn admin_entry_workflow(project_id: Uuid) -> WorkflowDefinition {
        let contract = WorkflowContract {
            capability_directives: vec![CapabilityDirective::add_simple("workflow_management")],
            ..WorkflowContract::default()
        };
        WorkflowDefinition::new(
            project_id,
            ENTRY_WORKFLOW_KEY,
            "Workflow Admin / Plan",
            "",
            WorkflowBindingKind::Project,
            WorkflowDefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow definition")
    }

    fn lifecycle_without_entry_step(project_id: Uuid) -> LifecycleDefinition {
        let mut def = admin_lifecycle(project_id);
        def.entry_step_key = "missing".to_string();
        def
    }

    // ── tests ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn project_with_admin_lifecycle_returns_workflow_management_capability() {
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let link_repo = MockAgentLinkRepo::default();
        link_repo
            .insert(make_link(
                project_id,
                agent_id,
                Some("builtin_workflow_admin"),
            ))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        lifecycle_repo.insert(admin_lifecycle(project_id)).await;

        let workflow_repo = MockWorkflowDefRepo::default();
        workflow_repo.insert(admin_entry_workflow(project_id)).await;

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                agent_link: &link_repo,
                lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                agent_id,
            },
        )
        .await;

        assert!(ctx.has_active_workflow);
        assert_eq!(
            ctx.workflow_capability_directives,
            Some(vec![CapabilityDirective::add_simple("workflow_management")])
        );
    }

    #[tokio::test]
    async fn project_link_without_lifecycle_returns_none() {
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let link_repo = MockAgentLinkRepo::default();
        link_repo
            .insert(make_link(project_id, agent_id, None))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        let workflow_repo = MockWorkflowDefRepo::default();

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                agent_link: &link_repo,
                lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                agent_id,
            },
        )
        .await;

        assert_eq!(ctx, SessionWorkflowContext::NONE);
    }

    #[tokio::test]
    async fn project_lifecycle_missing_in_repo_returns_none() {
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let link_repo = MockAgentLinkRepo::default();
        link_repo
            .insert(make_link(project_id, agent_id, Some("nonexistent")))
            .await;

        // lifecycle_repo 不注册任何定义
        let lifecycle_repo = MockLifecycleDefRepo::default();
        let workflow_repo = MockWorkflowDefRepo::default();

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                agent_link: &link_repo,
                lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                agent_id,
            },
        )
        .await;

        assert_eq!(ctx, SessionWorkflowContext::NONE);
    }

    #[tokio::test]
    async fn project_no_agent_link_returns_none() {
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let link_repo = MockAgentLinkRepo::default();
        let lifecycle_repo = MockLifecycleDefRepo::default();
        let workflow_repo = MockWorkflowDefRepo::default();

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                agent_link: &link_repo,
                lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                agent_id,
            },
        )
        .await;

        assert_eq!(ctx, SessionWorkflowContext::NONE);
    }

    #[tokio::test]
    async fn project_lifecycle_with_unknown_entry_step_returns_none() {
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let link_repo = MockAgentLinkRepo::default();
        link_repo
            .insert(make_link(
                project_id,
                agent_id,
                Some("builtin_workflow_admin"),
            ))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        lifecycle_repo
            .insert(lifecycle_without_entry_step(project_id))
            .await;

        let workflow_repo = MockWorkflowDefRepo::default();
        workflow_repo.insert(admin_entry_workflow(project_id)).await;

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                agent_link: &link_repo,
                lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                agent_id,
            },
        )
        .await;

        assert_eq!(ctx, SessionWorkflowContext::NONE);
    }

    /// 端到端：helper → CapabilityResolver。验证 Project session 绑定
    /// `builtin_workflow_admin` 后，session 创建时即获得 `workflow_management` 能力
    /// 与对应的 Workflow 平台 MCP 注入。这是 PR1 的核心回归点。
    #[tokio::test]
    async fn session_creation_with_default_lifecycle_grants_workflow_management() {
        use crate::capability::{CapabilityResolver, CapabilityResolverInput};
        use crate::platform_config::PlatformConfig;

        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let link_repo = MockAgentLinkRepo::default();
        link_repo
            .insert(make_link(
                project_id,
                agent_id,
                Some("builtin_workflow_admin"),
            ))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        lifecycle_repo.insert(admin_lifecycle(project_id)).await;

        let workflow_repo = MockWorkflowDefRepo::default();
        workflow_repo.insert(admin_entry_workflow(project_id)).await;

        // Step 1: helper 解析 workflow 上下文
        let wf_ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                agent_link: &link_repo,
                lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                agent_id,
            },
        )
        .await;

        assert!(wf_ctx.has_active_workflow);

        // Step 2: 将解析结果喂给 CapabilityResolver
        let platform = PlatformConfig {
            mcp_base_url: Some("http://localhost:3001".to_string()),
        };
        let output = CapabilityResolver::resolve(
            &CapabilityResolverInput {
                owner_ctx: agentdash_domain::session_binding::SessionOwnerCtx::Project {
                    project_id,
                },
                agent_declared_capabilities: None,
                workflow_ctx: wf_ctx,
                agent_mcp_servers: vec![],
                available_presets: Default::default(),
                companion_slice_mode: None,
            },
            &platform,
        );

        // 断言 effective_capabilities 包含 workflow_management
        assert!(
            output
                .effective_capabilities
                .iter()
                .any(|cap| cap.key() == "workflow_management"),
            "session 绑定 builtin_workflow_admin 后应获得 workflow_management 能力"
        );

        // 断言 platform_mcp_configs 包含 Workflow scope MCP 注入
        assert!(
            output
                .platform_mcp_configs
                .iter()
                .any(|c| c.endpoint_url().contains("/mcp/workflow/")),
            "应注入 WorkflowMcpServer"
        );
    }

    // ── Story / Routine / Task 辅助测试 ──────────────────────────────

    fn make_story_link(
        project_id: Uuid,
        agent_id: Uuid,
        lifecycle_key: Option<&str>,
        is_default_for_story: bool,
    ) -> ProjectAgentLink {
        let mut link = ProjectAgentLink::new(project_id, agent_id);
        link.default_lifecycle_key = lifecycle_key.map(str::to_string);
        link.is_default_for_story = is_default_for_story;
        link
    }

    #[tokio::test]
    async fn story_with_default_agent_bound_to_admin_lifecycle_grants_capability() {
        let project_id = Uuid::new_v4();
        let story_default_agent = Uuid::new_v4();
        let non_story_agent = Uuid::new_v4();

        let link_repo = MockAgentLinkRepo::default();
        // 非 story-default agent（应被忽略）
        link_repo
            .insert(make_story_link(
                project_id,
                non_story_agent,
                Some("builtin_workflow_admin"),
                false,
            ))
            .await;
        // story-default agent，绑定 admin lifecycle
        link_repo
            .insert(make_story_link(
                project_id,
                story_default_agent,
                Some("builtin_workflow_admin"),
                true,
            ))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        lifecycle_repo.insert(admin_lifecycle(project_id)).await;

        let workflow_repo = MockWorkflowDefRepo::default();
        workflow_repo.insert(admin_entry_workflow(project_id)).await;

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                agent_link: &link_repo,
                lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Story { project_id },
        )
        .await;

        assert!(ctx.has_active_workflow);
        assert_eq!(
            ctx.workflow_capability_directives,
            Some(vec![CapabilityDirective::add_simple("workflow_management")])
        );
    }

    #[tokio::test]
    async fn story_without_default_agent_link_returns_none() {
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let link_repo = MockAgentLinkRepo::default();
        // 有 agent_link 但 is_default_for_story=false
        link_repo
            .insert(make_story_link(
                project_id,
                agent_id,
                Some("builtin_workflow_admin"),
                false,
            ))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        lifecycle_repo.insert(admin_lifecycle(project_id)).await;

        let workflow_repo = MockWorkflowDefRepo::default();
        workflow_repo.insert(admin_entry_workflow(project_id)).await;

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                agent_link: &link_repo,
                lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Story { project_id },
        )
        .await;

        assert_eq!(ctx, SessionWorkflowContext::NONE);
    }

    #[tokio::test]
    async fn routine_with_admin_lifecycle_returns_workflow_management_capability() {
        let project_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let link_repo = MockAgentLinkRepo::default();
        link_repo
            .insert(make_link(
                project_id,
                agent_id,
                Some("builtin_workflow_admin"),
            ))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        lifecycle_repo.insert(admin_lifecycle(project_id)).await;

        let workflow_repo = MockWorkflowDefRepo::default();
        workflow_repo.insert(admin_entry_workflow(project_id)).await;

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                agent_link: &link_repo,
                lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Routine {
                project_id,
                agent_id,
            },
        )
        .await;

        assert!(ctx.has_active_workflow);
        assert_eq!(
            ctx.workflow_capability_directives,
            Some(vec![CapabilityDirective::add_simple("workflow_management")])
        );
    }

    #[test]
    fn capability_directives_from_active_workflow_preserves_directives() {
        let contract = WorkflowContract {
            capability_directives: vec![
                CapabilityDirective::add_simple("workflow_management"),
                CapabilityDirective::add_simple("file_read"),
                CapabilityDirective::remove_simple("shell_execute"),
                CapabilityDirective::add_simple("mcp:code_analyzer"),
            ],
            ..WorkflowContract::default()
        };
        let workflow = WorkflowDefinition::new(
            Uuid::new_v4(),
            "sample",
            "Sample",
            "",
            WorkflowBindingKind::Project,
            WorkflowDefinitionSource::UserAuthored,
            contract,
        )
        .expect("workflow");

        let directives = capability_directives_from_active_workflow(&workflow);
        assert_eq!(directives.len(), 4);
        // 指令序列必须保持与 contract 完全一致（顺序 + 内容）—— 不再做 Add 包装，
        // workflow 声明的 Remove 指令能够传递到 resolver。
        assert!(directives.iter().any(|d| matches!(
            d,
            CapabilityDirective::Remove(path) if path.capability == "shell_execute"
        )));
        assert!(directives.iter().any(|d| matches!(
            d,
            CapabilityDirective::Add(path) if path.capability == "workflow_management"
        )));
    }
}
