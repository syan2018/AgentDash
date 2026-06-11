//! Session workflow context 解析
//!
//! 统一入口：给定 session owner 上下文，解析出 (has_active_workflow, workflow_tool_directives)，
//! 取代 session 创建路径上分散的 `false / None` 硬编码。
//!
//! 覆盖：
//! - `Project { project_id, project_agent_id }` —— 直接查 ProjectAgent
//! - `Story { project_id }` —— 查 `is_default_for_story=true` 的 ProjectAgent
//! - `Routine { project_id, project_agent_id }` —— 复用 Project 查询（routine 自带 agent 绑定）
//!
//! Task session 已在 session_runtime_inputs / turn_context 就地拿到 `ActiveWorkflowProjection`，
//! 无需走 helper；那边直接用 `tool_directives_from_active_workflow` 做单步计算即可。
//!
//! 错误处理哲学：容忍 & 向后兼容——repo 报错 / 未找到 / 未配置统一回退到
//! `None`，只记录 `tracing::warn!`，不中断 session 创建。

use uuid::Uuid;

use agentdash_domain::agent::ProjectAgentRepository;
use agentdash_domain::workflow::{
    ActivityDefinition, ActivityExecutorSpec, AgentProcedure, AgentProcedureRepository,
    ToolCapabilityDirective, WorkflowGraph, WorkflowGraphRepository,
};

use crate::capability::ToolContribution;

/// session owner 描述符。
#[derive(Debug, Clone, Copy)]
pub enum SessionWorkflowOwner {
    /// Project 级 session：ProjectAgent 由 (project_id, project_agent_id) 唯一决定。
    Project {
        project_id: Uuid,
        project_agent_id: Uuid,
    },
    /// Story 级 session：找 project 内 `is_default_for_story=true` 的 ProjectAgent。
    Story { project_id: Uuid },
    /// Routine 触发的 session：routine 自带 project + agent 绑定，与 Project 路径等价。
    Routine {
        project_id: Uuid,
        project_agent_id: Uuid,
    },
}

/// helper 所需的 repository 依赖。调用方从 AppState 传 `Arc<dyn _>` 的 `as_ref()` 即可。
pub struct SessionWorkflowRepos<'a> {
    pub project_agent: &'a dyn ProjectAgentRepository,
    pub activity_lifecycle_def: &'a dyn WorkflowGraphRepository,
    /// workflow_def 在解析链末端用于拉取 entry activity 对应的 AgentProcedure,
    /// 其 `contract.capability_config.tool_directives` 即 session bootstrap baseline。
    pub workflow_def: &'a dyn AgentProcedureRepository,
}

/// 解析 session bootstrap workflow 上下文，直接返回 `Option<ToolContribution>`。
///
/// 规则：
/// - 成功解析到 lifecycle entry step → 对应 workflow → `contract.capability_config.tool_directives` →
///   返回 `Some(ToolContribution { directives, has_active_workflow: true })`
/// - 其余情况（未绑定 / 查询失败 / 配置缺失）→ 返回 `None`
pub async fn resolve_session_workflow_context(
    repos: SessionWorkflowRepos<'_>,
    owner: SessionWorkflowOwner,
) -> Option<ToolContribution> {
    match owner {
        SessionWorkflowOwner::Project {
            project_id,
            project_agent_id,
        }
        | SessionWorkflowOwner::Routine {
            project_id,
            project_agent_id,
        } => resolve_for_project_agent(repos, project_id, project_agent_id).await,
        SessionWorkflowOwner::Story { project_id } => resolve_for_story(repos, project_id).await,
    }
}

async fn resolve_for_project_agent(
    repos: SessionWorkflowRepos<'_>,
    project_id: Uuid,
    project_agent_id: Uuid,
) -> Option<ToolContribution> {
    let agent = match repos
        .project_agent
        .get_by_project_and_id(project_id, project_agent_id)
        .await
    {
        Ok(Some(agent)) => agent,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                project_agent_id = %project_agent_id,
                error = %error,
                "resolve_session_workflow_context: 读取 ProjectAgent 失败，回退到空 workflow 上下文"
            );
            return None;
        }
    };

    let lifecycle_key = normalize_lifecycle_key(agent.default_lifecycle_key.as_deref())?;

    resolve_from_lifecycle_key(
        repos.activity_lifecycle_def,
        repos.workflow_def,
        project_id,
        &lifecycle_key,
    )
    .await
}

