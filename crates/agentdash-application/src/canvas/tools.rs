use std::sync::Arc;

use agent_client_protocol::{SessionId, SessionInfoUpdate, SessionNotification, SessionUpdate};
use agentdash_acp_meta::{
    AgentDashEventV1, AgentDashMetaV1, AgentDashSourceV1, AgentDashTraceV1, merge_agentdash_meta,
};
use agentdash_domain::canvas::{Canvas, CanvasDataBinding, CanvasRepository};
use agentdash_spi::schema::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::vfs::build_canvas_mount_id;
use crate::vfs::tools::fs::SharedRuntimeVfs;
use crate::vfs::tools::provider::SharedSessionHubHandle;
use crate::canvas::{build_canvas, upsert_canvas_binding};

#[derive(Clone)]
pub struct ListCanvasesTool {
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
}

impl ListCanvasesTool {
    pub fn new(canvas_repo: Arc<dyn CanvasRepository>, project_id: Uuid) -> Self {
        Self {
            canvas_repo,
            project_id,
        }
    }
}

#[derive(Clone)]
pub struct StartCanvasTool {
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
    vfs: SharedRuntimeVfs,
    session_hub_handle: SharedSessionHubHandle,
    current_session_id: Option<String>,
}

impl StartCanvasTool {
    pub fn new(
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        vfs: SharedRuntimeVfs,
        session_hub_handle: SharedSessionHubHandle,
        current_session_id: Option<String>,
    ) -> Self {
        Self {
            canvas_repo,
            project_id,
            vfs,
            session_hub_handle,
            current_session_id,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartCanvasParams {
    /// Stable canvas identifier. If it matches an existing canvas, that canvas is attached to the current session; otherwise a new canvas is created with this id.
    pub canvas_id: Option<String>,
    /// Title for the new canvas. Required when `canvas_id` does not match an existing canvas.
    pub title: Option<String>,
    /// Optional description for the new canvas.
    pub description: Option<String>,
}

#[derive(Clone)]
pub struct BindCanvasDataTool {
    canvas_repo: Arc<dyn CanvasRepository>,
    project_id: Uuid,
}

impl BindCanvasDataTool {
    pub fn new(canvas_repo: Arc<dyn CanvasRepository>, project_id: Uuid) -> Self {
        Self {
            canvas_repo,
            project_id,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BindCanvasDataParams {
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
    action: String,
    canvas_id: String,
    mount_id: String,
    title: String,
    entry_file: String,
}

#[async_trait]
impl AgentTool for ListCanvasesTool {
    fn name(&self) -> &str {
        "canvases_list"
    }

    fn description(&self) -> &str {
        "List canvases in the current project. Returns canvas_id, mount_id (use as URI scheme for file operations, e.g. `<mount_id>://path`), and title for each canvas."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        _: &str,
        _: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let mut canvases = self
            .canvas_repo
            .list_by_project(self.project_id)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        canvases.sort_by(|left, right| {
            left.title
                .cmp(&right.title)
                .then_with(|| left.mount_id.cmp(&right.mount_id))
        });

        let entries = canvases
            .iter()
            .map(CanvasListEntry::from_canvas)
            .collect::<Vec<_>>();
        let body = if entries.is_empty() {
            "No canvases in the current project.".to_string()
        } else {
            format!(
                "canvas_count: {}\n{}",
                entries.len(),
                entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "- canvas_id={}  mount={}://  title={}",
                            entry.canvas_id, entry.mount_id, entry.title
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        Ok(AgentToolResult {
            content: vec![ContentPart::text(body)],
            is_error: false,
            details: Some(serde_json::json!({
                "canvas_count": entries.len(),
                "canvases": entries,
            })),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct CanvasListEntry {
    canvas_id: String,
    mount_id: String,
    title: String,
}

impl CanvasListEntry {
    fn from_canvas(canvas: &Canvas) -> Self {
        Self {
            canvas_id: canvas.mount_id.clone(),
            mount_id: build_canvas_mount_id(canvas),
            title: canvas.title.clone(),
        }
    }
}

#[async_trait]
impl AgentTool for StartCanvasTool {
    fn name(&self) -> &str {
        "canvas_start"
    }

    fn description(&self) -> &str {
        "Attach an existing canvas or create a new one. If `canvas_id` matches an existing canvas, attach it to the current session; otherwise create a new canvas with that stable id. If `canvas_id` is omitted, derive it from `title`. Returns canvas_id and mount_id; use mount_id as the URI scheme for file operations (e.g. `<mount_id>://path`)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<StartCanvasParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: StartCanvasParams = serde_json::from_value(args).map_err(|error| {
            AgentToolError::InvalidArguments(format!("invalid arguments: {error}"))
        })?;
        let requested_canvas_id = params
            .canvas_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let (canvas, action) = if let Some(canvas_id) = requested_canvas_id {
            let existing_canvas = self
                .canvas_repo
                .get_by_mount_id(self.project_id, &canvas_id)
                .await
                .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

            if let Some(canvas) = existing_canvas {
                ensure_canvas_project(canvas.project_id, self.project_id)?;
                (canvas, "attached".to_string())
            } else {
                let title = params
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        AgentToolError::InvalidArguments(
                            "title is required when canvas_id does not match an existing canvas"
                                .to_string(),
                        )
                    })?;

                let canvas = build_canvas(
                    self.project_id,
                    Some(canvas_id),
                    title.to_string(),
                    params.description.unwrap_or_default(),
                    Default::default(),
                )
                .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
                self.canvas_repo
                    .create(&canvas)
                    .await
                    .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
                (canvas, "created".to_string())
            }
        } else {
            let title = params
                .title
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AgentToolError::InvalidArguments(
                        "title is required when creating a new canvas".to_string(),
                    )
                })?;

            let canvas = build_canvas(
                self.project_id,
                None,
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
                    "canvas id already exists: {}",
                    canvas.mount_id
                )));
            }
            self.canvas_repo
                .create(&canvas)
                .await
                .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
            (canvas, "created".to_string())
        };

        expose_canvas_to_session(
            &self.vfs,
            &self.session_hub_handle,
            self.current_session_id.as_deref(),
            &canvas,
        )
        .await?;

        let result = CanvasToolResult {
            action,
            canvas_id: canvas.mount_id.clone(),
            mount_id: build_canvas_mount_id(&canvas),
            title: canvas.title.clone(),
            entry_file: canvas.entry_file.clone(),
        };
        let details = serde_json::to_value(&result).map_err(|error| {
            AgentToolError::ExecutionFailed(format!("failed to serialize canvas result: {error}"))
        })?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "action={}\ncanvas_id={}\nmount={}://\ntitle={}\nentry_file={}",
                result.action,
                result.canvas_id,
                build_canvas_mount_id(&canvas),
                result.title,
                result.entry_file
            ))],
            is_error: false,
            details: Some(details),
        })
    }
}

