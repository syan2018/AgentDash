use std::path::Path;
use std::sync::Arc;

use agentdash_application_ports::runtime_surface_adoption::{
    AgentFrameHookRuntimeTarget, AgentFrameRuntimeTarget,
};
use agentdash_domain::workflow::AgentFrame;
use agentdash_spi::ConnectorError;
use agentdash_spi::hooks::{
    AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, HookControlTarget, HookRuntimeAccess,
    RuntimeAdapterProvenance, SharedHookRuntime,
};

use super::hub::{HookTriggerDispatchResult, HookTriggerInput, SessionRuntimeInner};
use crate::agent_run::AgentRunAcceptedLaunchHookRuntimeSync;
use crate::agent_run::frame::hook_runtime::AgentFrameHookRuntime;

#[derive(Clone)]
pub struct SessionHookService {
    hub: SessionRuntimeInner,
}

impl SessionHookService {
    pub(super) fn new(hub: SessionRuntimeInner) -> Self {
        Self { hub }
    }

    /// 基于 `AgentFrameHookRuntimeTarget` 确保 hook runtime 就绪并校验 target 一致性。
    pub async fn ensure_hook_runtime_for_hook_target(
        &self,
        target: &AgentFrameHookRuntimeTarget,
        turn_id: Option<&str>,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        let frame = self
            .resolve_hook_target_frame_for_control_target(target)
            .await?;
        self.ensure_resolved_hook_runtime(
            target,
            Some(&frame),
            turn_id,
            "hook_runtime_target_rebuild",
        )
        .await
    }

    /// Adapter: 从 frame/session binding 解析 hook control target 后进入 hook target-first 路径。
    pub async fn ensure_hook_runtime_for_target(
        &self,
        target: &AgentFrameRuntimeTarget,
        turn_id: Option<&str>,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        let resolved_target = resolve_hook_target_frame(&self.hub, target, None).await?;
        self.ensure_resolved_hook_runtime(
            &resolved_target.hook_target,
            Some(&resolved_target.frame),
            turn_id,
            "hook_runtime_target_rebuild",
        )
        .await
    }

