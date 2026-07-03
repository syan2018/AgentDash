//! Workflow 管理 MCP Server — Project 级工作流创建/编辑工具集
//!
//! 面向拥有 workflow_management 能力的 Agent，在 Project 粒度上提供
//! Workflow / Lifecycle 定义的完整 CRUD 能力。
//!
//! 每个 WorkflowMcpServer 实例绑定到一个具体 Project，
//! 定义直接归属 Project（project_id 字段），无需额外的 Assignment 间接层。

use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::workflow::{
    ActivityDefinition, ActivityExecutorSpec, ActivityTransition, ActivityTransitionKind,
    AgentProcedure, AgentProcedureContract, ArtifactBinding, DefinitionSource, InputPortDefinition,
    OutputPortDefinition, ValidationSeverity, WorkflowGraph, WorkflowGraphDraft,
    WorkflowHookRuleSpec, WorkflowHookTrigger,
};
use agentdash_spi::platform::auth::AuthIdentity;

use crate::authz::{McpProjectPermission, require_project_permission};
use crate::error::McpError;
use crate::services::McpServices;

// ─── 工具参数定义 ─────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetWorkflowParams {
    #[schemars(description = "Workflow 唯一标识 key")]
    pub procedure_key: String,
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
    #[schemars(description = "行为契约")]
    pub contract: AgentProcedureContractInput,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AgentProcedureContractInput {
    #[schemars(description = "上下文注入配置（guidance、context_bindings）")]
    pub injection: Option<Value>,
    #[schemars(description = "Hook 规则列表")]
    pub hook_rules: Option<Vec<HookRuleInput>>,
    #[schemars(description = "输出端口定义 — 同时作为完成门禁（output 全部交付才可推进）")]
    pub output_ports: Option<Vec<OutputPortInput>>,
    #[schemars(description = "输入端口定义 — 声明本 workflow 所需的外部数据")]
    pub input_ports: Option<Vec<InputPortInput>>,
    #[schemars(
        description = "顶层能力配置：tool_directives 声明工具能力，mount_directives 声明 VFS/mount 资源能力。"
    )]
    pub capability_config: Option<Value>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HookRuleInput {
    #[schemars(description = "规则唯一 key")]
    pub key: String,
    #[schemars(
        description = "触发时机: before_turn / after_turn / before_tool / session_terminal / before_stop"
    )]
    pub trigger: String,
    #[schemars(description = "规则描述")]
    pub description: String,
    #[schemars(description = "预设名称（如 stop_gate_lifecycle_advance），与 script 二选一")]
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
    #[schemars(description = "入口 Activity key")]
    pub entry_activity_key: String,
    #[schemars(description = "Activity 定义列表")]
    pub activities: Vec<ActivityInput>,
    #[schemars(description = "Activity 转换列表（控制流与 artifact binding）")]
    pub transitions: Option<Vec<TransitionInput>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ActivityInput {
    #[schemars(description = "Activity 唯一 key")]
    pub key: String,
    #[schemars(description = "Activity 描述")]
    pub description: Option<String>,
    #[schemars(
        description = "Activity executor；agent executor 必须引用当前 Project 内已存在的 procedure_key"
    )]
    pub executor: Value,
    #[schemars(description = "输入端口列表")]
    pub input_ports: Option<Vec<InputPortInput>>,
    #[schemars(description = "输出端口列表")]
    pub output_ports: Option<Vec<OutputPortInput>>,
    #[schemars(description = "完成策略，默认 executor_terminal")]
    pub completion_policy: Option<Value>,
    #[schemars(description = "重试/产物别名策略，默认单次最新产物")]
    pub iteration_policy: Option<Value>,
    #[schemars(description = "Join 策略，默认 all")]
    pub join_policy: Option<Value>,
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
pub struct TransitionInput {
    #[schemars(description = "源 Activity key")]
    pub from: String,
    #[schemars(description = "目标 Activity key")]
    pub to: String,
    #[schemars(description = "转换条件，默认 always")]
    pub condition: Option<Value>,
    #[schemars(description = "artifact 端口绑定")]
    pub artifact_bindings: Option<Value>,
    #[schemars(description = "最大遍历次数")]
    pub max_traversals: Option<u32>,
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
    identity: AuthIdentity,
}