async fn resolve_for_story(
    repos: SessionWorkflowRepos<'_>,
    project_id: Uuid,
) -> Option<ToolContribution> {
    let agents = match repos.project_agent.list_by_project(project_id).await {
        Ok(agents) => agents,
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                error = %error,
                "resolve_session_workflow_context: Story - 读取 ProjectAgent 列表失败"
            );
            return None;
        }
    };

    let agent = agents.iter().find(|item| item.is_default_for_story)?;

    let lifecycle_key = normalize_lifecycle_key(agent.default_lifecycle_key.as_deref())?;

    resolve_from_lifecycle_key(
        repos.activity_lifecycle_def,
        repos.workflow_def,
        project_id,
        &lifecycle_key,
    )
    .await
}

async fn resolve_from_lifecycle_key(
    activity_lifecycle_def: &dyn WorkflowGraphRepository,
    workflow_def: &dyn AgentProcedureRepository,
    project_id: Uuid,
    lifecycle_key: &str,
) -> Option<ToolContribution> {
    let lifecycle = match activity_lifecycle_def
        .get_by_project_and_key(project_id, lifecycle_key)
        .await
    {
        Ok(Some(def)) => def,
        Ok(None) => {
            tracing::warn!(
                project_id = %project_id,
                lifecycle_key = %lifecycle_key,
                "resolve_session_workflow_context: ProjectAgent 绑定的 lifecycle 不存在"
            );
            return None;
        }
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                lifecycle_key = %lifecycle_key,
                error = %error,
                "resolve_session_workflow_context: 读取 lifecycle 定义失败"
            );
            return None;
        }
    };

    let Some(entry_activity) = find_entry_activity(&lifecycle) else {
        tracing::warn!(
            project_id = %project_id,
            lifecycle_key = %lifecycle_key,
            entry_activity_key = %lifecycle.entry_activity_key,
            "resolve_session_workflow_context: lifecycle entry activity 找不到对应定义"
        );
        return None;
    };

    let Some(procedure_key) = entry_activity_procedure_key(entry_activity) else {
        return Some(ToolContribution {
            directives: Vec::new(),
            has_active_workflow: true,
        });
    };

    let workflow = match workflow_def
        .get_by_project_and_key(project_id, procedure_key)
        .await
    {
        Ok(Some(workflow)) => workflow,
        Ok(None) => {
            tracing::warn!(
                project_id = %project_id,
                lifecycle_key = %lifecycle_key,
                procedure_key = %procedure_key,
                "resolve_session_workflow_context: entry step 引用的 workflow 不存在"
            );
            return None;
        }
        Err(error) => {
            tracing::warn!(
                project_id = %project_id,
                lifecycle_key = %lifecycle_key,
                procedure_key = %procedure_key,
                error = %error,
                "resolve_session_workflow_context: 读取 workflow 定义失败"
            );
            return None;
        }
    };

    Some(ToolContribution {
        directives: tool_directives_from_active_workflow(&workflow),
        has_active_workflow: true,
    })
}

fn find_entry_activity(lifecycle: &WorkflowGraph) -> Option<&ActivityDefinition> {
    lifecycle
        .activities
        .iter()
        .find(|activity| activity.key == lifecycle.entry_activity_key)
}

fn entry_activity_procedure_key(activity: &ActivityDefinition) -> Option<&str> {
    match &activity.executor {
        ActivityExecutorSpec::Agent(spec) => Some(spec.procedure_key.as_str()),
        _ => None,
    }
}

fn normalize_lifecycle_key(raw: Option<&str>) -> Option<String> {
    match raw {
        Some(key) if !key.trim().is_empty() => Some(key.trim().to_string()),
        _ => None,
    }
}

/// 从当前活跃 workflow 的 `contract.capability_config.tool_directives` 构建 session 基线工具指令。
///
/// session bootstrap 阶段直接使用这些 directive；hook runtime 的动态增减（`ToolCapabilityDirective`）
/// 由 workflow 级调用方在运行时叠加。
pub fn tool_directives_from_active_workflow(
    workflow: &AgentProcedure,
) -> Vec<ToolCapabilityDirective> {
    workflow.contract.capability_config.tool_directives.clone()
}

