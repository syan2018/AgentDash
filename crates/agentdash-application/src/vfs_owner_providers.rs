use std::sync::Arc;

use agentdash_application_agentrun::agent_run::{
    AgentRunJournalQuery, AgentRunJournalService, DeliveryRuntimeSelectionPolicy,
    DeliveryRuntimeSelectionRepositories, DeliveryRuntimeSelectionService,
    agent_run_journal_session_id, project_event_to_agent_run_journal,
};
use agentdash_application_lifecycle::lifecycle::surface::journey::{
    AgentRunJournalProjection, AgentRunJournalReader, AgentRunJournalRef, JourneyResult,
    LifecycleJourneyError, SessionToolResultCacheReadResult, SessionToolResultCacheReader,
};
use agentdash_domain::canvas::CanvasRepository;
use agentdash_domain::inline_file::InlineFileRepository;
use agentdash_domain::routine::RoutineExecutionRepository;
use agentdash_domain::skill_asset::SkillAssetRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, AgentRunDeliveryBindingRepository, LifecycleAgentRepository,
    LifecycleRunRepository, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::session_persistence::{
    SessionCompactionStore, SessionEventStore, SessionLineageStore, SessionMetaStore,
};
use async_trait::async_trait;

use crate::canvas::CanvasFsMountProvider;
use crate::lifecycle::{
    LifecycleMountProvider, SessionToolResultCacheStatus as LifecycleToolResultCacheStatus,
    SessionToolResultCacheStatusKind as LifecycleToolResultCacheStatusKind,
};
use crate::session::{
    SessionToolResultCache, SessionToolResultCacheRead as RuntimeToolResultCacheRead,
    SessionToolResultCacheStatusKind as RuntimeToolResultCacheStatusKind,
};
use crate::vfs::provider::MountProviderRegistryBuilder;
use crate::vfs::{InlineFsMountProvider, RoutineMountProvider, SkillAssetFsMountProvider};

pub trait MountProviderRegistryBuilderOwnerExt {
    #[allow(clippy::too_many_arguments)]
    fn with_application_builtins(
        self,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_meta_store: Arc<dyn SessionMetaStore>,
        session_event_store: Arc<dyn SessionEventStore>,
        session_lineage_store: Arc<dyn SessionLineageStore>,
        session_compaction_store: Arc<dyn SessionCompactionStore>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        agent_run_delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
        tool_result_cache: Arc<SessionToolResultCache>,
    ) -> Self;
}

impl MountProviderRegistryBuilderOwnerExt for MountProviderRegistryBuilder {
    #[allow(clippy::too_many_arguments)]
    fn with_application_builtins(
        self,
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        canvas_repo: Arc<dyn CanvasRepository>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
        skill_asset_repo: Arc<dyn SkillAssetRepository>,
        session_meta_store: Arc<dyn SessionMetaStore>,
        session_event_store: Arc<dyn SessionEventStore>,
        session_lineage_store: Arc<dyn SessionLineageStore>,
        session_compaction_store: Arc<dyn SessionCompactionStore>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        agent_run_delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
        tool_result_cache: Arc<SessionToolResultCache>,
    ) -> Self {
        let agent_run_journal_reader = Arc::new(ApplicationAgentRunJournalReader::new(
            lifecycle_run_repo.clone(),
            lifecycle_agent_repo,
            agent_frame_repo,
            execution_anchor_repo,
            agent_run_delivery_binding_repo,
            session_lineage_store,
            session_event_store.clone(),
        ));
        self.register(Arc::new(InlineFsMountProvider::new(
            inline_file_repo.clone(),
        )))
        .register(Arc::new(
            LifecycleMountProvider::new_with_tool_result_cache(
                lifecycle_run_repo,
                inline_file_repo.clone(),
                skill_asset_repo.clone(),
                session_meta_store,
                session_compaction_store,
                Arc::new(RuntimeSessionToolResultCacheReader { tool_result_cache }),
                agent_run_journal_reader,
            ),
        ))
        .register(Arc::new(RoutineMountProvider::new(
            routine_execution_repo,
            inline_file_repo.clone(),
        )))
        .register(Arc::new(CanvasFsMountProvider::new(canvas_repo)))
        .register(Arc::new(SkillAssetFsMountProvider::new(skill_asset_repo)))
    }
}