    async fn ensure_resolved_hook_runtime(
        &self,
        target: &AgentFrameHookRuntimeTarget,
        frame: Option<&AgentFrame>,
        turn_id: Option<&str>,
        provenance_source: &'static str,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        if self
            .hub
            .persistence
            .get_session_meta(&target.delivery_runtime_session_id)
            .await
            .map_err(std::io::Error::from)?
            .is_none()
        {
            return Ok(None);
        }

        let Some(provider) = self.hub.hook_provider.as_ref() else {
            self.hub
                .runtime_registry
                .with_runtime_mut(&target.delivery_runtime_session_id, |session_runtime| {
                    if let Some(session_runtime) = session_runtime {
                        session_runtime.hook_runtime_target_cache = None;
                    }
                })
                .await;
            return Ok(None);
        };

        if let Some(runtime) = self
            .hub
            .runtime_registry
            .hook_runtime_target_cache(&target.delivery_runtime_session_id)
            .await
            && validate_hook_runtime_target(runtime.as_ref(), target).is_ok()
        {
            // AgentFrame runtime surface is immutable by frame id; a matching cached runtime
            // already represents the requested target. New frame ids take the rebuild path.
            return Ok(Some(runtime));
        }

        let frame = match frame {
            Some(frame) => frame.clone(),
            None => {
                self.resolve_hook_target_frame_for_control_target(target)
                    .await?
            }
        };
        let snapshot = provider
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: target.control_target.clone(),
                provenance: RuntimeAdapterProvenance::runtime_session(
                    target.delivery_runtime_session_id.clone(),
                    turn_id.map(ToString::to_string),
                    provenance_source,
                ),
            })
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("重建 target Hook snapshot 失败: {error}"))
            })?;

        let Some(rebuilt_runtime) = build_frame_hook_runtime(
            &self.hub,
            &target.delivery_runtime_session_id,
            target.control_target.clone(),
            Some(&frame),
            provider.clone(),
            snapshot,
        )
        .await?
        else {
            return Ok(None);
        };

        let runtime = self
            .hub
            .runtime_registry
            .set_or_replace_hook_runtime_target_cache(
                &target.delivery_runtime_session_id,
                rebuilt_runtime,
            )
            .await;
        Ok(Some(runtime))
    }

    /// 基于 `AgentFrameRuntimeTarget` 获取 hook runtime。
    ///
    /// 语义与 `ensure_hook_runtime_for_target` 收敛：delivery-session 缓存只是
    /// adapter binding，命中 stale target 时由 ensure 路径重建后再返回。
    pub async fn get_hook_runtime_for_target(
        &self,
        target: &AgentFrameRuntimeTarget,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        self.ensure_hook_runtime_for_target(target, None).await
    }

    pub async fn get_hook_runtime_for_hook_target(
        &self,
        target: &AgentFrameHookRuntimeTarget,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        self.ensure_hook_runtime_for_hook_target(target, None).await
    }

    pub(crate) async fn resolve_hook_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        expected_frame_id: uuid::Uuid,
        pending_frame: Option<&AgentFrame>,
        executor_config: &agentdash_domain::common::AgentConfig,
        working_directory: &Path,
        is_owner_bootstrap: bool,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        let expected_target = AgentFrameRuntimeTarget {
            frame_id: expected_frame_id,
            delivery_runtime_session_id: session_id.to_string(),
        };
        let resolved_target =
            resolve_hook_target_frame(&self.hub, &expected_target, pending_frame).await?;
        let existing = self
            .hub
            .runtime_registry
            .hook_runtime_target_cache(session_id)
            .await;

        if is_owner_bootstrap || existing.is_none() {
            return self
                .reload_hook_runtime_for_frame(
                    &resolved_target.hook_target,
                    &resolved_target.frame,
                    turn_id,
                    executor_config.executor.as_str(),
                    executor_config.permission_policy.as_deref(),
                    working_directory,
                    "hook_runtime_launch_target_reload",
                )
                .await;
        }

        if let Some(ref hs) = existing {
            if validate_hook_runtime_target(hs.as_ref(), &resolved_target.hook_target).is_err() {
                return self
                    .reload_hook_runtime_for_frame(
                        &resolved_target.hook_target,
                        &resolved_target.frame,
                        turn_id,
                        executor_config.executor.as_str(),
                        executor_config.permission_policy.as_deref(),
                        working_directory,
                        "hook_runtime_launch_target_rebuild",
                    )
                    .await;
            }
            // The cached runtime is already bound to the immutable AgentFrame target.
            // Rebuilding is reserved for frame-id changes; same-target turns reuse it.
        }
        Ok(existing)
    }

    async fn reload_hook_runtime_for_frame(
        &self,
        target: &AgentFrameHookRuntimeTarget,
        frame: &AgentFrame,
        turn_id: &str,
        executor: &str,
        permission_policy: Option<&str>,
        working_directory: &Path,
        provenance_source: &'static str,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        let Some(provider) = self.hub.hook_provider.as_ref() else {
            self.hub
                .runtime_registry
                .with_runtime_mut(&target.delivery_runtime_session_id, |session_runtime| {
                    if let Some(session_runtime) = session_runtime {
                        session_runtime.hook_runtime_target_cache = None;
                    }
                })
                .await;
            return Ok(None);
        };
        let mut snapshot = provider
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: target.control_target.clone(),
                provenance: RuntimeAdapterProvenance::runtime_session(
                    target.delivery_runtime_session_id.clone(),
                    Some(turn_id.to_string()),
                    provenance_source,
                ),
            })
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("加载 target Hook snapshot 失败: {error}"))
            })?;
        enrich_hook_snapshot_runtime_metadata(
            &mut snapshot,
            turn_id,
            self.hub.connector.connector_id(),
            executor,
            permission_policy,
            working_directory,
        );

        let Some(runtime) = build_frame_hook_runtime(
            &self.hub,
            &target.delivery_runtime_session_id,
            target.control_target.clone(),
            Some(frame),
            provider.clone(),
            snapshot,
        )
        .await?
        else {
            self.hub
                .runtime_registry
                .with_runtime_mut(&target.delivery_runtime_session_id, |session_runtime| {
                    if let Some(session_runtime) = session_runtime {
                        session_runtime.hook_runtime_target_cache = None;
                    }
                })
                .await;
            return Ok(None);
        };

        Ok(Some(
            self.hub
                .runtime_registry
                .set_or_replace_hook_runtime_target_cache(
                    &target.delivery_runtime_session_id,
                    runtime,
                )
                .await,
        ))
    }

    pub(crate) async fn emit_session_hook_trigger(
        &self,
        hook_runtime: &dyn HookRuntimeAccess,
        input: &HookTriggerInput<'_>,
    ) -> HookTriggerDispatchResult {
        self.hub
            .emit_session_hook_trigger(hook_runtime, input)
            .await
    }

    async fn resolve_hook_target_frame_for_control_target(
        &self,
        target: &AgentFrameHookRuntimeTarget,
    ) -> Result<AgentFrame, ConnectorError> {
        let Some(frame_repo) = self.hub.agent_frame_repo.as_ref() else {
            return Err(ConnectorError::Runtime(
                "AgentFrameRepository 未注入，无法按 AgentFrameHookRuntimeTarget 查询 AgentFrame"
                    .to_string(),
            ));
        };
        let frame = frame_repo
            .get(target.frame_id())
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("查询 Hook target 对应 AgentFrame 失败: {error}"))
            })?
            .ok_or_else(|| {
                ConnectorError::Runtime(format!("Hook target frame `{}` 不存在", target.frame_id()))
            })?;
        if frame.agent_id != target.control_target.agent_id {
            return Err(ConnectorError::Runtime(format!(
                "Hook target frame `{}` belongs to agent `{}`, not `{}`",
                frame.id, frame.agent_id, target.control_target.agent_id
            )));
        }
        Ok(frame)
    }
}

