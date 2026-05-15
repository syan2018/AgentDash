//! `SessionHub` 对外 API 门面。
//!
//! 集中：session CRUD / subscribe / inject / state 查询 / prompt routing /
//! cancel / MCP runtime 热更 / 工具构建 / hook runtime 重建 / companion 回调 /
//! auto-resume 调度 / compaction 事件元数据富化。
//!
//! 后续 PR 6b/6c 会继续按职责拆 `tool_builder` / `hook_dispatch` / `cancel`
//! 独立子模块；本文件是 PR 6a 的过渡形态。

use std::io;

use agentdash_agent_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use tokio::sync::broadcast;

use super::super::compaction_context_frame::build_compaction_context_frame;
use super::super::continuation::build_projected_transcript_from_events;
use super::super::hub_support::*;
use super::super::launch::{LaunchCommand, LaunchPreparation, LaunchStrictness};
use super::super::types::*;
use super::SessionHub;
use crate::companion::build_companion_human_response_notification;
use agentdash_spi::ConnectorError;
use agentdash_spi::hooks::{ContextFrame, SharedHookSessionRuntime};

impl SessionHub {
    /// 启动时调用：将上次进程异常退出时残留的 `running` 状态修正为 `interrupted`。
    ///
    /// 统一通过事件投影驱动状态变更，不直接修改 SessionMeta。
    pub async fn recover_interrupted_sessions(&self) -> std::io::Result<()> {
        let sessions = self.stores.meta.list_sessions().await?;
        for meta in sessions {
            if meta.last_execution_status == ExecutionStatus::Running {
                tracing::warn!(
                    session_id = %meta.id,
                    "启动恢复：session 上次未正常结束，标记为 interrupted"
                );
                let turn_id = meta.last_turn_id.clone().unwrap_or_else(|| {
                    format!("t_recovery_{}", chrono::Utc::now().timestamp_millis())
                });
                let source = SourceInfo {
                    connector_id: "agentdash-server".to_string(),
                    connector_type: "system".to_string(),
                    executor_id: None,
                };
                let notification = build_turn_terminal_envelope(
                    &meta.id,
                    &source,
                    &turn_id,
                    TurnTerminalKind::Interrupted,
                    Some("检测到进程重启，已将上次未完成执行标记为 interrupted".to_string()),
                );
                let _ = self.persist_notification(&meta.id, notification).await?;
            }
        }
        Ok(())
    }

    pub async fn create_session(&self, title: &str) -> std::io::Result<SessionMeta> {
        self.create_session_with_title_source(title, super::super::types::TitleSource::Auto)
            .await
    }

