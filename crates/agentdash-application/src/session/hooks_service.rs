use std::path::Path;
use std::sync::Arc;

use agentdash_spi::ConnectorError;
use agentdash_spi::hooks::{
    AgentFrameHookSnapshot, AgentFrameHookSnapshotQuery, ExecutionHookProvider, HookControlTarget,
    HookRuntimeAccess, HookRuntimeRefreshQuery, RuntimeAdapterProvenance, SharedHookRuntime,
};

use super::hub::{HookTriggerDispatchResult, HookTriggerInput, SessionRuntimeInner};
use super::types::AgentFrameRuntimeTarget;
use crate::workflow::frame_hook_runtime::AgentFrameHookRuntime;

#[derive(Clone)]
pub struct SessionHookService {
    hub: SessionRuntimeInner,
}

impl SessionHookService {
    pub(super) fn new(hub: SessionRuntimeInner) -> Self {
        Self { hub }
    }

    /// 基于 `AgentFrameRuntimeTarget` 确保 hook runtime 就绪并校验 target 一致性。
    pub async fn ensure_hook_runtime_for_target(
        &self,
        target: &AgentFrameRuntimeTarget,
        turn_id: Option<&str>,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        let Some(runtime) = self
            .hub
            .ensure_hook_runtime_for_delivery_session(&target.delivery_runtime_session_id, turn_id)
            .await?
        else {
            return Ok(None);
        };
        validate_hook_runtime_target(runtime.as_ref(), target)?;
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

    pub async fn reload_hook_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        executor: &str,
        permission_policy: Option<&str>,
        working_directory: &Path,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        let Some(provider) = self.hub.hook_provider.as_ref() else {
            self.hub
                .runtime_registry
                .with_runtime_mut(session_id, |runtime| {
                    if let Some(runtime) = runtime {
                        runtime.hook_runtime = None;
                    }
                })
                .await;
            return Ok(None);
        };

        let Some(target) = resolve_runtime_hook_target(provider.as_ref(), session_id).await? else {
            self.hub
                .runtime_registry
                .with_runtime_mut(session_id, |session_runtime| {
                    if let Some(session_runtime) = session_runtime {
                        session_runtime.hook_runtime = None;
                    }
                })
                .await;
            return Ok(None);
        };
        let mut snapshot = provider
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: target.clone(),
                provenance: RuntimeAdapterProvenance::runtime_session(
                    session_id.to_string(),
                    Some(turn_id.to_string()),
                    "hook_runtime_reload",
                ),
            })
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("加载会话 Hook snapshot 失败: {error}"))
            })?;
        enrich_hook_snapshot_runtime_metadata(
            &mut snapshot,
            turn_id,
            self.hub.connector.connector_id(),
            executor,
            permission_policy,
            working_directory,
        );

        let runtime =
            build_frame_hook_runtime(&self.hub, session_id, target, provider.clone(), snapshot)
                .await?;
        let Some(runtime) = runtime else {
            self.hub
                .runtime_registry
                .with_runtime_mut(session_id, |session_runtime| {
                    if let Some(session_runtime) = session_runtime {
                        session_runtime.hook_runtime = None;
                    }
                })
                .await;
            return Ok(None);
        };

        self.hub
            .runtime_registry
            .with_runtime_mut(session_id, |session_runtime| {
                if let Some(session_runtime) = session_runtime {
                    session_runtime.hook_runtime = Some(runtime.clone());
                }
            })
            .await;

        Ok(Some(runtime))
    }

    pub(crate) async fn resolve_hook_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        executor_config: &agentdash_domain::common::AgentConfig,
        working_directory: &Path,
        is_owner_bootstrap: bool,
    ) -> Result<Option<SharedHookRuntime>, ConnectorError> {
        let existing = self.hub.runtime_registry.hook_runtime(session_id).await;

        if is_owner_bootstrap || existing.is_none() {
            return self
                .reload_hook_runtime(
                    session_id,
                    turn_id,
                    executor_config.executor.as_str(),
                    executor_config.permission_policy.as_deref(),
                    working_directory,
                )
                .await;
        }

        if let Some(ref hs) = existing {
            if let Some(provider) = self.hub.hook_provider.as_ref()
                && let Some(current_target) =
                    resolve_runtime_hook_target(provider.as_ref(), session_id).await?
                && hs.control_target() != current_target
            {
                return self
                    .reload_hook_runtime(
                        session_id,
                        turn_id,
                        executor_config.executor.as_str(),
                        executor_config.permission_policy.as_deref(),
                        working_directory,
                    )
                    .await;
            }
            let _ = hs
                .refresh_from_provenance(HookRuntimeRefreshQuery {
                    provenance: RuntimeAdapterProvenance::runtime_session(
                        session_id.to_string(),
                        Some(turn_id.to_string()),
                        "subsequent_turn_refresh",
                    ),
                    reason: Some("subsequent_turn_refresh".to_string()),
                })
                .await;
        }
        Ok(existing)
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
}

fn validate_hook_runtime_target(
    hook_runtime: &dyn HookRuntimeAccess,
    target: &AgentFrameRuntimeTarget,
) -> Result<(), ConnectorError> {
    let control_target = hook_runtime.control_target();
    if hook_runtime.session_id() == target.delivery_runtime_session_id
        && control_target.frame_id == target.frame_id
    {
        return Ok(());
    }

    Err(ConnectorError::Runtime(format!(
        "Hook runtime target mismatch: runtime session `{}` / frame `{}` cannot apply to delivery RuntimeSession `{}` / frame `{}`",
        hook_runtime.session_id(),
        control_target.frame_id,
        target.delivery_runtime_session_id,
        target.frame_id
    )))
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
    provider: Arc<dyn agentdash_spi::hooks::ExecutionHookProvider>,
    snapshot: AgentFrameHookSnapshot,
) -> Result<Option<SharedHookRuntime>, ConnectorError> {
    let Some(frame_repo) = hub.agent_frame_repo.as_ref() else {
        return Err(ConnectorError::Runtime(
            "AgentFrameRepository 未注入，拒绝创建 hook runtime".to_string(),
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

pub(crate) async fn resolve_runtime_hook_target(
    provider: &dyn ExecutionHookProvider,
    session_id: &str,
) -> Result<Option<HookControlTarget>, ConnectorError> {
    provider
        .resolve_runtime_hook_target(session_id)
        .await
        .map_err(|error| {
            ConnectorError::Runtime(format!(
                "解析 RuntimeSession `{session_id}` 的 Hook control target 失败: {error}"
            ))
        })
}