#[async_trait::async_trait]
impl AgentRunAcceptedLaunchHookRuntimeSync for SessionHookService {
    async fn sync_accepted_launch_hook_runtime(
        &self,
        target: AgentFrameRuntimeTarget,
        turn_id: &str,
    ) -> Result<(), ConnectorError> {
        self.ensure_hook_runtime_for_target(&target, Some(turn_id))
            .await
            .map(|_| ())
    }
}

fn validate_hook_runtime_target(
    hook_runtime: &dyn HookRuntimeAccess,
    target: &AgentFrameHookRuntimeTarget,
) -> Result<(), ConnectorError> {
    let control_target = hook_runtime.control_target();
    if hook_runtime.session_id() == target.delivery_runtime_session_id
        && control_target == target.control_target
    {
        return Ok(());
    }

    Err(ConnectorError::Runtime(format!(
        "Hook runtime target mismatch: runtime session `{}` / frame `{}` cannot apply to delivery RuntimeSession `{}` / frame `{}`",
        hook_runtime.session_id(),
        control_target.frame_id,
        target.delivery_runtime_session_id,
        target.frame_id()
    )))
}

struct ResolvedHookTargetFrame {
    hook_target: AgentFrameHookRuntimeTarget,
    frame: AgentFrame,
}

async fn resolve_hook_target_frame(
    hub: &SessionRuntimeInner,
    target: &AgentFrameRuntimeTarget,
    pending_frame: Option<&AgentFrame>,
) -> Result<ResolvedHookTargetFrame, ConnectorError> {
    if let Some(frame) = pending_frame {
        if frame.id != target.frame_id {
            return Err(ConnectorError::Runtime(format!(
                "Hook launch target frame `{}` 与 pending AgentFrame `{}` 不一致",
                target.frame_id, frame.id
            )));
        }
        return resolve_hook_target_frame_from_frame(hub, target, frame).await;
    }
    resolve_persisted_hook_target_frame(hub, target).await
}

async fn resolve_persisted_hook_target_frame(
    hub: &SessionRuntimeInner,
    target: &AgentFrameRuntimeTarget,
) -> Result<ResolvedHookTargetFrame, ConnectorError> {
    let Some(frame_repo) = hub.agent_frame_repo.as_ref() else {
        return Err(ConnectorError::Runtime(
            "AgentFrameRepository 未注入，无法按 AgentFrameRuntimeTarget 构造 HookControlTarget"
                .to_string(),
        ));
    };
    let Some(frame) = frame_repo.get(target.frame_id).await.map_err(|error| {
        ConnectorError::Runtime(format!("查询 Hook target 对应 AgentFrame 失败: {error}"))
    })?
    else {
        return Err(ConnectorError::Runtime(format!(
            "Hook target frame `{}` 不存在",
            target.frame_id
        )));
    };
    resolve_hook_target_frame_from_frame(hub, target, &frame).await
}