impl WorkflowMcpServer {
    pub fn new(services: Arc<McpServices>, project_id: Uuid, identity: AuthIdentity) -> Self {
        Self {
            services,
            project_id,
            identity,
        }
    }

    async fn require_project(
        &self,
        permission: McpProjectPermission,
    ) -> Result<agentdash_domain::project::Project, McpError> {
        require_project_permission(&self.services, &self.identity, self.project_id, permission)
            .await
    }

    /// Upsert workflow：按 key 查重，存在则更新版本，不存在则创建。
    async fn upsert_workflow(
        &self,
        definition: AgentProcedure,
    ) -> Result<AgentProcedure, McpError> {
        let repo = self.services.agent_procedure_repo.as_ref();

        if let Some(existing) = repo
            .get_by_project_and_key(self.project_id, &definition.key)
            .await
            .map_err(McpError::from)?
        {
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

    /// Upsert activity lifecycle：先做完整校验（域层 + workflow 引用），再持久化。
    async fn upsert_lifecycle_definition(
        &self,
        lifecycle: WorkflowGraph,
    ) -> Result<WorkflowGraph, McpError> {
        // 域层结构校验
        let mut issues = lifecycle.validate_full();

        // workflow 引用校验
        let repo = self.services.agent_procedure_repo.as_ref();
        for (idx, activity) in lifecycle.activities.iter().enumerate() {
            let ActivityExecutorSpec::Agent(agent) = &activity.executor else {
                continue;
            };
            match repo
                .get_by_project_and_key(lifecycle.project_id, &agent.procedure_key)
                .await
            {
                Ok(Some(_wf)) => {}
                Ok(None) => {
                    issues.push(agentdash_domain::workflow::ValidationIssue::error(
                        "workflow_not_found",
                        format!(
                            "activity `{}` 引用的 workflow `{}` 不存在，请先通过 upsert_workflow 创建",
                            activity.key, agent.procedure_key
                        ),
                        format!("activities[{idx}].executor.procedure_key"),
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

        let lc_repo = self.services.workflow_graph_repo.as_ref();
        if let Some(existing) = lc_repo
            .get_by_project_and_key(lifecycle.project_id, &lifecycle.key)
            .await
            .map_err(McpError::from)?
        {
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
        "companion_result" => Ok(WorkflowHookTrigger::CompanionResult),
        "before_compact" => Ok(WorkflowHookTrigger::BeforeCompact),
        other => Err(McpError::invalid_param(
            "trigger",
            format!(
                "不支持的触发时机: {other}，\
                 可选值: user_prompt_submit / before_tool / after_tool / after_turn / \
                 before_stop / session_terminal / before_subagent_dispatch / \
                 after_subagent_dispatch / companion_result / before_compact"
            ),
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

fn parse_domain_input<T: DeserializeOwned>(
    field: &'static str,
    value: &Value,
) -> Result<T, McpError> {
    serde_json::from_value(value.clone())
        .map_err(|error| McpError::invalid_param(field, format!("参数结构无效: {error}")))
}

fn build_contract(input: &AgentProcedureContractInput) -> Result<AgentProcedureContract, McpError> {
    Ok(AgentProcedureContract {
        injection: match &input.injection {
            Some(value) => parse_domain_input("injection", value)?,
            None => Default::default(),
        },
        hook_rules: match &input.hook_rules {
            Some(rules) => build_hook_rules(rules)?,
            None => Vec::new(),
        },
        output_ports: build_output_ports(input.output_ports.as_deref().unwrap_or_default()),
        input_ports: build_input_ports(input.input_ports.as_deref().unwrap_or_default()),
        capability_config: match &input.capability_config {
            Some(value) => parse_domain_input("capability_config", value)?,
            None => Default::default(),
        },
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
            standalone_fulfillment: Default::default(),
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

fn build_activities(inputs: &[ActivityInput]) -> Result<Vec<ActivityDefinition>, McpError> {
    inputs
        .iter()
        .map(|activity| {
            Ok(ActivityDefinition {
                key: activity.key.clone(),
                description: activity.description.clone().unwrap_or_default(),
                executor: parse_domain_input("activities.executor", &activity.executor)?,
                input_ports: build_input_ports(activity.input_ports.as_deref().unwrap_or_default()),
                output_ports: build_output_ports(
                    activity.output_ports.as_deref().unwrap_or_default(),
                ),
                completion_policy: match &activity.completion_policy {
                    Some(value) => parse_domain_input("activities.completion_policy", value)?,
                    None => Default::default(),
                },
                iteration_policy: match &activity.iteration_policy {
                    Some(value) => parse_domain_input("activities.iteration_policy", value)?,
                    None => Default::default(),
                },
                join_policy: match &activity.join_policy {
                    Some(value) => parse_domain_input("activities.join_policy", value)?,
                    None => Default::default(),
                },
            })
        })
        .collect()
}

fn build_transitions(inputs: &[TransitionInput]) -> Result<Vec<ActivityTransition>, McpError> {
    inputs
        .iter()
        .map(|transition| {
            let artifact_bindings: Vec<ArtifactBinding> = match &transition.artifact_bindings {
                Some(value) => parse_domain_input("transitions.artifact_bindings", value)?,
                None => Vec::new(),
            };
            Ok(ActivityTransition {
                from: transition.from.clone(),
                to: transition.to.clone(),
                kind: if artifact_bindings.is_empty() {
                    ActivityTransitionKind::Flow
                } else {
                    ActivityTransitionKind::Artifact
                },
                condition: match &transition.condition {
                    Some(value) => parse_domain_input("transitions.condition", value)?,
                    None => Default::default(),
                },
                artifact_bindings,
                max_traversals: transition.max_traversals,
            })
        })
        .collect()
}

// ─── 工具实现 ──────────────────────────────────────────────────

#[tool_router]
impl WorkflowMcpServer {
    #[tool(description = "列出当前项目下所有 Workflow 和 Lifecycle 定义")]
    async fn list_workflows(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Use).await?;
        let workflows = self
            .services
            .agent_procedure_repo
            .list_by_project(self.project_id)
            .await
            .map_err(|e| McpError::Internal(format!("加载 workflow 列表失败: {e}")))?;

        let lifecycles = self
            .services
            .workflow_graph_repo
            .list_by_project(self.project_id)
            .await
            .map_err(|e| McpError::Internal(format!("加载 lifecycle 列表失败: {e}")))?;

        let result = serde_json::json!({
            "project_id": self.project_id.to_string(),
            "workflows": workflows.iter().map(|w| serde_json::json!({
                "key": w.key,
                "name": w.name,
                "description": w.description,
                "source": w.source,
            })).collect::<Vec<_>>(),
            "lifecycles": lifecycles.iter().map(|l| serde_json::json!({
                "key": l.key,
                "name": l.name,
                "description": l.description,
                "source": l.source,
                "entry_activity_key": l.entry_activity_key,
                "activity_count": l.activities.len(),
                "transition_count": l.transitions.len(),
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
        self.require_project(McpProjectPermission::Use).await?;
        let workflow = self
            .services
            .agent_procedure_repo
            .get_by_project_and_key(self.project_id, &params.procedure_key)
            .await
            .map_err(|e| McpError::Internal(format!("加载 workflow 失败: {e}")))?
            .ok_or_else(|| McpError::not_found("AgentProcedure", &params.procedure_key))?;

        let result = serde_json::to_value(&workflow)
            .map_err(|e| McpError::Internal(format!("序列化失败: {e}")))?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "获取单个 Activity Lifecycle 定义的完整详情（含 activities、transitions）"
    )]
    async fn get_lifecycle(
        &self,
        Parameters(params): Parameters<GetLifecycleParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Use).await?;
        let lifecycle = self
            .services
            .workflow_graph_repo
            .get_by_project_and_key(self.project_id, &params.lifecycle_key)
            .await
            .map_err(|e| McpError::Internal(format!("加载 lifecycle 失败: {e}")))?
            .ok_or_else(|| McpError::not_found("WorkflowGraph", &params.lifecycle_key))?;

        let result = serde_json::to_value(&lifecycle)
            .map_err(|e| McpError::Internal(format!("序列化失败: {e}")))?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "创建或更新 Workflow 定义（单步行为契约）。保存时自动校验，失败会返回详细错误信息。"
    )]
    async fn upsert_workflow_tool(
        &self,
        Parameters(params): Parameters<UpsertWorkflowParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Configure)
            .await?;
        let contract = build_contract(&params.contract)?;

        let definition = AgentProcedure::new(
            self.project_id,
            params.key,
            params.name,
            params.description,
            DefinitionSource::UserAuthored,
            contract,
        )
        .map_err(|e| McpError::invalid_param("key", e))?;

        let saved = self.upsert_workflow(definition).await?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Workflow `{}` 已保存（id={}, version={}）",
            saved.key, saved.id, saved.version
        ))]))
    }

    #[tool(
        description = "创建或更新 Activity Lifecycle 定义（多 Activity DAG 编排）并自动绑定到当前 Project。\n\n保存时自动校验 DAG 拓扑、port 契约和 workflow 引用。agent executor 引用的 procedure_key 必须已存在。"
    )]
    async fn upsert_lifecycle_tool(
        &self,
        Parameters(params): Parameters<UpsertLifecycleParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.require_project(McpProjectPermission::Configure)
            .await?;
        let activities = build_activities(&params.activities)?;
        let transitions = build_transitions(params.transitions.as_deref().unwrap_or_default())?;

        let definition = WorkflowGraph::new(WorkflowGraphDraft {
            project_id: self.project_id,
            key: params.key,
            name: params.name,
            description: params.description,
            source: DefinitionSource::UserAuthored,
            entry_activity_key: params.entry_activity_key,
            activities,
            transitions,
        })
        .map_err(|e| McpError::invalid_param("lifecycle", e))?;

        let saved = self.upsert_lifecycle_definition(definition).await?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Activity Lifecycle `{}` 已保存（project={}, id={}, version={}, activities={}, transitions={}）",
            saved.key,
            self.project_id,
            saved.id,
            saved.version,
            saved.activities.len(),
            saved.transitions.len(),
        ))]))
    }
}

// ─── ServerHandler 实现 ──────────────────────────────────────

#[tool_handler(router = Self::tool_router())]
impl ServerHandler for WorkflowMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(format!(
                r#"Workflow 管理工具（Project: {project_id}）。

## 领域模型

- **AgentProcedure**：单步行为契约，定义一个 Agent session 的注入规则、I/O ports 和 hook 脚本。output ports 同时作为完成门禁。
- **WorkflowGraph**：多 Activity DAG 编排，每个 Activity 通过 executor 描述执行主体；agent executor 引用 Workflow，function/human executor 可直接作为编排节点。

## 推荐流程

1. `list_workflows` — 查看现有定义
2. `upsert_workflow_tool` — 先创建 agent activity 需要引用的 Workflow 定义
3. `upsert_lifecycle_tool` — 再创建 Activity Lifecycle，声明 activities 与 transitions
4. 所有定义自动归属当前 Project

## 注意事项

- agent executor 的 procedure_key 必须先创建再引用，lifecycle 中引用不存在的 workflow 会被拒绝
- hook_rules 支持 preset（预设名引用）和 script（Rhai 脚本）两种模式
- 所有写操作都会即时校验，失败会返回详细错误信息供修正"#,
                project_id = self.project_id,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_spi::context::tool_schema_sanitizer::sanitize_tool_schema;
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

    fn assert_schema_omits_keyword(value: &Value, keyword: &str) {
        match value {
            Value::Object(object) => {
                assert!(
                    !object.contains_key(keyword),
                    "Responses tool schema 不应包含 `{keyword}`: {}",
                    serde_json::to_string_pretty(value).unwrap_or_default()
                );
                for child in object.values() {
                    assert_schema_omits_keyword(child, keyword);
                }
            }
            Value::Array(items) => {
                for item in items {
                    assert_schema_omits_keyword(item, keyword);
                }
            }
            _ => {}
        }
    }

    fn assert_responses_schema_compatible(schema: &Value) {
        for keyword in [
            "$defs",
            "$ref",
            "definitions",
            "oneOf",
            "allOf",
            "default",
            "format",
            "$schema",
        ] {
            assert_schema_omits_keyword(schema, keyword);
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
        assert_responses_schema_compatible(&schema);
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
        assert_responses_schema_compatible(&schema);
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
