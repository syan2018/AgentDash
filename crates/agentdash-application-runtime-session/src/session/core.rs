use std::{collections::HashMap, sync::Arc};

use futures::future::join_all;

use super::hub_support::meta_to_execution_state;
use super::persistence::{SessionStoreError, SessionStoreResult, SessionStoreSet};
use super::runtime_registry::SessionRuntimeRegistry;
use super::types::{ExecutionStatus, SessionExecutionState, SessionMeta};

#[derive(Clone)]
pub struct SessionCoreService {
    stores: SessionStoreSet,
    runtime_registry: SessionRuntimeRegistry,
    connector: Arc<dyn agentdash_spi::AgentConnector>,
}

impl SessionCoreService {
    pub(super) fn new(
        stores: SessionStoreSet,
        runtime_registry: SessionRuntimeRegistry,
        connector: Arc<dyn agentdash_spi::AgentConnector>,
    ) -> Self {
        Self {
            stores,
            runtime_registry,
            connector,
        }
    }

    pub async fn recover_interrupted_sessions(&self) -> SessionStoreResult<Vec<SessionMeta>> {
        let sessions = self.stores.meta.list_sessions().await?;
        Ok(sessions
            .into_iter()
            .filter(|meta| meta.last_delivery_status == ExecutionStatus::Running)
            .collect())
    }

    pub async fn create_session(&self, title: &str) -> SessionStoreResult<SessionMeta> {
        self.create_session_with_title_source(title, super::types::TitleSource::Auto)
            .await
    }

    pub async fn create_session_with_title_source(
        &self,
        title: &str,
        title_source: super::types::TitleSource,
    ) -> SessionStoreResult<SessionMeta> {
        let id = format!(
            "sess-{}-{}",
            chrono::Utc::now().timestamp_millis(),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let now = chrono::Utc::now().timestamp_millis();
        let meta = SessionMeta {
            id: id.clone(),
            title: title.to_string(),
            title_source,
            created_at: now,
            updated_at: now,
            last_event_seq: 0,
            last_delivery_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_session_id: None,
        };
        self.stores.meta.create_session(&meta).await?;
        Ok(meta)
    }

    pub async fn list_sessions(&self) -> SessionStoreResult<Vec<SessionMeta>> {
        self.stores.meta.list_sessions().await
    }

    pub async fn get_session_meta(
        &self,
        session_id: &str,
    ) -> SessionStoreResult<Option<SessionMeta>> {
        self.stores.meta.get_session_meta(session_id).await
    }

    pub async fn get_session_metas_bulk(
        &self,
        session_ids: &[String],
    ) -> SessionStoreResult<HashMap<String, SessionMeta>> {
        let futures = session_ids.iter().map(|id| {
            let meta_store = self.stores.meta.clone();
            let id = id.clone();
            async move {
                let meta = meta_store.get_session_meta(&id).await?;
                Ok::<_, SessionStoreError>((id, meta))
            }
        });

        let results = join_all(futures).await;
        let mut map = HashMap::with_capacity(session_ids.len());
        for result in results {
            let (id, maybe_meta) = result?;
            if let Some(meta) = maybe_meta {
                map.insert(id, meta);
            }
        }
        Ok(map)
    }

    pub async fn update_session_meta<F>(
        &self,
        session_id: &str,
        updater: F,
    ) -> SessionStoreResult<Option<SessionMeta>>
    where
        F: FnOnce(&mut SessionMeta),
    {
        let Some(mut meta) = self.stores.meta.get_session_meta(session_id).await? else {
            return Ok(None);
        };
        updater(&mut meta);
        meta.updated_at = chrono::Utc::now().timestamp_millis();
        self.stores.meta.save_session_meta(&meta).await?;
        Ok(Some(meta))
    }

    pub async fn delete_session(&self, session_id: &str) -> SessionStoreResult<()> {
        self.runtime_registry.remove(session_id).await;
        self.stores.meta.delete_session(session_id).await
    }

    pub async fn inspect_execution_states_bulk(
        &self,
        session_ids: &[String],
    ) -> SessionStoreResult<HashMap<String, SessionExecutionState>> {
        let running_set = self.runtime_registry.running_set(session_ids).await;

        let mut result = HashMap::with_capacity(session_ids.len());
        for id in session_ids {
            if running_set.contains(id) {
                result.insert(id.clone(), SessionExecutionState::Running { turn_id: None });
            } else {
                let meta = self
                    .stores
                    .meta
                    .get_session_meta(id)
                    .await?
                    .ok_or_else(|| SessionStoreError::NotFound(format!("session {id} 不存在")))?;
                result.insert(id.clone(), meta_to_execution_state(&meta, id)?);
            }
        }
        Ok(result)
    }

    pub async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> SessionStoreResult<SessionExecutionState> {
        let (running, live_turn_id, cancelling) = self
            .runtime_registry
            .execution_state_snapshot(session_id)
            .await;

        if running {
            if cancelling {
                return Ok(SessionExecutionState::Cancelling {
                    turn_id: live_turn_id,
                });
            }
            return Ok(SessionExecutionState::Running {
                turn_id: live_turn_id,
            });
        }

        let Some(meta) = self.stores.meta.get_session_meta(session_id).await? else {
            return Err(SessionStoreError::NotFound(format!(
                "session {session_id} 不存在"
            )));
        };

        meta_to_execution_state(&meta, session_id)
    }

    pub async fn has_runtime_entry(&self, session_id: &str) -> bool {
        self.runtime_registry.has_runtime_entry(session_id).await
    }

    pub async fn has_active_turn(&self, session_id: &str) -> bool {
        self.runtime_registry.has_active_turn(session_id).await
    }

    pub async fn has_live_executor_session(&self, session_id: &str) -> bool {
        self.connector.has_live_session(session_id).await
    }
}