pub fn tool_directives_from_active_workflow_projection(
    workflow: &crate::workflow::ActiveWorkflowProjection,
) -> Vec<ToolCapabilityDirective> {
    workflow
        .active_contract()
        .map(|contract| contract.capability_config.tool_directives.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use tokio::sync::Mutex;

    use agentdash_domain::agent::{ProjectAgent, ProjectAgentRepository};
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::workflow::{
        ActivityDefinition, ActivityExecutorSpec, AgentActivityExecutorSpec, AgentProcedure,
        AgentProcedureContract, AgentProcedureRepository, DefinitionSource,
        ToolCapabilityDirective, WorkflowGraph, WorkflowGraphRepository,
    };

    use super::*;

    const ENTRY_PROCEDURE_KEY: &str = "builtin_workflow_admin_plan";

    // ── in-memory mocks ──────────────────────────────────────────────

    #[derive(Default, Clone)]
    struct MockProjectAgentRepo {
        agents: Arc<Mutex<Vec<ProjectAgent>>>,
    }

    impl MockProjectAgentRepo {
        async fn insert(&self, agent: ProjectAgent) {
            self.agents.lock().await.push(agent);
        }
    }

    #[async_trait]
    impl ProjectAgentRepository for MockProjectAgentRepo {
        async fn create(&self, agent: &ProjectAgent) -> Result<(), DomainError> {
            self.agents.lock().await.push(agent.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .await
                .iter()
                .find(|l| l.id == id)
                .cloned())
        }
        async fn get_by_project_and_id(
            &self,
            project_id: Uuid,
            id: Uuid,
        ) -> Result<Option<ProjectAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .await
                .iter()
                .find(|l| l.project_id == project_id && l.id == id)
                .cloned())
        }
        async fn get_by_project_and_name(
            &self,
            project_id: Uuid,
            name: &str,
        ) -> Result<Option<ProjectAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .await
                .iter()
                .find(|l| l.project_id == project_id && l.name == name)
                .cloned())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectAgent>, DomainError> {
            Ok(self
                .agents
                .lock()
                .await
                .iter()
                .filter(|l| l.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn update(&self, agent: &ProjectAgent) -> Result<(), DomainError> {
            let mut lock = self.agents.lock().await;
            if let Some(existing) = lock.iter_mut().find(|l| l.id == agent.id) {
                *existing = agent.clone();
            }
            Ok(())
        }
        async fn delete(&self, project_id: Uuid, id: Uuid) -> Result<(), DomainError> {
            self.agents
                .lock()
                .await
                .retain(|l| !(l.project_id == project_id && l.id == id));
            Ok(())
        }
    }

    #[derive(Default, Clone)]
    struct MockLifecycleDefRepo {
        defs: Arc<Mutex<Vec<WorkflowGraph>>>,
    }

    impl MockLifecycleDefRepo {
        async fn insert(&self, def: WorkflowGraph) {
            self.defs.lock().await.push(def);
        }
    }

    #[async_trait]
    impl WorkflowGraphRepository for MockLifecycleDefRepo {
        async fn create(&self, def: &WorkflowGraph) -> Result<(), DomainError> {
            self.defs.lock().await.push(def.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(self.defs.lock().await.iter().find(|d| d.id == id).cloned())
        }
        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<WorkflowGraph>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .find(|d| d.project_id == project_id && d.key == key)
                .cloned())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<WorkflowGraph>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .filter(|d| d.project_id == project_id)
                .cloned()
                .collect())
        }
        async fn update(&self, def: &WorkflowGraph) -> Result<(), DomainError> {
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
        defs: Arc<Mutex<Vec<AgentProcedure>>>,
    }

    impl MockWorkflowDefRepo {
        async fn insert(&self, def: AgentProcedure) {
            self.defs.lock().await.push(def);
        }
    }

    #[async_trait]
    impl AgentProcedureRepository for MockWorkflowDefRepo {
        async fn create(&self, def: &AgentProcedure) -> Result<(), DomainError> {
            self.defs.lock().await.push(def.clone());
            Ok(())
        }
        async fn get_by_id(&self, id: Uuid) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self.defs.lock().await.iter().find(|d| d.id == id).cloned())
        }
        async fn get_by_key(&self, key: &str) -> Result<Option<AgentProcedure>, DomainError> {
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
        ) -> Result<Option<AgentProcedure>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .find(|d| d.project_id == project_id && d.key == key)
                .cloned())
        }
        async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self.defs.lock().await.clone())
        }
        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<AgentProcedure>, DomainError> {
            Ok(self
                .defs
                .lock()
                .await
                .iter()
                .filter(|d| d.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, def: &AgentProcedure) -> Result<(), DomainError> {
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

    fn make_project_agent(
        project_id: Uuid,
        project_agent_id: Uuid,
        lifecycle_key: Option<&str>,
    ) -> ProjectAgent {
        let mut agent = ProjectAgent::new(project_id, "test-agent", "PI_AGENT");
        agent.id = project_agent_id;
        agent.default_lifecycle_key = lifecycle_key.map(str::to_string);
        agent
    }

    fn admin_plan_directives() -> Vec<ToolCapabilityDirective> {
        vec![
            ToolCapabilityDirective::add_simple("workflow_management"),
            ToolCapabilityDirective::remove_tool("workflow_management", "upsert_workflow_tool"),
            ToolCapabilityDirective::remove_tool("workflow_management", "upsert_lifecycle_tool"),
        ]
    }

    /// 构造 `builtin_workflow_admin` lifecycle 及其 entry activity workflow。
    /// Plan 阶段必须保留 workflow_management 只读工具，同时屏蔽 upsert 写入工具。
    fn admin_lifecycle(project_id: Uuid) -> WorkflowGraph {
        let plan = ActivityDefinition {
            key: "plan".to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Agent(
                AgentActivityExecutorSpec::create_activity_agent(ENTRY_PROCEDURE_KEY),
            ),
            output_ports: vec![],
            input_ports: vec![],
            completion_policy: Default::default(),
            iteration_policy: Default::default(),
            join_policy: Default::default(),
        };
        WorkflowGraph::new(WorkflowGraphDraft {
            project_id,
            key: "builtin_workflow_admin".to_string(),
            name: "Workflow Admin".to_string(),
            description: String::new(),
            source: DefinitionSource::BuiltinSeed,
            entry_activity_key: "plan".to_string(),
            activities: vec![plan],
            transitions: vec![],
        })
        .expect("build lifecycle")
    }

    fn admin_entry_workflow(project_id: Uuid) -> AgentProcedure {
        let contract = AgentProcedureContract {
            capability_config: agentdash_domain::workflow::CapabilityConfig {
                tool_directives: admin_plan_directives(),
                ..Default::default()
            },
            ..AgentProcedureContract::default()
        };
        AgentProcedure::new(
            project_id,
            ENTRY_PROCEDURE_KEY,
            "Workflow Admin / Plan",
            "",
            DefinitionSource::BuiltinSeed,
            contract,
        )
        .expect("workflow definition")
    }

    fn lifecycle_without_entry_step(project_id: Uuid) -> WorkflowGraph {
        let mut def = admin_lifecycle(project_id);
        def.entry_activity_key = "missing".to_string();
        def
    }

    // ── tests ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn project_with_admin_lifecycle_returns_workflow_management_capability() {
        let project_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();

        let project_agent_repo = MockProjectAgentRepo::default();
        project_agent_repo
            .insert(make_project_agent(
                project_id,
                project_agent_id,
                Some("builtin_workflow_admin"),
            ))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        lifecycle_repo.insert(admin_lifecycle(project_id)).await;

        let workflow_repo = MockWorkflowDefRepo::default();
        workflow_repo.insert(admin_entry_workflow(project_id)).await;

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                project_agent: &project_agent_repo,
                activity_lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                project_agent_id,
            },
        )
        .await;

        let ctx = ctx.expect("should resolve to ToolContribution");
        assert!(ctx.has_active_workflow);
        assert_eq!(ctx.directives, admin_plan_directives());
    }

    #[tokio::test]
    async fn project_agent_without_lifecycle_returns_none() {
        let project_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();

        let project_agent_repo = MockProjectAgentRepo::default();
        project_agent_repo
            .insert(make_project_agent(project_id, project_agent_id, None))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        let workflow_repo = MockWorkflowDefRepo::default();

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                project_agent: &project_agent_repo,
                activity_lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                project_agent_id,
            },
        )
        .await;

        assert!(ctx.is_none());
    }

    #[tokio::test]
    async fn project_lifecycle_missing_in_repo_returns_none() {
        let project_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();

        let project_agent_repo = MockProjectAgentRepo::default();
        project_agent_repo
            .insert(make_project_agent(
                project_id,
                project_agent_id,
                Some("nonexistent"),
            ))
            .await;

        // lifecycle_repo 不注册任何定义
        let lifecycle_repo = MockLifecycleDefRepo::default();
        let workflow_repo = MockWorkflowDefRepo::default();

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                project_agent: &project_agent_repo,
                activity_lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                project_agent_id,
            },
        )
        .await;

        assert!(ctx.is_none());
    }

    #[tokio::test]
    async fn project_no_agent_returns_none() {
        let project_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();

        let project_agent_repo = MockProjectAgentRepo::default();
        let lifecycle_repo = MockLifecycleDefRepo::default();
        let workflow_repo = MockWorkflowDefRepo::default();

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                project_agent: &project_agent_repo,
                activity_lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                project_agent_id,
            },
        )
        .await;

        assert!(ctx.is_none());
    }

    #[tokio::test]
    async fn project_lifecycle_with_unknown_entry_step_returns_none() {
        let project_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();

        let project_agent_repo = MockProjectAgentRepo::default();
        project_agent_repo
            .insert(make_project_agent(
                project_id,
                project_agent_id,
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
                project_agent: &project_agent_repo,
                activity_lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                project_agent_id,
            },
        )
        .await;

        assert!(ctx.is_none());
    }

    /// 端到端：helper → CapabilityResolver。验证 Project session 绑定
    /// `builtin_workflow_admin` 后，session 创建时即获得 `workflow_management` 能力
    /// 与对应的 Workflow 平台 MCP 注入。这是 PR1 的核心回归点。
    #[tokio::test]
    async fn session_creation_with_default_lifecycle_grants_workflow_management() {
        use crate::capability::{CapabilityResolver, CapabilityResolverInput};
        use crate::platform_config::PlatformConfig;

        let project_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();

        let project_agent_repo = MockProjectAgentRepo::default();
        project_agent_repo
            .insert(make_project_agent(
                project_id,
                project_agent_id,
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
                project_agent: &project_agent_repo,
                activity_lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Project {
                project_id,
                project_agent_id,
            },
        )
        .await;

        let wf_tool = wf_ctx.expect("should resolve to ToolContribution");
        assert!(wf_tool.has_active_workflow);
        assert_eq!(
            wf_tool.directives,
            admin_plan_directives(),
            "bootstrap workflow 上下文必须携带 Plan 阶段工具级 remove，不能只授予 capability"
        );

        // Step 2: 将解析结果直接喂给 CapabilityResolver（无需中间类型转换）
        let platform = PlatformConfig {
            mcp_base_url: Some("http://localhost:3001".to_string()),
        };
        let contributions = vec![crate::capability::ContextContributions {
            source: crate::capability::ContextContributionSource::Workflow,
            tool: Some(wf_tool),
            companion: None,
        }];
        let output = CapabilityResolver::resolve(
            &CapabilityResolverInput {
                owner_ctx: agentdash_spi::CapabilityScopeCtx::Project { project_id },
                contributions,
                mcp_candidates: Default::default(),
                capability_context: None,
            },
            &platform,
        );

        // 断言 capabilities 包含 workflow_management
        assert!(
            output
                .tool
                .capabilities
                .iter()
                .any(|cap| cap.key() == "workflow_management"),
            "session 绑定 builtin_workflow_admin 后应获得 workflow_management 能力"
        );

        // 断言 CapabilityState 包含 Workflow scope MCP 注入
        assert!(
            output.tool.mcp_servers.iter().any(|server| matches!(
                &server.transport,
                agentdash_spi::McpTransportConfig::Http { url, .. }
                    if url.contains("/mcp/workflow/")
            )),
            "应注入 WorkflowMcpServer"
        );

        assert!(
            !output.is_capability_tool_enabled("workflow_management", "upsert_workflow_tool", None),
            "Plan 初始化阶段不得暴露 upsert_workflow_tool schema"
        );
        assert!(
            !output.is_capability_tool_enabled(
                "workflow_management",
                "upsert_lifecycle_tool",
                None
            ),
            "Plan 初始化阶段不得暴露 upsert_lifecycle_tool schema"
        );
    }

    // ── Story / Routine / Task 辅助测试 ──────────────────────────────

    fn make_story_project_agent(
        project_id: Uuid,
        project_agent_id: Uuid,
        lifecycle_key: Option<&str>,
        is_default_for_story: bool,
    ) -> ProjectAgent {
        let mut agent = make_project_agent(project_id, project_agent_id, lifecycle_key);
        agent.is_default_for_story = is_default_for_story;
        agent
    }

    #[tokio::test]
    async fn story_with_default_agent_bound_to_admin_lifecycle_grants_capability() {
        let project_id = Uuid::new_v4();
        let story_default_agent = Uuid::new_v4();
        let non_story_agent = Uuid::new_v4();

        let project_agent_repo = MockProjectAgentRepo::default();
        // 非 story-default agent（应被忽略）
        project_agent_repo
            .insert(make_story_project_agent(
                project_id,
                non_story_agent,
                Some("builtin_workflow_admin"),
                false,
            ))
            .await;
        // story-default agent，绑定 admin lifecycle
        project_agent_repo
            .insert(make_story_project_agent(
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
                project_agent: &project_agent_repo,
                activity_lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Story { project_id },
        )
        .await;

        let ctx = ctx.expect("should resolve to ToolContribution");
        assert!(ctx.has_active_workflow);
        assert_eq!(ctx.directives, admin_plan_directives());
    }

    #[tokio::test]
    async fn story_without_default_agent_returns_none() {
        let project_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();

        let project_agent_repo = MockProjectAgentRepo::default();
        // 有 ProjectAgent 但 is_default_for_story=false
        project_agent_repo
            .insert(make_story_project_agent(
                project_id,
                project_agent_id,
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
                project_agent: &project_agent_repo,
                activity_lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Story { project_id },
        )
        .await;

        assert!(ctx.is_none());
    }

    #[tokio::test]
    async fn routine_with_admin_lifecycle_returns_workflow_management_capability() {
        let project_id = Uuid::new_v4();
        let project_agent_id = Uuid::new_v4();

        let project_agent_repo = MockProjectAgentRepo::default();
        project_agent_repo
            .insert(make_project_agent(
                project_id,
                project_agent_id,
                Some("builtin_workflow_admin"),
            ))
            .await;

        let lifecycle_repo = MockLifecycleDefRepo::default();
        lifecycle_repo.insert(admin_lifecycle(project_id)).await;

        let workflow_repo = MockWorkflowDefRepo::default();
        workflow_repo.insert(admin_entry_workflow(project_id)).await;

        let ctx = resolve_session_workflow_context(
            SessionWorkflowRepos {
                project_agent: &project_agent_repo,
                activity_lifecycle_def: &lifecycle_repo,
                workflow_def: &workflow_repo,
            },
            SessionWorkflowOwner::Routine {
                project_id,
                project_agent_id,
            },
        )
        .await;

        let ctx = ctx.expect("should resolve to ToolContribution");
        assert!(ctx.has_active_workflow);
        assert_eq!(ctx.directives, admin_plan_directives());
    }

    #[test]
    fn tool_directives_from_active_workflow_preserves_directives() {
        let contract = AgentProcedureContract {
            capability_config: agentdash_domain::workflow::CapabilityConfig {
                tool_directives: vec![
                    ToolCapabilityDirective::add_simple("workflow_management"),
                    ToolCapabilityDirective::add_simple("file_read"),
                    ToolCapabilityDirective::remove_simple("shell_execute"),
                    ToolCapabilityDirective::add_simple("mcp:code_analyzer"),
                ],
                ..Default::default()
            },
            ..AgentProcedureContract::default()
        };
        let workflow = AgentProcedure::new(
            Uuid::new_v4(),
            "sample",
            "Sample",
            "",
            DefinitionSource::UserAuthored,
            contract,
        )
        .expect("workflow");

        let directives = tool_directives_from_active_workflow(&workflow);
        assert_eq!(directives.len(), 4);
        // 指令序列必须保持与 contract 完全一致（顺序 + 内容）—— 不再做 Add 包装，
        // workflow 声明的 Remove 指令能够传递到 resolver。
        assert!(directives.iter().any(|d| matches!(
            d,
            ToolCapabilityDirective::Remove(path) if path.capability == "shell_execute"
        )));
        assert!(directives.iter().any(|d| matches!(
            d,
            ToolCapabilityDirective::Add(path) if path.capability == "workflow_management"
        )));
    }
}