#[async_trait]
impl AgentTool for BindCanvasDataTool {
    fn name(&self) -> &str {
        "bind_canvas_data"
    }

    fn description(&self) -> &str {
        "Declare a canvas data binding. Maps `source_uri` to runtime path `bindings/<alias>.json`. `content_type` is optional and defaults to `application/json`."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_value::<BindCanvasDataParams>()
    }

    async fn execute(
        &self,
        _: &str,
        args: serde_json::Value,
        _: CancellationToken,
        _: Option<ToolUpdateCallback>,
    ) -> Result<AgentToolResult, AgentToolError> {
        let params: BindCanvasDataParams = serde_json::from_value(args)
            .map_err(|error| AgentToolError::InvalidArguments(format!("参数解析失败: {error}")))?;
        let mut canvas = load_canvas_by_ref(
            self.canvas_repo.as_ref(),
            self.project_id,
            &params.canvas_id,
        )
        .await?;

        let mut binding = CanvasDataBinding::new(params.alias, params.source_uri);
        if let Some(content_type) = params.content_type {
            binding.content_type = content_type;
        }
        let alias = binding.alias.clone();
        let source_uri = binding.source_uri.clone();
        let content_type = binding.content_type.clone();
        upsert_canvas_binding(&mut canvas, binding)
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
        self.canvas_repo
            .update(&canvas)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        let details_value = serde_json::json!({
            "canvas_id": canvas.mount_id,
            "mount_id": build_canvas_mount_id(&canvas),
            "bindings": canvas.bindings,
        });
        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "canvas_id={}\nmount={}://\nalias={}\nsource_uri={}\ncontent_type={}",
                canvas.mount_id,
                build_canvas_mount_id(&canvas),
                alias,
                source_uri,
                content_type
            ))],
            is_error: false,
            details: Some(details_value),
        })
    }
}