async fn resolve_hook_target_frame_from_frame(
    hub: &SessionRuntimeInner,
    target: &AgentFrameRuntimeTarget,
    frame: &AgentFrame,
) -> Result<ResolvedHookTargetFrame, ConnectorError> {
    if frame.id != target.frame_id {
        return Err(ConnectorError::Runtime(format!(
            "Hook target frame `{}` 与 AgentFrame `{}` 不一致",
            target.frame_id, frame.id
        )));
    }
    let Some(anchor_repo) = hub.execution_anchor_repo.as_ref() else {
        return Err(ConnectorError::Runtime(
            "RuntimeSessionExecutionAnchorRepository 未注入，无法按 AgentFrameRuntimeTarget 构造 HookControlTarget"
                .to_string(),
        ));
    };
    let Some(anchor) = anchor_repo
        .find_by_session(&target.delivery_runtime_session_id)
        .await
        .map_err(|error| {
            ConnectorError::Runtime(format!(
                "查询 delivery RuntimeSession `{}` anchor 失败: {error}",
                target.delivery_runtime_session_id
            ))
        })?
    else {
        return Err(ConnectorError::Runtime(format!(
            "delivery RuntimeSession `{}` 缺少 RuntimeSessionExecutionAnchor",
            target.delivery_runtime_session_id
        )));
    };
    if anchor.agent_id != frame.agent_id {
        return Err(ConnectorError::Runtime(format!(
            "Hook target frame `{}` belongs to agent `{}`, not delivery RuntimeSession `{}` agent `{}`",
            frame.id, frame.agent_id, target.delivery_runtime_session_id, anchor.agent_id
        )));
    }
    Ok(ResolvedHookTargetFrame {
        hook_target: AgentFrameHookRuntimeTarget::new(
            HookControlTarget {
                run_id: anchor.run_id,
                agent_id: frame.agent_id,
                frame_id: frame.id,
            },
            target.delivery_runtime_session_id.clone(),
        ),
        frame: frame.clone(),
    })
}

fn enrich_hook_snapshot_runtime_metadata(
    snapshot: &mut AgentFrameHookSnapshot,
    turn_id: &str,
    connector_id: &str,
    executor: &str,
    permission_policy: Option<&str>,
    working_directory: &Path,
) {
    let metadata = snapshot
        .metadata
        .get_or_insert_with(agentdash_spi::SessionSnapshotMetadata::default);
    metadata.turn_id = Some(turn_id.to_string());
    metadata.connector_id = Some(connector_id.to_string());
    metadata.executor = Some(executor.to_string());
    metadata.permission_policy = permission_policy.map(ToString::to_string);
    metadata.working_directory = Some(working_directory.to_string_lossy().replace('\\', "/"));
}

pub(crate) async fn build_frame_hook_runtime(
    hub: &SessionRuntimeInner,
    session_id: &str,
    target: HookControlTarget,
    frame: Option<&AgentFrame>,
    provider: Arc<dyn agentdash_spi::hooks::ExecutionHookProvider>,
    snapshot: AgentFrameHookSnapshot,
) -> Result<Option<SharedHookRuntime>, ConnectorError> {
    let frame = if let Some(frame) = frame {
        frame.clone()
    } else {
        let Some(frame_repo) = hub.agent_frame_repo.as_ref() else {
            return Err(ConnectorError::Runtime(
                "AgentFrameRepository 未注入，拒绝创建 hook runtime".to_string(),
            ));
        };
        frame_repo
            .get(target.frame_id)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("查询 Hook target 对应 AgentFrame 失败: {error}"))
            })?
            .ok_or_else(|| {
                ConnectorError::Runtime(format!("Hook target frame `{}` 不存在", target.frame_id))
            })?
    };
    if frame.agent_id != target.agent_id {
        return Err(ConnectorError::Runtime(format!(
            "Hook target frame `{}` belongs to agent `{}`, not `{}`",
            frame.id, frame.agent_id, target.agent_id
        )));
    }
    let Some(anchor_repo) = hub.execution_anchor_repo.as_ref() else {
        return Err(ConnectorError::Runtime(
            "RuntimeSessionExecutionAnchorRepository 未注入，拒绝创建 hook runtime".to_string(),
        ));
    };
    let anchor = anchor_repo
        .find_by_session(session_id)
        .await
        .map_err(|error| {
            ConnectorError::Runtime(format!(
                "查询 Hook target 对应 RuntimeSessionExecutionAnchor 失败: {error}"
            ))
        })?;
    if anchor.is_none_or(|anchor| anchor.agent_id != target.agent_id) {
        return Err(ConnectorError::Runtime(format!(
            "Hook target agent `{}` does not own delivery RuntimeSession `{session_id}`",
            target.agent_id
        )));
    }
    Ok(Some(Arc::new(AgentFrameHookRuntime::from_frame(
        target.run_id,
        &frame,
        session_id.to_string(),
        provider,
        snapshot,
    ))))
}
