use std::path::Path;
use std::sync::Arc;

use agentdash_spi::ConnectorError;
use agentdash_spi::hooks::{
    HookSessionRuntimeAccess, SessionHookRefreshQuery, SessionHookSnapshot,
    SessionHookSnapshotQuery, SharedHookSessionRuntime,
};

use super::hook_runtime::HookSessionRuntime;
use super::hub::{HookTriggerDispatchResult, HookTriggerInput, SessionRuntimeInner};

#[derive(Clone)]
pub struct SessionHookService {
    hub: SessionRuntimeInner,
}

impl SessionHookService {
    pub(super) fn new(hub: SessionRuntimeInner) -> Self {
        Self { hub }
    }

    pub async fn ensure_hook_session_runtime(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        self.hub
            .ensure_hook_session_runtime(session_id, turn_id)
            .await
    }

    pub async fn get_hook_session_runtime(
        &self,
        session_id: &str,
    ) -> Option<SharedHookSessionRuntime> {
        self.hub.get_hook_session_runtime(session_id).await
    }

    pub async fn reload_session_hook_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        executor: &str,
        permission_policy: Option<&str>,
        working_directory: &Path,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        let Some(provider) = self.hub.hook_provider.as_ref() else {
            self.hub
                .runtime_registry
                .with_runtime_mut(session_id, |runtime| {
                    if let Some(runtime) = runtime {
                        runtime.hook_session = None;
                    }
                })
                .await;
            return Ok(None);
        };

        let mut snapshot = provider
            .load_session_snapshot(SessionHookSnapshotQuery {
                session_id: session_id.to_string(),
                turn_id: Some(turn_id.to_string()),
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

        let runtime = Arc::new(HookSessionRuntime::new(
            session_id.to_string(),
            provider.clone(),
            snapshot,
        ));

        self.hub
            .runtime_registry
            .with_runtime_mut(session_id, |session_runtime| {
                if let Some(session_runtime) = session_runtime {
                    session_runtime.hook_session = Some(runtime.clone());
                }
            })
            .await;

        Ok(Some(runtime))
    }

    pub(crate) async fn resolve_hook_session(
        &self,
        session_id: &str,
        turn_id: &str,
        executor_config: &agentdash_domain::common::AgentConfig,
        working_directory: &Path,
        is_owner_bootstrap: bool,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        let existing = self
            .hub
            .runtime_registry
            .hook_session_runtime(session_id)
            .await;

        if is_owner_bootstrap || existing.is_none() {
            return self
                .reload_session_hook_runtime(
                    session_id,
                    turn_id,
                    executor_config.executor.as_str(),
                    executor_config.permission_policy.as_deref(),
                    working_directory,
                )
                .await;
        }

        if let Some(ref hs) = existing {
            let _ = hs
                .refresh(SessionHookRefreshQuery {
                    session_id: session_id.to_string(),
                    turn_id: Some(turn_id.to_string()),
                    reason: Some("subsequent_turn_refresh".to_string()),
                })
                .await;
        }
        Ok(existing)
    }

    pub(crate) async fn emit_session_hook_trigger(
        &self,
        hook_session: &dyn HookSessionRuntimeAccess,
        input: &HookTriggerInput<'_>,
    ) -> HookTriggerDispatchResult {
        self.hub
            .emit_session_hook_trigger(hook_session, input)
            .await
    }
}

fn enrich_hook_snapshot_runtime_metadata(
    snapshot: &mut SessionHookSnapshot,
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