    /// 创建会话并显式指定标题来源。
    /// Task 绑定的会话应使用 `TitleSource::User` 以阻止自动覆盖。
    pub async fn create_session_with_title_source(
        &self,
        title: &str,
        title_source: super::super::types::TitleSource,
    ) -> std::io::Result<SessionMeta> {
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
            last_execution_status: ExecutionStatus::Idle,
            last_turn_id: None,
            last_terminal_message: None,
            executor_config: None,
            executor_session_id: None,
            companion_context: None,
            visible_canvas_mount_ids: Vec::new(),
            bootstrap_state: SessionBootstrapState::Plain,
        };
        self.stores.meta.create_session(&meta).await?;
        Ok(meta)
    }

    pub async fn list_sessions(&self) -> std::io::Result<Vec<SessionMeta>> {
        self.stores.meta.list_sessions().await
    }

    pub async fn get_session_meta(&self, session_id: &str) -> std::io::Result<Option<SessionMeta>> {
        self.stores.meta.get_session_meta(session_id).await
    }

    /// 批量获取多个 session 的 meta，并发读取。
    pub async fn get_session_metas_bulk(
        &self,
        session_ids: &[String],
    ) -> std::io::Result<std::collections::HashMap<String, SessionMeta>> {
        use futures::future::join_all;

        let futures: Vec<_> = session_ids
            .iter()
            .map(|id| {
                let meta_store = self.stores.meta.clone();
                let id = id.clone();
                async move {
                    let meta = meta_store.get_session_meta(&id).await?;
                    Ok::<_, std::io::Error>((id, meta))
                }
            })
            .collect();

        let results = join_all(futures).await;
        let mut map = std::collections::HashMap::with_capacity(session_ids.len());
        for result in results {
            let (id, maybe_meta) = result?;
            if let Some(meta) = maybe_meta {
                map.insert(id, meta);
            }
        }
        Ok(map)
    }

    /// 批量查询 session 执行状态。
    ///
    /// 优先从内存 map 判断是否正在运行（无延迟），
    /// 否则读 meta 的 last_execution_status（持久化的终态）。
    pub async fn inspect_execution_states_bulk(
        &self,
        session_ids: &[String],
    ) -> std::io::Result<std::collections::HashMap<String, SessionExecutionState>> {
        let running_set = self.runtime_registry.running_set(session_ids).await;

        let mut result = std::collections::HashMap::with_capacity(session_ids.len());
        for id in session_ids {
            if running_set.contains(id) {
                result.insert(id.clone(), SessionExecutionState::Running { turn_id: None });
            } else {
                let meta = self
                    .stores
                    .meta
                    .get_session_meta(id)
                    .await?
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, format!("session {id} 不存在"))
                    })?;
                let status = meta_to_execution_state(&meta, id)?;
                result.insert(id.clone(), status);
            }
        }
        Ok(result)
    }

    pub async fn update_session_meta<F>(
        &self,
        session_id: &str,
        updater: F,
    ) -> std::io::Result<Option<SessionMeta>>
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

    /// 查询单个 session 的执行状态。
    pub async fn inspect_session_execution_state(
        &self,
        session_id: &str,
    ) -> std::io::Result<SessionExecutionState> {
        let (running, live_turn_id) = self
            .runtime_registry
            .execution_state_snapshot(session_id)
            .await;

        if running {
            return Ok(SessionExecutionState::Running {
                turn_id: live_turn_id,
            });
        }

        let Some(meta) = self.stores.meta.get_session_meta(session_id).await? else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("session {session_id} 不存在"),
            ));
        };

        meta_to_execution_state(&meta, session_id)
    }

    pub async fn delete_session(&self, session_id: &str) -> std::io::Result<()> {
        self.runtime_registry.remove(session_id).await;
        self.stores.meta.delete_session(session_id).await
    }

    pub async fn ensure_session(
        &self,
        session_id: &str,
    ) -> broadcast::Receiver<super::super::persistence::PersistedSessionEvent> {
        self.runtime_registry.subscribe(session_id).await
    }

    pub async fn get_hook_session_runtime(
        &self,
        session_id: &str,
    ) -> Option<SharedHookSessionRuntime> {
        self.runtime_registry.hook_session_runtime(session_id).await
    }

    /// 向运行中 session 的 agent 注入一条 out-of-band user message。
    ///
    /// 走 connector 的 steering 队列（in-process connector 实现）。
    /// 消息会在下一次 LLM 调用前被合并到对话末尾，对 KV cache 前缀友好。
    pub async fn push_session_notification(
        &self,
        session_id: &str,
        message: String,
    ) -> Result<(), ConnectorError> {
        self.connector
            .push_session_notification(session_id, message)
            .await
    }

    /// 持久化一条结构化能力状态变更事件。
    ///
    /// 这是 UI / 审计 / 回放的事实源；connector steering 消息只是 live agent 的
    /// 尽力投递通道。
    pub(crate) async fn emit_capability_state_changed(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        value: serde_json::Value,
    ) -> io::Result<super::super::persistence::PersistedSessionEvent> {
        let connector_type = match self.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let source = SourceInfo {
            connector_id: self.connector.connector_id().to_string(),
            connector_type: connector_type.to_string(),
            executor_id: None,
        };
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "capability_state_changed".to_string(),
                value,
            }),
            session_id,
            source,
        )
        .with_trace(TraceInfo {
            turn_id: turn_id.map(ToString::to_string),
            entry_index: None,
        });

        self.persist_notification(session_id, envelope).await
    }

    pub(crate) async fn emit_context_frame(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        notice: &ContextFrame,
    ) -> io::Result<super::super::persistence::PersistedSessionEvent> {
        let connector_type = match self.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let source = SourceInfo {
            connector_id: self.connector.connector_id().to_string(),
            connector_type: connector_type.to_string(),
            executor_id: None,
        };
        let value = serde_json::to_value(notice).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("runtime context notice 序列化失败: {error}"),
            )
        })?;
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "context_frame".to_string(),
                value,
            }),
            session_id,
            source,
        )
        .with_trace(TraceInfo {
            turn_id: turn_id.map(ToString::to_string),
            entry_index: None,
        });

        self.persist_notification(session_id, envelope).await
    }

    pub(crate) async fn enqueue_pending_capability_state_transition(
        &self,
        session_id: &str,
        transition: PendingCapabilityStateTransition,
    ) -> io::Result<()> {
        self.stores
            .runtime_commands
            .upsert_pending_runtime_command(session_id, transition)
            .await?;
        Ok(())
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

    pub async fn mark_owner_bootstrap_pending(&self, session_id: &str) -> std::io::Result<()> {
        let _ = self
            .update_session_meta(session_id, |meta| {
                meta.bootstrap_state = SessionBootstrapState::Pending;
            })
            .await?;
        Ok(())
    }

    /// 从持久化事件重建投影 transcript。
    ///
    /// 消费者自选渲染方式：
    /// - `.into_messages()` → 执行器原生 session restore
    /// - `build_continuation_context_frame(&transcript, owner)` → continuation frame 注入
    pub async fn build_projected_transcript(
        &self,
        session_id: &str,
    ) -> std::io::Result<agentdash_agent_types::ProjectedTranscript> {
        let events = self.stores.events.list_all_events(session_id).await?;
        Ok(build_projected_transcript_from_events(&events))
    }

    /// 低层启动入口：跳过 augment，直接进入 prompt pipeline。
    ///
    /// 外部应通过 `launch_command` 或其具名包装启动，
    /// 此方法仅供测试或已预组装的路径调用。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) async fn start_prompt(
        &self,
        session_id: &str,
        req: PreparedLaunchPrompt,
    ) -> Result<String, ConnectorError> {
        self.start_prompt_with_follow_up(session_id, None, req)
            .await
    }

    /// 类型化启动入口（统一门面）。
    ///
    /// 由 [`LaunchCommand`] 决定是否需要 augment、是否 strict、
    /// 以及可选的 follow_up_session_id 透传。
    pub async fn launch_command(
        &self,
        session_id: &str,
        command: LaunchCommand,
    ) -> Result<String, ConnectorError> {
        let follow_up_session_id = command.follow_up_session_id().map(ToString::to_string);
        let reason = command.reason_tag();
        let req = match command.preparation() {
            LaunchPreparation::RequiresAugment => match command.strictness() {
                LaunchStrictness::Strict => {
                    let Some(augmenter) = self.current_prompt_augmenter().await else {
                        return Err(ConnectorError::Runtime(format!(
                            "prompt_augmenter 未注入，拒绝 strict launch: {reason}"
                        )));
                    };
                    augmenter
                        .augment(session_id, command.into_augment_input())
                        .await?
                }
                LaunchStrictness::Relaxed => {
                    self.augment_prompt_request(session_id, command.into_augment_input(), reason)
                        .await?
                }
            },
            LaunchPreparation::PreAssembled => command.into_prepared_prompt(),
        };
        self.start_prompt_with_follow_up(session_id, follow_up_session_id.as_deref(), req)
            .await
    }

    /// 将内部 follow-up 的裸请求补齐到与 HTTP 主通道一致。
    ///
    /// 没有注入 augmenter 的测试/嵌入场景保留旧行为，但正式 AppState
    /// 应始终注入 augmenter，避免 owner / MCP / flow 上下文漂移。
    pub(crate) async fn augment_prompt_request(
        &self,
        session_id: &str,
        input: super::super::augmenter::PromptAugmentInput,
        reason: &str,
    ) -> Result<PreparedLaunchPrompt, ConnectorError> {
        match self.current_prompt_augmenter().await {
            Some(augmenter) => augmenter.augment(session_id, input).await,
            None => {
                tracing::warn!(
                    session_id = %session_id,
                    reason = %reason,
                    "prompt_augmenter 未注入，内部 follow-up 将使用裸请求"
                );
                Ok(input.into_prepared_prompt())
            }
        }
    }

    pub async fn subscribe_with_history(
        &self,
        session_id: &str,
    ) -> io::Result<SessionEventSubscription> {
        self.subscribe_after(session_id, 0).await
    }

    pub async fn subscribe_after(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> io::Result<SessionEventSubscription> {
        let rx = self.ensure_session(session_id).await;
        let backlog = self
            .stores
            .events
            .read_backlog(session_id, after_seq)
            .await?;
        Ok(SessionEventSubscription {
            snapshot_seq: backlog.snapshot_seq,
            backlog: backlog.events,
            rx,
        })
    }

    pub async fn list_event_page(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: u32,
    ) -> io::Result<super::super::persistence::SessionEventPage> {
        self.stores
            .events
            .list_event_page(session_id, after_seq, limit)
            .await
    }

    /// 向指定 session 主动注入补充通知（bridge 事件 / companion / canvas 等）。
    /// 直接 persist + broadcast，不经过 turn processor。
    pub async fn inject_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> std::io::Result<()> {
        let _ = self.persist_notification(session_id, envelope).await?;
        Ok(())
    }

    pub(crate) async fn persist_notification(
        &self,
        session_id: &str,
        envelope: BackboneEnvelope,
    ) -> io::Result<super::super::persistence::PersistedSessionEvent> {
        let envelope = self
            .maybe_enrich_compaction_notification(session_id, envelope)
            .await?;
        let persisted = self
            .stores
            .events
            .append_event(session_id, &envelope)
            .await?;
        let tx = self.runtime_registry.touch_and_sender(session_id).await;
        let _ = tx.send(persisted.clone());
        if let BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value }) =
            &persisted.notification.event
            && key == "context_compacted"
            && let Some(frame) = build_compaction_context_frame(value)
        {
            let _ = self
                .persist_context_frame_direct(session_id, persisted.turn_id.as_deref(), &frame)
                .await;
        }
        Ok(persisted)
    }

    async fn persist_context_frame_direct(
        &self,
        session_id: &str,
        turn_id: Option<&str>,
        frame: &ContextFrame,
    ) -> io::Result<super::super::persistence::PersistedSessionEvent> {
        let connector_type = match self.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let source = SourceInfo {
            connector_id: self.connector.connector_id().to_string(),
            connector_type: connector_type.to_string(),
            executor_id: None,
        };
        let value = serde_json::to_value(frame).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("context frame 序列化失败: {error}"),
            )
        })?;
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "context_frame".to_string(),
                value,
            }),
            session_id,
            source,
        )
        .with_trace(TraceInfo {
            turn_id: turn_id.map(ToString::to_string),
            entry_index: None,
        });

        let persisted = self
            .stores
            .events
            .append_event(session_id, &envelope)
            .await?;
        let tx = self.runtime_registry.touch_and_sender(session_id).await;
        let _ = tx.send(persisted.clone());
        Ok(persisted)
    }

    /// 查找所有超过指定超时时间无活动的 running session，返回其 session_id 列表。
    pub async fn find_stalled_sessions(&self, stall_timeout_ms: u64) -> Vec<String> {
        self.turn_supervisor
            .find_stalled_sessions(stall_timeout_ms)
            .await
    }

    pub async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<(), ConnectorError> {
        self.connector
            .approve_tool_call(session_id, tool_call_id)
            .await
    }

    pub async fn reject_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConnectorError> {
        self.connector
            .reject_tool_call(session_id, tool_call_id, reason)
            .await
    }

    /// 人通过 API 回应 companion 请求。
    /// 若命中 wait registry，则恢复挂起的工具调用；
    /// 无论是否命中，都把回应写入 session 事件流，保证历史可回放。
    pub async fn respond_companion_request(
        &self,
        session_id: &str,
        request_id: &str,
        payload: serde_json::Value,
    ) -> Result<(), ConnectorError> {
        let resolved = self
            .companion_wait_registry
            .resolve(session_id, request_id, payload.clone())
            .await;

        let fallback_turn_id = self
            .persistence
            .get_session_meta(session_id)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?
            .and_then(|meta| meta.last_turn_id);
        let turn_id = resolved
            .as_ref()
            .map(|result| result.turn_id.as_str())
            .or(fallback_turn_id.as_deref());

        let request_type = resolved
            .as_ref()
            .and_then(|result| result.request_type.as_deref());

        let notification = build_companion_human_response_notification(
            session_id,
            turn_id,
            request_id,
            &payload,
            request_type,
            resolved.is_some(),
        );
        let _ = self.inject_notification(session_id, notification).await;

        Ok(())
    }
}
