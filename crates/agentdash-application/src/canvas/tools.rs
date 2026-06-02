use std::sync::Arc;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_domain::canvas::{
    CANVAS_SYSTEM_SKILL_NAME, CANVAS_SYSTEM_SKILL_PATH, Canvas, CanvasDataBinding, CanvasRepository,
};
use agentdash_spi::context::tool_schema_sanitizer::schema_value;
use agentdash_spi::{AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::canvas::{build_canvas, upsert_canvas_binding};
use crate::session::AgentFrameRuntimeTarget;
use crate::vfs::build_canvas_mount_id;
use crate::vfs::tools::SharedSessionToolServicesHandle;
use crate::vfs::tools::fs::SharedRuntimeVfs;

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
    session_services_handle: SharedSessionToolServicesHandle,
    current_session_id: Option<String>,
}

impl StartCanvasTool {
    pub fn new(
        canvas_repo: Arc<dyn CanvasRepository>,
        project_id: Uuid,
        vfs: SharedRuntimeVfs,
        session_services_handle: SharedSessionToolServicesHandle,
        current_session_id: Option<String>,
    ) -> Self {
        Self {
            canvas_repo,
            project_id,
            vfs,
            session_services_handle,
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
    vfs: SharedRuntimeVfs,
    session_services_handle: SharedSessionToolServicesHandle,
    current_session_id: String,
    current_turn_id: String,
    project_id: Uuid,
}

impl PresentCanvasTool {
    pub fn new(
        canvas_repo: Arc<dyn CanvasRepository>,
        vfs: SharedRuntimeVfs,
        session_services_handle: SharedSessionToolServicesHandle,
        current_session_id: String,
        current_turn_id: String,
        project_id: Uuid,
    ) -> Self {
        Self {
            canvas_repo,
            vfs,
            session_services_handle,
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
    skill_name: String,
    skill_path: String,
}

#[async_trait]
impl AgentTool for ListCanvasesTool {
    fn name(&self) -> &str {
        "canvases_list"
    }

    fn description(&self) -> &str {
        "List canvases in the current project. Returns canvas_id, mount_id (use as URI scheme for file operations, e.g. `<mount_id>://path`), and title for each canvas. New canvases include the `canvas-system` skill at `<mount_id>://skills/canvas-system/SKILL.md`."
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
        "Attach an existing canvas or create a new one. If `canvas_id` matches an existing canvas, attach it to the current session; otherwise create a new canvas with that stable id. If `canvas_id` is omitted, derive it from `title`. New canvases include the `canvas-system` skill; the result returns canvas_id, mount_id, entry_file, and skill_path so the agent can load the canvas rules before editing."
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
            &self.session_services_handle,
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
            skill_name: CANVAS_SYSTEM_SKILL_NAME.to_string(),
            skill_path: format!(
                "{}://{}",
                build_canvas_mount_id(&canvas),
                CANVAS_SYSTEM_SKILL_PATH
            ),
        };
        let details = serde_json::to_value(&result).map_err(|error| {
            AgentToolError::ExecutionFailed(format!("failed to serialize canvas result: {error}"))
        })?;

        Ok(AgentToolResult {
            content: vec![ContentPart::text(format!(
                "action={}\ncanvas_id={}\nmount={}://\ntitle={}\nentry_file={}\nskill={}\nskill_path={}",
                result.action,
                result.canvas_id,
                build_canvas_mount_id(&canvas),
                result.title,
                result.entry_file,
                result.skill_name,
                result.skill_path
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
        "Declare a canvas data binding. Maps `source_uri` to runtime path `bindings/<alias>.json`. `content_type` is optional and defaults to `application/json`. The `canvas-system` skill explains how bound JSON files are imported from canvas source code."
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

        expose_canvas_to_session(
            &self.vfs,
            &self.session_services_handle,
            Some(&self.current_session_id),
            &canvas,
        )
        .await?;

        let notification = build_canvas_presented_notification(
            &self.current_session_id,
            &self.current_turn_id,
            &canvas,
        )?;
        let session_services = self.session_services_handle.get().await.ok_or_else(|| {
            AgentToolError::ExecutionFailed("Session services 尚未完成初始化".to_string())
        })?;
        session_services
            .eventing
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
    session_services_handle: &SharedSessionToolServicesHandle,
    current_session_id: Option<&str>,
    canvas: &Canvas,
) -> Result<(), AgentToolError> {
    vfs.append_canvas_mount(canvas).await;
    if let Some(session_services) = session_services_handle.get().await
        && let Some(session_id) = current_session_id
    {
        let mount_id = canvas.mount_id.clone();
        if let Err(error) = session_services
            .capability
            .append_visible_canvas_mount_to_frame(session_id, &mount_id)
            .await
        {
            tracing::warn!(
                session_id = %session_id,
                mount_id = %mount_id,
                %error,
                "Canvas mount 写入 AgentFrame 失败，降级为仅 VFS 可见"
            );
        }
        sync_canvas_mount_capability_state_for_runtime_delivery(
            vfs,
            &session_services,
            session_id,
            canvas,
        )
        .await?;
    }
    Ok(())
}

async fn sync_canvas_mount_capability_state_for_runtime_delivery(
    vfs: &SharedRuntimeVfs,
    session_services: &crate::vfs::tools::SessionToolServices,
    session_id: &str,
    canvas: &Canvas,
) -> Result<(), AgentToolError> {
    let Some(before_state) = session_services
        .capability
        .get_latest_capability_state(session_id)
        .await
    else {
        tracing::debug!(
            session_id = %session_id,
            canvas_id = %canvas.mount_id,
            "Canvas mount 已写入 VFS，但当前 session 尚无 CapabilityState 可同步"
        );
        return Ok(());
    };

    let target = session_services
        .capability
        .resolve_runtime_session_target(session_id)
        .await
        .map_err(AgentToolError::ExecutionFailed)?;

    let Some(hook_runtime) = session_services
        .hooks
        .get_hook_runtime_for_target(&target)
        .await
        .map_err(|error| AgentToolError::ExecutionFailed(error.to_string()))?
    else {
        tracing::debug!(
            session_id = %session_id,
            canvas_id = %canvas.mount_id,
            "Canvas mount 已写入 VFS，但当前 session 尚无 Hook runtime 可接收能力状态热更新"
        );
        return Ok(());
    };

    sync_canvas_mount_capability_state(vfs, session_services, target, before_state, hook_runtime)
        .await
}

async fn sync_canvas_mount_capability_state(
    vfs: &SharedRuntimeVfs,
    session_services: &crate::vfs::tools::SessionToolServices,
    target: AgentFrameRuntimeTarget,
    before_state: crate::session::CapabilityState,
    hook_runtime: agentdash_spi::hooks::SharedHookRuntime,
) -> Result<(), AgentToolError> {
    let active_vfs = vfs.snapshot().await;
    session_services
        .capability
        .apply_live_vfs_capability_state(
            &hook_runtime,
            target,
            before_state,
            active_vfs,
            "canvas",
            "canvas_visible",
        )
        .await
        .map_err(AgentToolError::ExecutionFailed)?;
    Ok(())
}

fn build_canvas_presented_notification(
    session_id: &str,
    turn_id: &str,
    canvas: &agentdash_domain::canvas::Canvas,
) -> Result<BackboneEnvelope, AgentToolError> {
    let source = SourceInfo {
        connector_id: "agentdash-canvas".to_string(),
        connector_type: "runtime_tool".to_string(),
        executor_id: None,
    };

    Ok(BackboneEnvelope::new(
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
            key: "canvas_presented".to_string(),
            value: serde_json::json!({
                "canvas_id": canvas.mount_id,
                "title": canvas.title,
                "entry_file": canvas.entry_file,
            }),
        }),
        session_id,
        source,
    )
    .with_trace(TraceInfo {
        turn_id: Some(turn_id.to_string()),
        entry_index: None,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use agentdash_domain::DomainError;
    use agentdash_domain::workflow::{
        AgentFrame, AgentFrameRepository, LifecycleGate, LifecycleGateRepository,
    };
    use agentdash_spi::hooks::{
        ActiveWorkflowMeta, AgentFrameHookEvaluationQuery, AgentFrameHookRefreshQuery,
        AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, ExecutionHookProvider, HookResolution,
        SessionSnapshotMetadata,
    };
    use agentdash_spi::{AgentConnector, CapabilityState, ConnectorError, PromptPayload, Vfs};
    use async_trait::async_trait;
    use futures::stream;
    use tokio::sync::RwLock;

    use crate::session::construction::{
        ConstructionResolutionPlan, OwnerResolutionTrace, ResolvedSessionOwner,
        RuntimeContextInspectionPlan,
    };
    use crate::session::hub::SessionRuntimeInner;
    use crate::session::{MemorySessionPersistence, UserPromptInput, local_workspace_vfs};
    use crate::vfs::tools::SessionToolServices;
    use crate::vfs::tools::fs::FsApplyPatchTool;
    use crate::vfs::{CanvasFsMountProvider, MountProviderRegistry, VfsService};

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

        async fn find_by_mount_id(
            &self,
            mount_id: &str,
        ) -> Result<Option<agentdash_domain::canvas::Canvas>, DomainError> {
            Ok(self
                .canvases
                .read()
                .await
                .values()
                .find(|canvas| canvas.mount_id == mount_id)
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

    #[derive(Default)]
    struct MemoryAgentFrameRepository {
        frames: RwLock<Vec<AgentFrame>>,
    }

    #[async_trait]
    impl AgentFrameRepository for MemoryAgentFrameRepository {
        async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
            self.frames.write().await.push(frame.clone());
            Ok(())
        }

        async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .read()
                .await
                .iter()
                .find(|frame| frame.id == frame_id)
                .cloned())
        }

        async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
            let frames = self.frames.read().await;
            Ok(frames
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .read()
                .await
                .iter()
                .filter(|frame| frame.agent_id == agent_id)
                .cloned()
                .collect())
        }

        async fn attach_runtime_session_ref(
            &self,
            frame_id: Uuid,
            runtime_session_id: &str,
        ) -> Result<(), DomainError> {
            let mut frames = self.frames.write().await;
            let frame = frames
                .iter_mut()
                .find(|frame| frame.id == frame_id)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "agent_frame",
                    id: frame_id.to_string(),
                })?;
            frame.attach_runtime_session_ref(runtime_session_id);
            Ok(())
        }

        async fn find_by_runtime_session(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<AgentFrame>, DomainError> {
            Ok(self
                .frames
                .read()
                .await
                .iter()
                .filter(|frame| {
                    frame
                        .runtime_session_ids()
                        .iter()
                        .any(|session_id| session_id == runtime_session_id)
                })
                .max_by_key(|frame| frame.revision)
                .cloned())
        }

        async fn append_visible_canvas_mount(
            &self,
            frame_id: Uuid,
            mount_id: &str,
        ) -> Result<(), DomainError> {
            let mut frames = self.frames.write().await;
            let frame = frames
                .iter_mut()
                .find(|frame| frame.id == frame_id)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "agent_frame",
                    id: frame_id.to_string(),
                })?;
            frame.append_visible_canvas_mount(mount_id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryLifecycleGateRepository {
        gates: RwLock<Vec<LifecycleGate>>,
    }

    #[async_trait]
    impl LifecycleGateRepository for MemoryLifecycleGateRepository {
        async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            self.gates.write().await.push(gate.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .read()
                .await
                .iter()
                .find(|gate| gate.id == id)
                .cloned())
        }

        async fn list_open_for_agent(
            &self,
            agent_id: Uuid,
        ) -> Result<Vec<LifecycleGate>, DomainError> {
            Ok(self
                .gates
                .read()
                .await
                .iter()
                .filter(|gate| gate.agent_id == Some(agent_id) && gate.is_open())
                .cloned()
                .collect())
        }

        async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
            let mut gates = self.gates.write().await;
            if let Some(existing) = gates.iter_mut().find(|existing| existing.id == gate.id) {
                *existing = gate.clone();
                return Ok(());
            }
            Err(DomainError::NotFound {
                entity: "lifecycle_gate",
                id: gate.id.to_string(),
            })
        }
    }

    #[derive(Default)]
    struct PendingConnector;

    #[async_trait]
    impl AgentConnector for PendingConnector {
        fn connector_id(&self) -> &'static str {
            "pending"
        }

        fn connector_type(&self) -> agentdash_spi::ConnectorType {
            agentdash_spi::ConnectorType::LocalExecutor
        }

        fn capabilities(&self) -> agentdash_spi::ConnectorCapabilities {
            agentdash_spi::ConnectorCapabilities::default()
        }

        fn list_executors(&self) -> Vec<agentdash_spi::AgentInfo> {
            Vec::new()
        }

        async fn discover_options_stream(
            &self,
            _executor: &str,
            _working_dir: Option<PathBuf>,
        ) -> Result<futures::stream::BoxStream<'static, json_patch::Patch>, ConnectorError>
        {
            Ok(Box::pin(stream::empty()))
        }

        async fn prompt(
            &self,
            _session_id: &str,
            _follow_up_session_id: Option<&str>,
            _prompt: &PromptPayload,
            _context: agentdash_spi::ExecutionContext,
        ) -> Result<agentdash_spi::ExecutionStream, ConnectorError> {
            Ok(Box::pin(stream::pending()))
        }

        async fn cancel(&self, _session_id: &str) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn approve_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }

        async fn reject_tool_call(
            &self,
            _session_id: &str,
            _tool_call_id: &str,
            _reason: Option<String>,
        ) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    struct EmptyHookProvider {
        active_run_id: Uuid,
        frame_repo: Arc<MemoryAgentFrameRepository>,
    }

    impl EmptyHookProvider {
        fn snapshot(&self, session_id: String) -> AgentFrameHookSnapshot {
            AgentFrameHookSnapshot {
                session_id,
                metadata: Some(SessionSnapshotMetadata {
                    active_workflow: Some(ActiveWorkflowMeta {
                        run_id: Some(self.active_run_id),
                        ..ActiveWorkflowMeta::default()
                    }),
                    ..SessionSnapshotMetadata::default()
                }),
                ..AgentFrameHookSnapshot::default()
            }
        }
    }

    #[async_trait]
    impl ExecutionHookProvider for EmptyHookProvider {
        async fn resolve_runtime_hook_target(
            &self,
            runtime_session_id: &str,
        ) -> Result<Option<agentdash_spi::hooks::HookControlTarget>, agentdash_spi::hooks::HookError>
        {
            let frame = self
                .frame_repo
                .find_by_runtime_session(runtime_session_id)
                .await
                .map_err(|e| agentdash_spi::hooks::HookError::Runtime(e.to_string()))?;
            Ok(frame.map(|f| agentdash_spi::hooks::HookControlTarget {
                run_id: self.active_run_id,
                agent_id: f.agent_id,
                frame_id: f.id,
                assignment_id: None,
            }))
        }

        async fn load_frame_snapshot(
            &self,
            query: AgentFrameHookSnapshotQuery,
        ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
            Ok(self.snapshot(query.provenance.runtime_session_id.unwrap_or_default()))
        }

        async fn refresh_frame_snapshot(
            &self,
            query: AgentFrameHookRefreshQuery,
        ) -> Result<AgentFrameHookSnapshot, agentdash_spi::hooks::HookError> {
            Ok(self.snapshot(query.provenance.runtime_session_id.unwrap_or_default()))
        }

        async fn evaluate_frame_hook(
            &self,
            _query: AgentFrameHookEvaluationQuery,
        ) -> Result<HookResolution, agentdash_spi::hooks::HookError> {
            Ok(HookResolution::default())
        }
    }

    fn prompt_construction(
        session_id: &str,
        project_id: Uuid,
        working_dir: &std::path::Path,
    ) -> RuntimeContextInspectionPlan {
        let user_input = UserPromptInput {
            executor_config: Some(agentdash_spi::AgentConfig::new("PI_AGENT")),
            ..UserPromptInput::from_text("present canvas")
        };
        let owner = ResolvedSessionOwner {
            owner_type: agentdash_spi::CapabilityScope::Project,
            project_id: Some(project_id),
            trace: OwnerResolutionTrace {
                selected_reason: "test".to_string(),
            },
        };
        let mut construction =
            RuntimeContextInspectionPlan::from_source_input(session_id, owner, &user_input);
        let vfs = local_workspace_vfs(working_dir);
        let mut capability_state =
            CapabilityState::from_clusters([agentdash_spi::ToolCluster::Canvas]);
        capability_state.vfs.active = Some(vfs.clone());
        construction.workspace.working_directory = Some(working_dir.to_path_buf());
        construction.execution_profile.executor_config = user_input.executor_config;
        construction.surface.vfs = Some(vfs);
        construction.projections.capability_state = Some(capability_state);
        construction.resolution = ConstructionResolutionPlan {
            vfs_source: Some("test.local_workspace_vfs".to_string()),
            mcp_source: Some("test.empty".to_string()),
            capability_source: Some("test.capability_state".to_string()),
            executor_source: Some("test.executor_config".to_string()),
            working_directory_source: Some("test.working_dir".to_string()),
            pending_overlay_applied: false,
            runtime_base_capability_state: None,
        };
        construction
    }

    #[tokio::test]
    async fn canvas_start_creates_shared_mounts_for_followup_apply_patch() {
        let project_id = Uuid::new_v4();
        let canvas_repo = Arc::new(MemoryCanvasRepository::default());

        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(CanvasFsMountProvider::new(canvas_repo.clone())));
        let service = Arc::new(VfsService::new(Arc::new(registry)));
        let shared_vfs = SharedRuntimeVfs::new(Vfs::default());

        let start_tool = StartCanvasTool::new(
            canvas_repo.clone(),
            project_id,
            shared_vfs.clone(),
            SharedSessionToolServicesHandle::default(),
            Some("sess-test".to_string()),
        );
        let patch_tool = FsApplyPatchTool::new(service.clone(), shared_vfs.clone(), None, None);

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
        assert_eq!(
            create_details
                .get("skill_name")
                .and_then(serde_json::Value::as_str),
            Some("canvas-system")
        );

        let vfs = shared_vfs.snapshot().await;
        let expected_mount_id = format!("cvs-{canvas_id}");
        assert!(
            vfs.mounts.iter().any(|mount| mount.id == expected_mount_id),
            "shared VFS should contain the new canvas mount after canvas_start"
        );
        let discovered_skills = crate::skill::load_skills_from_vfs(&service, &vfs).await;
        assert!(
            discovered_skills
                .skills
                .iter()
                .any(|skill| skill.name == "canvas-system"
                    && skill
                        .file_path
                        .to_string_lossy()
                        .ends_with("skills/canvas-system/SKILL.md")),
            "new canvas mounts should expose the managed canvas-system skill"
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
            canvas_repo.clone(),
            project_id,
            shared_vfs.clone(),
            SharedSessionToolServicesHandle::default(),
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
            vfs.mounts
                .iter()
                .any(|mount| mount.id == "cvs-existing-kpi"),
            "shared VFS should contain the attached canvas mount"
        );
    }

    #[tokio::test]
    async fn present_canvas_updates_meta_capability_skill_and_events() {
        let project_id = Uuid::new_v4();
        let canvas_repo = Arc::new(MemoryCanvasRepository::default());
        let canvas = build_canvas(
            project_id,
            Some("demo".to_string()),
            "Demo".to_string(),
            "already created".to_string(),
            Default::default(),
        )
        .expect("应能构建 canvas");
        canvas_repo.create(&canvas).await.expect("应能写入仓储");

        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(CanvasFsMountProvider::new(canvas_repo.clone())));
        let vfs_service = Arc::new(VfsService::new(Arc::new(registry)));
        let base = tempfile::tempdir().expect("tempdir");
        let active_run_id = Uuid::new_v4();
        let frame_repo = Arc::new(MemoryAgentFrameRepository::default());
        let gate_repo = Arc::new(MemoryLifecycleGateRepository::default());
        let hub = SessionRuntimeInner::new_with_hooks_and_persistence(
            Arc::new(PendingConnector),
            Some(Arc::new(EmptyHookProvider {
                active_run_id,
                frame_repo: frame_repo.clone(),
            })),
            Arc::new(MemorySessionPersistence::default()),
        )
        .with_vfs_service(vfs_service)
        .with_agent_frame_repo(frame_repo.clone())
        .with_lifecycle_gate_repo(gate_repo);
        let session = hub
            .create_session("present-canvas")
            .await
            .expect("session 应能创建");
        let frame = AgentFrame::new_initial(
            Uuid::new_v4(),
            AgentFrame::runtime_session_refs_json([session.id.as_str()]),
        );
        frame_repo.create(&frame).await.expect("frame 应能写入");
        hub.ensure_session(&session.id).await;
        let turn_id = hub
            .start_prompt(
                &session.id,
                prompt_construction(&session.id, project_id, base.path()),
            )
            .await
            .expect("prompt 应能启动");
        hub.hook_service()
            .reload_hook_runtime(&session.id, &turn_id, "PI_AGENT", None, base.path())
            .await
            .expect("hook runtime 应能刷新");

        let handle = SharedSessionToolServicesHandle::default();
        handle
            .set(SessionToolServices {
                core: hub.core_service(),
                eventing: hub.eventing_service(),
                control: hub.control_service(),
                launch: hub.launch_service(),
                hooks: hub.hook_service(),
                capability: hub.capability_service(),
            })
            .await;

        let shared_vfs = SharedRuntimeVfs::new(local_workspace_vfs(base.path()));
        let present_tool = PresentCanvasTool::new(
            canvas_repo,
            shared_vfs,
            handle,
            session.id.clone(),
            turn_id,
            project_id,
        );

        present_tool
            .execute(
                "tool-present",
                serde_json::json!({ "canvas_id": "demo" }),
                CancellationToken::new(),
                None,
            )
            .await
            .expect("present_canvas 应成功");

        let updated_frame = frame_repo
            .find_by_runtime_session(&session.id)
            .await
            .expect("frame 查询应成功")
            .expect("frame 应存在");
        assert_eq!(
            updated_frame.visible_canvas_mount_ids(),
            vec!["demo".to_string()]
        );

        let state = hub
            .get_current_capability_state(&session.id)
            .await
            .expect("当前 capability state 应存在");
        let active_vfs = state.vfs.active.expect("active VFS 应存在");
        assert!(active_vfs.mounts.iter().any(|mount| mount.id == "cvs-demo"));
        assert!(
            state
                .skill
                .skills
                .iter()
                .any(|skill| skill.name == "canvas-system"
                    && skill.file_path == "cvs-demo://skills/canvas-system/SKILL.md")
        );

        let events = hub
            .eventing_service()
            .list_event_page(&session.id, 0, 100)
            .await
            .expect("events 应能读取")
            .events;
        let capability_index = events
            .iter()
            .position(|event| {
                matches!(
                    &event.notification.event,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value }
                    ) if key == "context_frame"
                        && value.get("kind").and_then(|v| v.as_str()) == Some("capability_state_update")
                )
            })
            .expect("应写入 context_frame(capability_state_update) 事件");
        let presented_index = events
            .iter()
            .position(|event| {
                matches!(
                    &event.notification.event,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. }
                    ) if key == "canvas_presented"
                )
            })
            .expect("应写入 canvas_presented 事件");
        assert!(capability_index < presented_index);
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
            SharedSessionToolServicesHandle::default(),
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
