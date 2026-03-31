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
use crate::address_space::tools::fs::SharedRuntimeAddressSpace;
use crate::address_space::tools::provider::SharedSessionHubHandle;
use crate::canvas::{build_canvas, upsert_canvas_binding};

#[derive(Clone)]
pub struct CreateCanvasTool {
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    address_space: SharedRuntimeAddressSpace,
}

impl CreateCanvasTool {
    pub fn new(
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        address_space: SharedRuntimeAddressSpace,
    ) -> Self {
        Self {
            canvas_repo,
            project_id,
            address_space,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateCanvasParams {
    pub id: Option<String>,
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
        "在当前 Project 下创建一个新的 Canvas 资产。可选传入稳定 id，返回的 canvas_id 可直接作为 mount id 使用"
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
            params.id,
            title.to_string(),
            params.description.unwrap_or_default(),
            Default::default(),
        )
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        if self
            .canvas_repo
            .get_by_mount_id(self.project_id, &canvas.mount_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
            .is_some()
        {
            return Err(AgentToolError::ExecutionFailed(format!(
                "Canvas id 已存在: {}",
                canvas.mount_id
            )));
        }
        self.canvas_repo
            .create(&canvas)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        self.address_space.append_canvas_mount(&canvas).await;

        let result = CanvasToolResult {
            canvas_id: canvas.mount_id.clone(),
            mount_id: build_canvas_mount_id(&canvas),
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
        "为 Canvas 绑定一个外部数据文件引用，在运行时映射为 bindings/<alias>.json。canvas_id 支持稳定 id 或内部 UUID"
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
        let mut canvas = load_canvas_by_ref(self.canvas_repo.as_ref(), self.project_id, &params.canvas_id)
            .await?;

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
                canvas.mount_id, alias, source_uri
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "canvas_id": canvas.mount_id,
                "mount_id": canvas.mount_id,
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
        "通过 ACP 系统事件请求前端打开指定 Canvas 面板。canvas_id 支持稳定 id 或内部 UUID"
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
        let canvas = load_canvas_by_ref(self.canvas_repo.as_ref(), self.project_id, &params.canvas_id)
            .await?;

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
                canvas.mount_id,
                build_canvas_mount_id(&canvas),
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "canvas_id": canvas.mount_id,
                "title": canvas.title,
                "mount_id": build_canvas_mount_id(&canvas),
            })),
        })
    }
}

async fn load_canvas_by_ref(
    canvas_repo: &dyn CanvasRepository,
    expected_project_id: Uuid,
    raw_canvas_id: &str,
) -> Result<agentdash_domain::canvas::Canvas, AgentToolError> {
    let trimmed = raw_canvas_id.trim();
    if trimmed.is_empty() {
        return Err(AgentToolError::InvalidArguments(
            "canvas_id 不能为空".to_string(),
        ));
    }

    let canvas = if let Ok(canvas_uuid) = Uuid::parse_str(trimmed) {
        canvas_repo
            .get_by_id(canvas_uuid)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
    } else {
        canvas_repo
            .get_by_mount_id(expected_project_id, trimmed)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
    };
    let canvas = canvas.ok_or_else(|| {
        AgentToolError::ExecutionFailed(format!("Canvas 不存在: {trimmed}"))
    })?;
    ensure_canvas_project(canvas.project_id, expected_project_id)?;
    Ok(canvas)
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
        "mount_id": build_canvas_mount_id(canvas),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use agentdash_domain::DomainError;
    use agentdash_spi::AddressSpace;
    use async_trait::async_trait;
    use tokio::sync::RwLock;

    use crate::address_space::tools::fs::FsWriteTool;
    use crate::address_space::{CanvasFsMountProvider, MountProviderRegistry, RelayAddressSpaceService};

    use super::*;

    #[derive(Default)]
    struct MemoryCanvasRepository {
        canvases: RwLock<HashMap<Uuid, agentdash_domain::canvas::Canvas>>,
    }

    #[async_trait]
    impl CanvasRepository for MemoryCanvasRepository {
        async fn create(&self, canvas: &agentdash_domain::canvas::Canvas) -> Result<(), DomainError> {
            self.canvases.write().await.insert(canvas.id, canvas.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<agentdash_domain::canvas::Canvas>, DomainError> {
            Ok(self.canvases.read().await.get(&id).cloned())
        }

        async fn get_by_mount_id(
            &self,
            project_id: Uuid,
            mount_id: &str,
        ) -> Result<Option<agentdash_domain::canvas::Canvas>, DomainError> {
            Ok(self
                .canvases
                .read()
                .await
                .values()
                .find(|canvas| canvas.project_id == project_id && canvas.mount_id == mount_id)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<agentdash_domain::canvas::Canvas>, DomainError> {
            Ok(self
                .canvases
                .read()
                .await
                .values()
                .filter(|canvas| canvas.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, canvas: &agentdash_domain::canvas::Canvas) -> Result<(), DomainError> {
            self.canvases.write().await.insert(canvas.id, canvas.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.canvases.write().await.remove(&id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn create_canvas_updates_shared_mounts_for_followup_fs_write() {
        let project_id = Uuid::new_v4();
        let canvas_repo = Arc::new(MemoryCanvasRepository::default());

        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(CanvasFsMountProvider::new(canvas_repo.clone())));
        let service = Arc::new(RelayAddressSpaceService::new(Arc::new(registry)));
        let shared_address_space = SharedRuntimeAddressSpace::new(AddressSpace::default());

        let create_tool =
            CreateCanvasTool::new(canvas_repo.clone(), project_id, shared_address_space.clone());
        let write_tool = FsWriteTool::new(
            service,
            shared_address_space.clone(),
            None,
            None,
        );

        let create_result = create_tool
            .execute(
                "tool-create",
                serde_json::json!({
                    "title": "agent mounted kpi runtime test",
                    "description": "runtime mount visibility regression test"
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("create_canvas 应成功");
        let create_details = create_result
            .details
            .as_ref()
            .expect("create_canvas 应返回 details");
        let mount_id = create_details
            .get("mount_id")
            .and_then(serde_json::Value::as_str)
            .expect("mount_id 应存在");

        let address_space = shared_address_space.snapshot().await;
        assert!(
            address_space.mounts.iter().any(|mount| mount.id == mount_id),
            "create_canvas 后共享 address space 应包含新 mount"
        );

        write_tool
            .execute(
                "tool-write",
                serde_json::json!({
                    "path": format!("{mount_id}://src/main.tsx"),
                    "content": "export const value = 'ok';",
                    "append": false
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("fs_write 应能在同一轮写入新建 canvas mount");

        let saved = canvas_repo
            .get_by_mount_id(project_id, mount_id)
            .await
            .expect("repo 查询应成功")
            .expect("canvas 应存在");
        let main_file = saved
            .files
            .iter()
            .find(|file| file.path == "src/main.tsx")
            .expect("应存在 src/main.tsx");
        assert_eq!(main_file.content, "export const value = 'ok';");
    }
}