struct ApplicationAgentRunJournalReader {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
    agent_frame_repo: Arc<dyn AgentFrameRepository>,
    execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
    delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
    service: AgentRunJournalService,
}

impl ApplicationAgentRunJournalReader {
    fn new(
        lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
        lifecycle_agent_repo: Arc<dyn LifecycleAgentRepository>,
        agent_frame_repo: Arc<dyn AgentFrameRepository>,
        execution_anchor_repo: Arc<dyn RuntimeSessionExecutionAnchorRepository>,
        delivery_binding_repo: Arc<dyn AgentRunDeliveryBindingRepository>,
        lineage_store: Arc<dyn SessionLineageStore>,
        event_store: Arc<dyn SessionEventStore>,
    ) -> Self {
        Self {
            lifecycle_run_repo,
            lifecycle_agent_repo,
            agent_frame_repo,
            execution_anchor_repo,
            delivery_binding_repo,
            service: AgentRunJournalService::new_from_session_stores(lineage_store, event_store),
        }
    }
}

#[async_trait]
impl AgentRunJournalReader for ApplicationAgentRunJournalReader {
    async fn visible_journal(
        &self,
        reference: AgentRunJournalRef,
    ) -> JourneyResult<AgentRunJournalProjection> {
        let delivery = DeliveryRuntimeSelectionService::new(DeliveryRuntimeSelectionRepositories {
            lifecycle_runs: self.lifecycle_run_repo.as_ref(),
            lifecycle_agents: self.lifecycle_agent_repo.as_ref(),
            agent_frames: self.agent_frame_repo.as_ref(),
            execution_anchors: self.execution_anchor_repo.as_ref(),
            delivery_bindings: self.delivery_binding_repo.as_ref(),
        })
        .select(DeliveryRuntimeSelectionPolicy::CurrentDelivery {
            run_id: reference.run_id,
            agent_id: reference.agent_id,
        })
        .await
        .map_err(|error| {
            LifecycleJourneyError::OperationFailed(format!(
                "解析 AgentRun 当前 delivery 失败: {error}"
            ))
        })?;
        let journal = self
            .service
            .load_visible_journal(AgentRunJournalQuery {
                run_id: reference.run_id,
                agent_id: reference.agent_id,
                delivery_runtime_session_id: Some(delivery.runtime_session_id.clone()),
            })
            .await
            .map_err(|error| {
                LifecycleJourneyError::OperationFailed(format!(
                    "读取 AgentRun journal 失败: {error}"
                ))
            })?;
        let journal_session_id = agent_run_journal_session_id(journal.run_id, journal.agent_id);
        let events = journal
            .events
            .into_iter()
            .map(|event| {
                project_event_to_agent_run_journal(
                    event.event,
                    event.journal_seq,
                    &journal_session_id,
                )
            })
            .collect();
        Ok(AgentRunJournalProjection {
            delivery_runtime_session_id: delivery.runtime_session_id,
            events,
        })
    }
}

struct RuntimeSessionToolResultCacheReader {
    tool_result_cache: Arc<SessionToolResultCache>,
}

impl SessionToolResultCacheReader for RuntimeSessionToolResultCacheReader {
    fn read_text(&self, session_id: &str, item_id: &str) -> SessionToolResultCacheReadResult {
        match self.tool_result_cache.read_text(session_id, item_id) {
            RuntimeToolResultCacheRead::Available { metadata, text } => {
                SessionToolResultCacheReadResult::Available {
                    text,
                    stored_bytes: metadata.stored_bytes,
                    original_bytes: metadata.original_bytes,
                }
            }
            RuntimeToolResultCacheRead::Unavailable(status) => {
                SessionToolResultCacheReadResult::Unavailable(LifecycleToolResultCacheStatus {
                    status: map_tool_result_cache_status(status.status),
                    session_id: status.session_id,
                    item_id: status.item_id,
                    lifecycle_path: status.lifecycle_path,
                    message: status.message,
                })
            }
        }
    }
}

fn map_tool_result_cache_status(
    status: RuntimeToolResultCacheStatusKind,
) -> LifecycleToolResultCacheStatusKind {
    match status {
        RuntimeToolResultCacheStatusKind::Missing => LifecycleToolResultCacheStatusKind::Missing,
        RuntimeToolResultCacheStatusKind::Expired => LifecycleToolResultCacheStatusKind::Expired,
    }
}