#[async_trait]
impl AgentTool for PresentCanvasTool {
    fn name(&self) -> &str {
        "present_canvas"
    }

    fn description(&self) -> &str {
        "Request the frontend to open the specified canvas."
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
        let canvas = load_canvas_by_ref(
            self.canvas_repo.as_ref(),
            self.project_id,
            &params.canvas_id,
        )
        .await?;

        let notification = build_canvas_presented_notification(
            &self.current_session_id,
            &self.current_turn_id,
            &canvas,
        )?;
        let session_hub = self.session_hub_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed("SessionHub 尚未完成初始化".to_string())
        })?;
        session_hub
            .inject_notification(&self.current_session_id, notification)
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "canvas_id={}\nmount={}://",
                canvas.mount_id,
                build_canvas_mount_id(&canvas),
            ))],
            is_error: false,
            details: Some(serde_json::json!({
                "canvas_id": canvas.mount_id,
                "mount_id": build_canvas_mount_id(&canvas),
                "title": canvas.title,
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

    let canvas = canvas_repo
        .get_by_mount_id(expected_project_id, trimmed)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    let canvas = canvas
        .ok_or_else(|| AgentToolError::ExecutionFailed(format!("Canvas 不存在: {trimmed}")))?;
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

async fn expose_canvas_to_session(
    vfs: &SharedRuntimeVfs,
    session_hub_handle: &SharedSessionHubHandle,
    current_session_id: Option<&str>,
    canvas: &Canvas,
) -> Result<(), AgentToolError> {
    vfs.append_canvas_mount(canvas).await;
    if let Some(session_hub) = session_hub_handle.get().await
        && let Some(session_id) = current_session_id
    {
        let mount_id = canvas.mount_id.clone();
        session_hub
            .update_session_meta(session_id, move |meta| {
                if !meta
                    .visible_canvas_mount_ids
                    .iter()
                    .any(|item| item == &mount_id)
                {
                    meta.visible_canvas_mount_ids.push(mount_id);
                }
            })
            .await
            .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?;
    }
    Ok(())
}

fn build_canvas_presented_notification(
    session_id: &str,
    turn_id: &str,
    canvas: &agentdash_domain::canvas::Canvas,
) -> Result<SessionNotification, AgentToolError> {
    let mut trace = AgentDashTraceV1::new();
    trace.turn_id = Some(turn_id.to_string());

    let mut event = AgentDashEventV1::new("canvas_presented");
    event.severity = Some("info".to_string());
    event.message = Some(format!("已请求打开 Canvas `{}`", canvas.title));
    event.data = Some(serde_json::json!({
        "canvas_id": canvas.mount_id,
        "title": canvas.title,
        "entry_file": canvas.entry_file,
    }));

    let source = AgentDashSourceV1::new("agentdash-canvas", "runtime_tool");
    let agentdash = AgentDashMetaV1::new()
        .source(Some(source))
        .trace(Some(trace))
        .event(Some(event));

    let meta = merge_agentdash_meta(None, &agentdash)
        .ok_or_else(|| AgentToolError::ExecutionFailed("无法构造 AgentDash meta".to_string()))?;
    Ok(SessionNotification::new(
        SessionId::new(session_id.to_string()),
        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().meta(meta)),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use agentdash_domain::DomainError;
    use agentdash_spi::Vfs;
    use async_trait::async_trait;
    use tokio::sync::RwLock;

    use crate::vfs::tools::fs::FsApplyPatchTool;
    use crate::vfs::{
        CanvasFsMountProvider, MountProviderRegistry, RelayVfsService,
    };

    use super::*;

    #[derive(Default)]
    struct MemoryCanvasRepository {
        canvases: RwLock<HashMap<Uuid, agentdash_domain::canvas::Canvas>>,
    }

    #[async_trait]
    impl CanvasRepository for MemoryCanvasRepository {
        async fn create(
            &self,
            canvas: &agentdash_domain::canvas::Canvas,
        ) -> Result<(), DomainError> {
            self.canvases
                .write()
                .await
                .insert(canvas.id, canvas.clone());
            Ok(())
        }

        async fn get_by_id(
            &self,
            id: Uuid,
        ) -> Result<Option<agentdash_domain::canvas::Canvas>, DomainError> {
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

        async fn update(
            &self,
            canvas: &agentdash_domain::canvas::Canvas,
        ) -> Result<(), DomainError> {
            self.canvases
                .write()
                .await
                .insert(canvas.id, canvas.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.canvases.write().await.remove(&id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn canvas_start_creates_shared_mounts_for_followup_apply_patch() {
        let project_id = Uuid::new_v4();
        let canvas_repo = Arc::new(MemoryCanvasRepository::default());

        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(CanvasFsMountProvider::new(canvas_repo.clone())));
        let service = Arc::new(RelayVfsService::new(Arc::new(registry)));
        let shared_vfs = SharedRuntimeVfs::new(Vfs::default());

        let start_tool = StartCanvasTool::new(
            canvas_repo.clone(),
            project_id,
            shared_vfs.clone(),
            SharedSessionHubHandle::default(),
            Some("sess-test".to_string()),
        );
        let patch_tool = FsApplyPatchTool::new(service, shared_vfs.clone(), None, None);

        let create_result = start_tool
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
            .expect("canvas_start should succeed");
        let create_details = create_result
            .details
            .as_ref()
            .expect("canvas_start should return details");
        let canvas_id = create_details
            .get("canvas_id")
            .and_then(serde_json::Value::as_str)
            .expect("canvas_id should exist");

        let vfs = shared_vfs.snapshot().await;
        let expected_mount_id = format!("cvs-{canvas_id}");
        assert!(
            vfs
                .mounts
                .iter()
                .any(|mount| mount.id == expected_mount_id),
            "shared VFS should contain the new canvas mount after canvas_start"
        );

        let patch_content = format!(
            "*** Begin Patch\n*** Add File: cvs-{canvas_id}://src/util.ts\n+export const value = 'ok';\n*** End Patch"
        );
        patch_tool
            .execute(
                "tool-patch",
                serde_json::json!({
                    "patch": patch_content
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("fs_apply_patch should write to the newly created canvas mount");

        let saved = canvas_repo
            .get_by_mount_id(project_id, canvas_id)
            .await
            .expect("repo query should succeed")
            .expect("canvas should exist");
        let util_file = saved
            .files
            .iter()
            .find(|file| file.path == "src/util.ts")
            .expect("src/util.ts should exist");
        assert_eq!(util_file.content, "export const value = 'ok';\n");
    }

    #[tokio::test]
    async fn canvas_start_attaches_existing_canvas_mount() {
        let project_id = Uuid::new_v4();
        let canvas_repo = Arc::new(MemoryCanvasRepository::default());
        let existing_canvas = build_canvas(
            project_id,
            Some("existing-kpi".to_string()),
            "Existing KPI".to_string(),
            "already created".to_string(),
            Default::default(),
        )
        .expect("应能构建 canvas");
        canvas_repo
            .create(&existing_canvas)
            .await
            .expect("应能写入仓储");

        let shared_vfs = SharedRuntimeVfs::new(Vfs::default());
        let start_tool = StartCanvasTool::new(
            canvas_repo,
            project_id,
            shared_vfs.clone(),
            SharedSessionHubHandle::default(),
            Some("sess-test".to_string()),
        );

        let result = start_tool
            .execute(
                "tool-start",
                serde_json::json!({
                    "canvas_id": "existing-kpi"
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("应能接入已有 canvas");

        let details = result.details.expect("应返回 details");
        assert_eq!(
            details.get("action").and_then(serde_json::Value::as_str),
            Some("attached")
        );
        assert_eq!(
            details.get("canvas_id").and_then(serde_json::Value::as_str),
            Some("existing-kpi")
        );
        let text = match &result.content[0] {
            ContentPart::Text { text } => text.as_str(),
            other => panic!("unexpected content part: {other:?}"),
        };
        assert!(text.contains("mount=cvs-existing-kpi://"));

        let vfs = shared_vfs.snapshot().await;
        assert!(
            vfs
                .mounts
                .iter()
                .any(|mount| mount.id == "cvs-existing-kpi"),
            "shared VFS should contain the attached canvas mount"
        );
    }

    #[tokio::test]
    async fn canvases_list_returns_project_canvas_summaries() {
        let project_id = Uuid::new_v4();
        let canvas_repo = Arc::new(MemoryCanvasRepository::default());
        let canvas = build_canvas(
            project_id,
            Some("dashboard-a".to_string()),
            "Dashboard A".to_string(),
            "demo".to_string(),
            Default::default(),
        )
        .expect("应能构建 canvas");
        canvas_repo.create(&canvas).await.expect("应能写入仓储");

        let list_tool = ListCanvasesTool::new(canvas_repo, project_id);
        let result = list_tool
            .execute(
                "tool-list",
                serde_json::json!({}),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("应能列出 canvases");

        let text = match &result.content[0] {
            ContentPart::Text { text } => text.as_str(),
            other => panic!("unexpected content part: {other:?}"),
        };
        assert!(text.contains("canvas_count: 1"));
        assert!(text.contains("canvas_id=dashboard-a"));
        assert!(text.contains("mount=cvs-dashboard-a://"));
        assert!(text.contains("title=Dashboard A"));

        let details = result.details.expect("应返回 details");
        let entries = details
            .get("canvases")
            .and_then(serde_json::Value::as_array)
            .expect("details.canvases 应为数组");
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]
                .get("canvas_id")
                .and_then(serde_json::Value::as_str),
            Some("dashboard-a")
        );
        assert_eq!(
            entries[0]
                .get("mount_id")
                .and_then(serde_json::Value::as_str),
            Some("cvs-dashboard-a")
        );
        assert!(entries[0].get("entry_file").is_none());
    }

    #[tokio::test]
    async fn canvas_start_creates_new_canvas_with_requested_canvas_id() {
        let project_id = Uuid::new_v4();
        let canvas_repo = Arc::new(MemoryCanvasRepository::default());
        let shared_vfs = SharedRuntimeVfs::new(Vfs::default());
        let start_tool = StartCanvasTool::new(
            canvas_repo.clone(),
            project_id,
            shared_vfs,
            SharedSessionHubHandle::default(),
            Some("sess-test".to_string()),
        );

        let result = start_tool
            .execute(
                "tool-start",
                serde_json::json!({
                    "canvas_id": "planned-kpi",
                    "title": "Planned KPI"
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("应能使用指定 canvas_id 创建");

        let details = result.details.expect("应返回 details");
        assert_eq!(
            details.get("action").and_then(serde_json::Value::as_str),
            Some("created")
        );
        assert_eq!(
            details.get("canvas_id").and_then(serde_json::Value::as_str),
            Some("planned-kpi")
        );
        let text = match &result.content[0] {
            ContentPart::Text { text } => text.as_str(),
            other => panic!("unexpected content part: {other:?}"),
        };
        assert!(text.contains("mount=cvs-planned-kpi://"));

        let saved = canvas_repo
            .get_by_mount_id(project_id, "planned-kpi")
            .await
            .expect("repo query should succeed")
            .expect("canvas should exist");
        assert_eq!(saved.title, "Planned KPI");
    }

    #[tokio::test]
    async fn bind_canvas_data_defaults_content_type_to_json() {
        let project_id = Uuid::new_v4();
        let canvas_repo = Arc::new(MemoryCanvasRepository::default());
        let canvas = build_canvas(
            project_id,
            Some("binding-demo".to_string()),
            "Binding Demo".to_string(),
            String::new(),
            Default::default(),
        )
        .expect("应能构建 canvas");
        canvas_repo.create(&canvas).await.expect("应能写入仓储");

        let bind_tool = BindCanvasDataTool::new(canvas_repo.clone(), project_id);
        bind_tool
            .execute(
                "tool-bind",
                serde_json::json!({
                    "canvas_id": "binding-demo",
                    "alias": "orders",
                    "source_uri": "main://tmp/orders.json"
                }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("应能声明绑定");

        let saved = canvas_repo
            .get_by_mount_id(project_id, "binding-demo")
            .await
            .expect("repo query should succeed")
            .expect("canvas should exist");
        let binding = saved
            .bindings
            .iter()
            .find(|binding| binding.alias == "orders")
            .expect("orders binding should exist");
        assert_eq!(binding.content_type, "application/json");
    }
}
