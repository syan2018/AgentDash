//! Workflow 管理 MCP Server — Project 级工作流创建/编辑工具集
//!
//! 面向拥有 workflow_management 能力的 Agent，在 Project 粒度上提供
//! Workflow / Lifecycle 定义的完整 CRUD 能力。
//!
//! 每个 WorkflowMcpServer 实例绑定到一个具体 Project，
//! 定义直接归属 Project（project_id 字段），无需额外的 Assignment 间接层。

use std::sync::Arc;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::*;
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use uuid::Uuid;

use agentdash_domain::workflow::{
    InputPortDefinition, LifecycleDefinition, LifecycleEdge, LifecycleEdgeKind,
    LifecycleNodeType, LifecycleStepDefinition, OutputPortDefinition, ValidationSeverity,
    WorkflowBindingKind, WorkflowBindingRole, WorkflowCompletionSpec, WorkflowConstraintSpec,
    WorkflowContract, WorkflowDefinition, WorkflowDefinitionSource, WorkflowHookRuleSpec,
    WorkflowHookTrigger, WorkflowInjectionSpec,
};

use crate::error::McpError;
use crate::services::McpServices;

// ─── 工具参数定义 ─────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetWorkflowParams {
    #[schemars(description = "Workflow 唯一标识 key")]
    pub workflow_key: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetLifecycleParams {
    #[schemars(description = "Lifecycle 唯一标识 key")]
    pub lifecycle_key: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpsertWorkflowParams {
    #[schemars(description = "唯一标识 key（snake_case，如 my_project_implement）")]
    pub key: String,
    #[schemars(description = "显示名称")]
    pub name: String,
    #[schemars(description = "描述")]
    pub description: String,
    #[schemars(description = "绑定类型: project / story / task")]
    pub binding_kind: String,
    #[schemars(description = "推荐绑定角色列表（可选），如 [\"task\"]")]
    pub recommended_binding_roles: Option<Vec<String>>,
    #[schemars(description = "行为契约")]
    pub contract: WorkflowContractInput,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkflowContractInput {
    #[schemars(description = "上下文注入配置（instructions、context_bindings）")]
    pub injection: Option<WorkflowInjectionSpec>,
    #[schemars(description = "Hook 规则列表")]
    pub hook_rules: Option<Vec<HookRuleInput>>,
    #[schemars(description = "行为约束列表")]
    pub constraints: Option<Vec<WorkflowConstraintSpec>>,
    #[schemars(description = "完成条件定义")]
    pub completion: Option<WorkflowCompletionSpec>,
    #[schemars(description = "推荐的输出端口（WorkflowContract 级，非 step 级）")]
    pub recommended_output_ports: Option<Vec<OutputPortInput>>,
    #[schemars(description = "推荐的输入端口（WorkflowContract 级，非 step 级）")]
    pub recommended_input_ports: Option<Vec<InputPortInput>>,
    #[schemars(description = "Workflow 基线能力 key 集合。平台 well-known key（如 file_system、workflow_management），或自定义 MCP 引用 mcp:<preset_name>。")]
    pub capabilities: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HookRuleInput {
    #[schemars(description = "规则唯一 key")]
    pub key: String,
    #[schemars(description = "触发时机: before_turn / after_turn / before_tool / session_terminal / before_stop")]
    pub trigger: String,
    #[schemars(description = "规则描述")]
    pub description: String,
    #[schemars(description = "预设名称（如 task_session_terminal），与 script 二选一")]
    pub preset: Option<String>,
    #[schemars(description = "预设参数")]
    pub params: Option<serde_json::Value>,
    #[schemars(description = "Rhai 脚本内容，与 preset 二选一")]
    pub script: Option<String>,
    #[schemars(description = "是否启用，默认 true")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpsertLifecycleParams {
    #[schemars(description = "唯一标识 key（snake_case）")]
    pub key: String,
    #[schemars(description = "显示名称")]
    pub name: String,
    #[schemars(description = "描述")]
    pub description: String,
    #[schemars(description = "绑定类型: project / story / task")]
    pub binding_kind: String,
    #[schemars(description = "推荐绑定角色列表（可选）")]
    pub recommended_binding_roles: Option<Vec<String>>,
    #[schemars(description = "入口步骤 key")]
    pub entry_step_key: String,
    #[schemars(description = "步骤定义列表")]
    pub steps: Vec<StepInput>,
    #[schemars(description = "边定义列表（定义步骤间数据流）")]
    pub edges: Option<Vec<EdgeInput>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StepInput {
    #[schemars(description = "步骤唯一 key")]
    pub key: String,
    #[schemars(description = "步骤描述")]
    pub description: Option<String>,
    #[schemars(description = "关联的 workflow key（必须已存在）")]
    pub workflow_key: Option<String>,
    #[schemars(description = "节点类型: agent_node（默认）/ phase_node")]
    pub node_type: Option<String>,
    #[schemars(description = "输入端口列表")]
    pub input_ports: Option<Vec<InputPortInput>>,
    #[schemars(description = "输出端口列表")]
    pub output_ports: Option<Vec<OutputPortInput>>,
    /// 旧版字段：step 级 capability 指令。新模型已将 capability 归属迁移到 WorkflowContract,
    /// 此字段若有值仅会落 warn 日志，不再实际参与解析；兼容已有 upsert 请求避免强制破坏契约。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "已废弃：step 级 capabilities 已迁移到 workflow.contract.capabilities，此字段收到后会被忽略。")]
    pub capabilities: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InputPortInput {
    #[schemars(description = "端口 key")]
    pub key: String,
    #[schemars(description = "端口描述")]
    pub description: Option<String>,
    #[schemars(description = "是否必须")]
    pub required: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OutputPortInput {
    #[schemars(description = "端口 key")]
    pub key: String,
    #[schemars(description = "端口描述")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKindInput {
    Flow,
    Artifact,
}

fn default_edge_kind_input() -> EdgeKindInput {
    // 历史调用不带 kind 时按 artifact 兼容
    EdgeKindInput::Artifact
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EdgeInput {
    #[schemars(description = "边类别：flow（控制流，无 port）/ artifact（数据流，需声明 port）；默认 artifact 以兼容历史调用")]
    #[serde(default = "default_edge_kind_input")]
    pub kind: EdgeKindInput,
    #[schemars(description = "源节点 key")]
    pub from_node: String,
    #[schemars(description = "源输出端口 key（仅 artifact edge 需要）")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_port: Option<String>,
    #[schemars(description = "目标节点 key")]
    pub to_node: String,
    #[schemars(description = "目标输入端口 key（仅 artifact edge 需要）")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
}

// ─── Server 定义 ──────────────────────────────────────────────

/// Workflow 管理 MCP Server
///
/// 绑定到具体 Project 实例，暴露 Workflow/Lifecycle 的 CRUD 工具。
/// 工具操作范围限定在所绑定的 Project 内。
#[derive(Clone)]
pub struct WorkflowMcpServer {
    services: Arc<McpServices>,
    project_id: Uuid,
    tool_router: ToolRouter<Self>,
}

impl WorkflowMcpServer {
    pub fn new(services: Arc<McpServices>, project_id: Uuid) -> Self {
        Self {
            services,
            project_id,
            tool_router: Self::tool_router(),
        }
    }

    /// Upsert workflow：按 key 查重，存在则更新版本，不存在则创建。
    async fn upsert_workflow(
        &self,
        definition: WorkflowDefinition,
    ) -> Result<WorkflowDefinition, McpError> {
        let repo = self.services.workflow_definition_repo.as_ref();

        if let Some(existing) = repo.get_by_key(&definition.key).await.map_err(McpError::from)? {
            if existing.binding_kind != definition.binding_kind {
                return Err(McpError::invalid_param(
                    "binding_kind",
                    format!(
                        "workflow `{}` 已绑定 binding_kind={:?}，不能改为 {:?}",
                        definition.key, existing.binding_kind, definition.binding_kind
                    ),
                ));
            }
            let mut updated = definition;
            updated.id = existing.id;
            updated.version = existing.version + 1;
            updated.created_at = existing.created_at;
            updated.updated_at = chrono::Utc::now();
            repo.update(&updated).await.map_err(McpError::from)?;
            return Ok(updated);
        }

        repo.create(&definition).await.map_err(McpError::from)?;
        Ok(definition)
    }

    /// Upsert lifecycle：先做完整校验（域层 + workflow 引用），再持久化。
    async fn upsert_lifecycle_definition(
        &self,
        lifecycle: LifecycleDefinition,
    ) -> Result<LifecycleDefinition, McpError> {
        // 域层结构校验
        let mut issues = lifecycle.validate_full();

        // workflow 引用校验
        let repo = self.services.workflow_definition_repo.as_ref();
        for (idx, step) in lifecycle.steps.iter().enumerate() {
            let Some(wk) = step.effective_workflow_key() else {
                continue;
            };
            match repo.get_by_key(wk).await {
                Ok(Some(wf)) => {
                    if wf.binding_kind != lifecycle.binding_kind {
                        issues.push(agentdash_domain::workflow::ValidationIssue::error(
                            "binding_kind_mismatch",
                            format!(
                                "step `{}` 引用的 workflow `{wk}` binding_kind={:?}，与 lifecycle {:?} 不一致",
                                step.key, wf.binding_kind, lifecycle.binding_kind
                            ),
                            format!("steps[{idx}].workflow_key"),
                        ));
                    }
                }
                Ok(None) => {
                    issues.push(agentdash_domain::workflow::ValidationIssue::error(
                        "workflow_not_found",
                        format!(
                            "step `{}` 引用的 workflow `{wk}` 不存在，请先通过 upsert_workflow 创建",
                            step.key
                        ),
                        format!("steps[{idx}].workflow_key"),
                    ));
                }
                Err(e) => {
                    return Err(McpError::Internal(format!("校验 workflow 引用失败: {e}")));
                }
            }
        }

        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(McpError::invalid_param(
                "lifecycle",
                errors
                    .iter()
                    .map(|i| format!("[{}] {}", i.field_path, i.message))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }

        let lc_repo = self.services.lifecycle_definition_repo.as_ref();
        if let Some(existing) = lc_repo.get_by_key(&lifecycle.key).await.map_err(McpError::from)? {
            if existing.binding_kind != lifecycle.binding_kind {
                return Err(McpError::invalid_param(
                    "binding_kind",
                    format!(
                        "lifecycle `{}` 已绑定 binding_kind={:?}，不能改为 {:?}",
                        lifecycle.key, existing.binding_kind, lifecycle.binding_kind
                    ),
                ));
            }
            let mut updated = lifecycle;
            updated.id = existing.id;
            updated.version = existing.version + 1;
            updated.created_at = existing.created_at;
            updated.updated_at = chrono::Utc::now();
            lc_repo.update(&updated).await.map_err(McpError::from)?;
            return Ok(updated);
        }

        lc_repo.create(&lifecycle).await.map_err(McpError::from)?;
        Ok(lifecycle)
    }

}

// ─── 辅助转换 ─────────────────────────────────────────────────

fn parse_binding_kind(raw: &str) -> Result<WorkflowBindingKind, McpError> {
    match raw {
        "project" => Ok(WorkflowBindingKind::Project),
        "story" => Ok(WorkflowBindingKind::Story),
        "task" => Ok(WorkflowBindingKind::Task),
        other => Err(McpError::invalid_param(
            "binding_kind",
            format!("不支持的绑定类型: {other}，可选值: project / story / task"),
        )),
    }
}

fn parse_binding_roles(raw: &[String]) -> Result<Vec<WorkflowBindingRole>, McpError> {
    raw.iter()
        .map(|s| match s.as_str() {
            "project" => Ok(WorkflowBindingRole::Project),
            "story" => Ok(WorkflowBindingRole::Story),
            "task" => Ok(WorkflowBindingRole::Task),
            other => Err(McpError::invalid_param(
                "recommended_binding_roles",
                format!("不支持的角色: {other}"),
            )),
        })
        .collect()
}

fn parse_hook_trigger(raw: &str) -> Result<WorkflowHookTrigger, McpError> {
    match raw {
        "user_prompt_submit" => Ok(WorkflowHookTrigger::UserPromptSubmit),
        "before_tool" => Ok(WorkflowHookTrigger::BeforeTool),
        "after_tool" => Ok(WorkflowHookTrigger::AfterTool),
        "after_turn" => Ok(WorkflowHookTrigger::AfterTurn),
        "before_stop" => Ok(WorkflowHookTrigger::BeforeStop),
        "session_terminal" => Ok(WorkflowHookTrigger::SessionTerminal),
        "before_subagent_dispatch" => Ok(WorkflowHookTrigger::BeforeSubagentDispatch),
        "after_subagent_dispatch" => Ok(WorkflowHookTrigger::AfterSubagentDispatch),
        "subagent_result" => Ok(WorkflowHookTrigger::SubagentResult),
        "before_compact" => Ok(WorkflowHookTrigger::BeforeCompact),
        other => Err(McpError::invalid_param(
            "trigger",
            format!(
                "不支持的触发时机: {other}，\
                 可选值: user_prompt_submit / before_tool / after_tool / after_turn / \
                 before_stop / session_terminal / before_subagent_dispatch / \
                 after_subagent_dispatch / subagent_result / before_compact"
            ),
        )),
    }
}

fn parse_node_type(raw: Option<&str>) -> Result<LifecycleNodeType, McpError> {
    match raw.unwrap_or("agent_node") {
        "agent_node" => Ok(LifecycleNodeType::AgentNode),
        "phase_node" => Ok(LifecycleNodeType::PhaseNode),
        other => Err(McpError::invalid_param(
            "node_type",
            format!("不支持的节点类型: {other}，可选值: agent_node / phase_node"),
        )),
    }
}

fn build_hook_rules(rules: &[HookRuleInput]) -> Result<Vec<WorkflowHookRuleSpec>, McpError> {
    rules
        .iter()
        .map(|rule| {
            Ok(WorkflowHookRuleSpec {
                key: rule.key.clone(),
                trigger: parse_hook_trigger(&rule.trigger)?,
                description: rule.description.clone(),
                preset: rule.preset.clone(),
                params: rule.params.clone(),
                script: rule.script.clone(),
                enabled: rule.enabled.unwrap_or(true),
            })
        })
        .collect()
}

fn build_contract(input: &WorkflowContractInput) -> Result<WorkflowContract, McpError> {
    Ok(WorkflowContract {
        injection: input.injection.clone().unwrap_or_default(),
        hook_rules: match &input.hook_rules {
            Some(rules) => build_hook_rules(rules)?,
            None => Vec::new(),
        },
        constraints: input.constraints.clone().unwrap_or_default(),
        completion: input.completion.clone().unwrap_or_default(),
        recommended_output_ports: build_output_ports(
            input.recommended_output_ports.as_deref().unwrap_or_default(),
        ),
        recommended_input_ports: build_input_ports(
            input.recommended_input_ports.as_deref().unwrap_or_default(),
        ),
        capabilities: input
            .capabilities
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|s| agentdash_domain::workflow::CapabilityEntry::simple(s))
            .collect(),
    })
}

fn build_input_ports(inputs: &[InputPortInput]) -> Vec<InputPortDefinition> {
    inputs
        .iter()
        .map(|p| InputPortDefinition {
            key: p.key.clone(),
            description: p.description.clone().unwrap_or_default(),
            context_strategy: Default::default(),
            context_template: None,
        })
        .collect()
}

fn build_output_ports(inputs: &[OutputPortInput]) -> Vec<OutputPortDefinition> {
    inputs
        .iter()
        .map(|p| OutputPortDefinition {
            key: p.key.clone(),
            description: p.description.clone().unwrap_or_default(),
            gate_strategy: Default::default(),
            gate_params: None,
        })
        .collect()
}

fn build_steps(inputs: &[StepInput]) -> Result<Vec<LifecycleStepDefinition>, McpError> {
    inputs
        .iter()
        .map(|step| {
            if step.capabilities.is_some() {
                tracing::warn!(
                    step_key = %step.key,
                    "upsert_lifecycle: 收到已废弃的 step.capabilities 字段,已忽略。请改用 workflow.contract.capabilities。"
                );
            }
            Ok(LifecycleStepDefinition {
                key: step.key.clone(),
                description: step.description.clone().unwrap_or_default(),
                workflow_key: step.workflow_key.clone(),
                node_type: parse_node_type(step.node_type.as_deref())?,
                input_ports: build_input_ports(
                    step.input_ports.as_deref().unwrap_or_default(),
                ),
                output_ports: build_output_ports(
                    step.output_ports.as_deref().unwrap_or_default(),
                ),
            })
        })
        .collect()
}

fn build_edges(inputs: &[EdgeInput]) -> Vec<LifecycleEdge> {
    inputs
        .iter()
        .map(|e| match e.kind {
            EdgeKindInput::Flow => LifecycleEdge::flow(e.from_node.clone(), e.to_node.clone()),
            EdgeKindInput::Artifact => LifecycleEdge {
                kind: LifecycleEdgeKind::Artifact,
                from_node: e.from_node.clone(),
                to_node: e.to_node.clone(),
                from_port: e.from_port.clone(),
                to_port: e.to_port.clone(),
            },
        })
        .collect()
}

// ─── 工具实现 ──────────────────────────────────────────────────

#[tool_router]
impl WorkflowMcpServer {
    #[tool(description = "列出当前项目下所有 Workflow 和 Lifecycle 定义")]
    async fn list_workflows(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let workflows = self
            .services
            .workflow_definition_repo
            .list_by_project(self.project_id)
            .await
            .map_err(|e| McpError::Internal(format!("加载 workflow 列表失败: {e}")))?;

        let lifecycles = self
            .services
            .lifecycle_definition_repo
            .list_by_project(self.project_id)
            .await
            .map_err(|e| McpError::Internal(format!("加载 lifecycle 列表失败: {e}")))?;

        let result = serde_json::json!({
            "project_id": self.project_id.to_string(),
            "workflows": workflows.iter().map(|w| serde_json::json!({
                "key": w.key,
                "name": w.name,
                "description": w.description,
                "binding_kind": w.binding_kind,
                "source": w.source,
            })).collect::<Vec<_>>(),
            "lifecycles": lifecycles.iter().map(|l| serde_json::json!({
                "key": l.key,
                "name": l.name,
                "description": l.description,
                "binding_kind": l.binding_kind,
                "source": l.source,
                "entry_step_key": l.entry_step_key,
                "step_count": l.steps.len(),
                "edge_count": l.edges.len(),
            })).collect::<Vec<_>>(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "获取单个 Workflow 定义的完整详情（含 contract）")]
    async fn get_workflow(
        &self,
        Parameters(params): Parameters<GetWorkflowParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let workflow = self
            .services
            .workflow_definition_repo
            .get_by_key(&params.workflow_key)
            .await
            .map_err(|e| McpError::Internal(format!("加载 workflow 失败: {e}")))?
            .ok_or_else(|| {
                McpError::not_found("WorkflowDefinition", &params.workflow_key)
            })?;

        let result = serde_json::to_value(&workflow)
            .map_err(|e| McpError::Internal(format!("序列化失败: {e}")))?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "获取单个 Lifecycle 定义的完整详情（含 steps、edges）")]
    async fn get_lifecycle(
        &self,
        Parameters(params): Parameters<GetLifecycleParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let lifecycle = self
            .services
            .lifecycle_definition_repo
            .get_by_key(&params.lifecycle_key)
            .await
            .map_err(|e| McpError::Internal(format!("加载 lifecycle 失败: {e}")))?
            .ok_or_else(|| {
                McpError::not_found("LifecycleDefinition", &params.lifecycle_key)
            })?;

        let result = serde_json::to_value(&lifecycle)
            .map_err(|e| McpError::Internal(format!("序列化失败: {e}")))?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(description = "创建或更新 Workflow 定义（单步行为契约）。保存时自动校验，失败会返回详细错误信息。")]
    async fn upsert_workflow_tool(
        &self,
        Parameters(params): Parameters<UpsertWorkflowParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let binding_kind = parse_binding_kind(&params.binding_kind)?;
        let contract = build_contract(&params.contract)?;

        let mut definition = WorkflowDefinition::new(
            self.project_id,
            params.key,
            params.name,
            params.description,
            binding_kind,
            WorkflowDefinitionSource::UserAuthored,
            contract,
        )
        .map_err(|e| McpError::invalid_param("key", e))?;

        if let Some(roles) = &params.recommended_binding_roles {
            definition.recommended_binding_roles = parse_binding_roles(roles)?;
        }

        let saved = self.upsert_workflow(definition).await?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Workflow `{}` 已保存（id={}, version={}）",
            saved.key, saved.id, saved.version
        ))]))
    }

    #[tool(description = "创建或更新 Lifecycle 定义（多步 DAG 编排）并自动绑定到当前 Project。\n\n保存时自动校验 DAG 拓扑、port 契约和 workflow 引用。step.workflow_key 引用的 Workflow 必须已存在。")]
    async fn upsert_lifecycle_tool(
        &self,
        Parameters(params): Parameters<UpsertLifecycleParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let binding_kind = parse_binding_kind(&params.binding_kind)?;
        let steps = build_steps(&params.steps)?;
        let edges = build_edges(params.edges.as_deref().unwrap_or_default());

        let mut definition = LifecycleDefinition::new(
            self.project_id,
            params.key,
            params.name,
            params.description,
            binding_kind,
            WorkflowDefinitionSource::UserAuthored,
            params.entry_step_key,
            steps,
            edges,
        )
        .map_err(|e| McpError::invalid_param("lifecycle", e))?;

        if let Some(roles) = &params.recommended_binding_roles {
            definition.recommended_binding_roles = parse_binding_roles(roles)?;
        }

        let saved = self.upsert_lifecycle_definition(definition).await?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Lifecycle `{}` 已保存（project={}, id={}, version={}, steps={}, edges={}）",
            saved.key,
            self.project_id,
            saved.id,
            saved.version,
            saved.steps.len(),
            saved.edges.len(),
        ))]))
    }
}

