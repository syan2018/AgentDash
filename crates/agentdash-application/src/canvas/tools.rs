use std::sync::Arc;

use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};
use agentdash_domain::canvas::{CanvasDataBinding, CanvasRepository};
use agentdash_spi::schema::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::address_space::build_canvas_mount_id;
use crate::address_space::tools::provider::SharedSessionHubHandle;
use crate::canvas::{build_canvas, upsert_canvas_binding};

#[derive(Clone)]
pub struct CreateCanvasTool {
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
}

impl CreateCanvasTool {
    pub fn new(canvas_repo: Arc<dyn CanvasRepository>, project_id: Uuid) -> Self {
        Self {
            canvas_repo,
            project_id,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateCanvasParams {
    pub title: String,
    pub description: Option<String>,
}

#[derive(Clone)]
pub struct InjectCanvasDataTool {
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
}

impl InjectCanvasDataTool {
    pub fn new(canvas_repo: Arc<dyn CanvasRepository>, project_id: Uuid) -> Self {
        Self {
            canvas_repo,
            project_id,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InjectCanvasDataParams {
    pub canvas_id: String,
    pub alias: String,
    pub source_uri: String,
    pub content_type: Option<String>,
}

#[derive(Clone)]
pub struct PresentCanvasTool {
    canvas_repo: Arc<dyn CanvasRepository>,
    session_hub_handle: SharedSessionHubHandle,
    current_session_id: String,
    current_turn_id: String,
    project_id: Uuid,
}

impl PresentCanvasTool {
    pub fn new(
        canvas_repo: Arc<dyn CanvasRepository>,
        session_hub_handle: SharedSessionHubHandle,
        current_session_id: String,
        current_turn_id: String,
        project_id: Uuid,
    ) -> Self {
        Self {
            canvas_repo,
            session_hub_handle,
            current_session_id,
            current_turn_id,
            project_id,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PresentCanvasParams {
    pub canvas_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct CanvasToolResult {
    canvas_id: String,
    mount_id: String,
    entry_file: String,
}

#[async_trait]
impl AgentTool for CreateCanvasTool {
    fn name(&self) -> &str {
        "create_canvas"
    }

    fn description(&self) -> &str {
        "在当前 Project 下创建一个新的 Canvas 资产，并返回对应的 mount 标识"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<CreateCanvasParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: CreateCanvasParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
        let title = params.title.trim();
        if title.is_empty() {
            return Err(AgentToolError::InvalidArguments(
                "Canvas 标题不能为空".to_string(),
            ));
        }

        let canvas = build_canvas(
            self.project_id,
            title.to_string(),
            params.description.unwrap_or_default(),
            Default::default(),
        )
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        self.canvas_repo
            .create(&canvas)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let result = CanvasToolResult {
            canvas_id: canvas.id.to_string(),
            mount_id: build_canvas_mount_id(canvas.id),
            entry_file: canvas.entry_file.clone(),
        };

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已创建 Canvas。\n- canvas_id: {}\n- mount: {}://\n- entry_file: {}",
                result.canvas_id, result.mount_id, result.entry_file
            ))],
            is_error: false,
            details: Some(serde_json::to_value(result).unwrap_or_default()),
        })
    }
}

#[async_trait]
impl AgentTool for InjectCanvasDataTool {
    fn name(&self) -> &str {
        "inject_canvas_data"
    }

    fn description(&self) -> &str {
        "为 Canvas 绑定一个外部数据文件引用，在运行时映射为 bindings/<alias>.json"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<InjectCanvasDataParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: InjectCanvasDataParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
        let canvas_id = parse_canvas_uuid(&params.canvas_id)?;
        let mut canvas = self
            .canvas_repo
            .get_by_id(canvas_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            .ok_or_else(|| AgentToolError::ExecutionFailed(format!("Canvas 不存在: {canvas_id}")))?;
        ensure_canvas_project(canvas.project_id, self.project_id)?;

        let binding = CanvasDataBinding {
            alias: params.alias,
            source_uri: params.source_uri,
            content_type: params
                .content_type
                .unwrap_or_else(|| "application/json".to_string()),
        };
        let alias = binding.alias.clone();
        let source_uri = binding.source_uri.clone();
        upsert_canvas_binding(&mut canvas, binding)
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        self.canvas_repo
            .update(&canvas)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已更新 Canvas 数据绑定。\n- canvas_id: {}\n- alias: {}\n- source_uri: {}",
                canvas.id, alias, source_uri
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "canvas_id": canvas.id,
                "bindings": canvas.bindings,
            })),
        })
    }
}

#[async_trait]
impl AgentTool for PresentCanvasTool {
    fn name(&self) -> &str {
        "present_canvas"
    }

    fn description(&self) -> &str {
        "通过 ACP 系统事件请求前端打开指定 Canvas 面板"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<PresentCanvasParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: PresentCanvasParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
        let canvas_id = parse_canvas_uuid(&params.canvas_id)?;
        let canvas = self
            .canvas_repo
            .get_by_id(canvas_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            .ok_or_else(|| AgentToolError::ExecutionFailed(format!("Canvas 不存在: {canvas_id}")))?;
        ensure_canvas_project(canvas.project_id, self.project_id)?;

        let notification =
            build_canvas_presented_notification(&self.current_session_id, &self.current_turn_id, &canvas);
        let session_hub = self.session_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed("SessionHub 尚未完成初始化".to_string())
        })?;
        session_hub
            .inject_notification(&self.current_session_id, notification)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "已请求前端展示 Canvas。\n- canvas_id: {}\n- mount: {}://",
                canvas.id,
                build_canvas_mount_id(canvas.id),
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "canvas_id": canvas.id,
                "title": canvas.title,
                "mount_id": build_canvas_mount_id(canvas.id),
            })),
        })
    }
}

fn parse_canvas_uuid(raw_canvas_id: &str) -> Result<Uuid, AgentToolError> {
    Uuid::parse_str(raw_canvas_id.trim()).map_err(|_| {
        AgentToolError::InvalidArguments(format!("无效的 canvas_id: {raw_canvas_id}"))
    })
}

fn ensure_canvas_project(
    canvas_project_id: Uuid,
    expected_project_id: Uuid,
) -> Result<(), AgentToolError> {
    if canvas_project_id == expected_project_id {
        Ok(())
    } else {
        Err(AgentToolError::ExecutionFailed(
            "当前 session 无权操作其它 Project 的 Canvas".to_string(),
        ))
    }
}

fn build_canvas_presented_notification(
    session_id: &str,
    turn_id: &str,
    canvas: &agentdash_domain::canvas::Canvas,
) -> SessionNotification {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new("canvas_presented");
    event.severity = Some("info".to_string());
    event.message = Some(format!("已请求打开 Canvas `{}`", canvas.title));
    event.data = Some(serde_json::json!({
        "canvas_id": canvas.id.to_string(),
        "title": canvas.title,
        "mount_id": build_canvas_mount_id(canvas.id),
        "entry_file": canvas.entry_file,
    }));

    let source = AgentDashSourceV1::new("agentdash-canvas", "runtime_tool");
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));

    SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(
            SessionInfoUpdate::new()
                .meta(merge_agentdash_meta(None, &agentdash).unwrap_or_default()),
        ),
    )
}