// ─── ServerHandler 实现 ──────────────────────────────────────

#[tool_handler]
impl ServerHandler for WorkflowMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(format!(
                r#"Workflow 管理工具（Project: {project_id}）。

## 领域模型

- **WorkflowDefinition**：单步行为契约，定义一个 Agent session 的注入规则、约束、完成条件和 hook 脚本。
- **LifecycleDefinition**：多步 DAG 编排，由多个 step 组成，每个 step 可绑定一个 Workflow，step 之间通过 port + edge 声明数据依赖。

## 推荐流程

1. `list_workflows` — 查看现有定义
2. `upsert_workflow_tool` — 先创建各步骤的 Workflow 定义
3. `upsert_lifecycle_tool` — 再创建 Lifecycle，引用已有的 workflow_key
4. 所有定义自动归属当前 Project

## 注意事项

- workflow_key 必须先创建再引用，lifecycle 中引用不存在的 workflow 会被拒绝
- binding_kind 一旦设定不可修改（project / story / task）
- hook_rules 支持 preset（预设名引用）和 script（Rhai 脚本）两种模式
- 所有写操作都会即时校验，失败会返回详细错误信息供修正"#,
                project_id = self.project_id,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::schema::sanitize_tool_schema;
    use serde_json::Value;

    fn assert_schema_objects_have_type(value: &Value) {
        let Some(object) = value.as_object() else {
            if let Some(items) = value.as_array() {
                for item in items {
                    assert_schema_objects_have_type(item);
                }
            }
            return;
        };

        if object.contains_key("properties") {
            assert!(
                object.get("type").and_then(Value::as_str) == Some("object"),
                "带 properties 的 schema 必须显式声明 type=object: {}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        }

        for key in [
            "items",
            "additionalProperties",
            "contains",
            "if",
            "then",
            "else",
        ] {
            if let Some(child) = object.get(key) {
                assert_schema_objects_have_type(child);
            }
        }

        for key in [
            "$defs",
            "definitions",
            "dependentSchemas",
            "patternProperties",
        ] {
            if let Some(children) = object.get(key).and_then(Value::as_object) {
                for child in children.values() {
                    assert_schema_objects_have_type(child);
                }
            }
        }

        for key in ["anyOf", "allOf", "oneOf", "prefixItems"] {
            if let Some(children) = object.get(key).and_then(Value::as_array) {
                for child in children {
                    assert_schema_objects_have_type(child);
                }
            }
        }
    }

    #[test]
    fn upsert_workflow_schema_is_openai_compatible() {
        let tool = WorkflowMcpServer::tool_router()
            .list_all()
            .into_iter()
            .find(|t| t.name.as_ref() == "upsert_workflow_tool")
            .expect("upsert_workflow_tool should exist");
        let schema = sanitize_tool_schema(Value::Object((*tool.input_schema).clone()));

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_schema_objects_have_type(&schema);
    }

    #[test]
    fn upsert_lifecycle_schema_is_openai_compatible() {
        let tool = WorkflowMcpServer::tool_router()
            .list_all()
            .into_iter()
            .find(|t| t.name.as_ref() == "upsert_lifecycle_tool")
            .expect("upsert_lifecycle_tool should exist");
        let schema = sanitize_tool_schema(Value::Object((*tool.input_schema).clone()));

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_schema_objects_have_type(&schema);
    }

    #[test]
    fn all_tools_registered() {
        let tools = WorkflowMcpServer::tool_router().list_all();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&"list_workflows"));
        assert!(names.contains(&"get_workflow"));
        assert!(names.contains(&"get_lifecycle"));
        assert!(names.contains(&"upsert_workflow_tool"));
        assert!(names.contains(&"upsert_lifecycle_tool"));
        assert_eq!(names.len(), 5);
    }
}
