//! postgres session 持久化的共享逻辑。
//!
//! 这里集中行解析（`*_from_row`）、值编解码辅助、业务不变量校验
//! （`validate_commit_session`）以及从 `BackboneEnvelope` 推导 session 投影的纯逻辑。

use agentdash_agent_protocol::{
    AgentDashThreadItem, BackboneEnvelope, BackboneEvent, PlatformEvent,
};
use agentdash_spi::session_persistence::{
    AgentFrameTransitionRecord, ExecutionStatus, NewCompactionProjectionCommit,
    PersistedSessionEvent, RuntimeCapabilityTransition, RuntimeCommandRecord, RuntimeCommandStatus,
    RuntimeDeliveryCommand, SessionCompactionRecord, SessionCompactionStatus, SessionLineageRecord,
    SessionLineageRelationKind, SessionLineageStatus, SessionMeta, SessionProjectionHeadRecord,
    SessionProjectionSegmentRecord, SessionStoreError, SessionStoreResult, TerminalEffectRecord,
    TerminalEffectStatus, TerminalEffectType, TitleSource,
};
use sqlx::Row;

/// `synthetic` 列在 postgres 存为 BOOLEAN，由 row 类型实现该 trait 让
/// `projection_segment_from_row` 保持泛型解析。
pub(crate) trait SessionRow {
    fn synthetic_flag(&self) -> bool;
}

impl SessionRow for sqlx::postgres::PgRow {
    fn synthetic_flag(&self) -> bool {
        self.get::<bool, _>("synthetic")
    }
}

pub(crate) fn map_meta_row<R>(row: &R) -> SessionStoreResult<SessionMeta>
where
    R: Row,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> i64: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    Ok(SessionMeta {
        id: row.get::<String, _>("id"),
        title: row.get::<String, _>("title"),
        title_source: parse_title_source(
            row.get::<String, _>("title_source"),
            "sessions.title_source",
        )?,
        created_at: row.get::<i64, _>("created_at"),
        updated_at: row.get::<i64, _>("updated_at"),
        last_event_seq: parse_non_negative_u64(
            row.get::<i64, _>("last_event_seq"),
            "sessions.last_event_seq",
        )?,
        last_delivery_status: parse_execution_status(
            row.get::<String, _>("last_delivery_status"),
            "sessions.last_delivery_status",
        )?,
        last_turn_id: row.get::<Option<String>, _>("last_turn_id"),
        last_terminal_message: row.get::<Option<String>, _>("last_terminal_message"),
        executor_session_id: row.get::<Option<String>, _>("executor_session_id"),
    })
}

pub(crate) fn persisted_event_from_row<R>(row: &R) -> SessionStoreResult<PersistedSessionEvent>
where
    R: Row,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> i64: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<i64>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    let notification_json = row.get::<String, _>("notification_json");
    let notification = serde_json::from_str::<BackboneEnvelope>(&notification_json)
        .map_err(|error| SessionStoreError::InvalidData(error.to_string()))?;
    let event_seq_i64 = row.get::<i64, _>("event_seq");
    let event_seq = parse_non_negative_u64(event_seq_i64, "session_events.event_seq")?;
    let entry_index = row
        .get::<Option<i64>, _>("entry_index")
        .map(|value| parse_non_negative_u32(value, "session_events.entry_index"))
        .transpose()?;
    Ok(PersistedSessionEvent {
        session_id: row.get::<String, _>("session_id"),
        event_seq,
        occurred_at_ms: row.get::<i64, _>("occurred_at_ms"),
        committed_at_ms: row.get::<i64, _>("committed_at_ms"),
        session_update_type: row.get::<String, _>("session_update_type"),
        turn_id: row.get::<Option<String>, _>("turn_id"),
        entry_index,
        tool_call_id: row.get::<Option<String>, _>("tool_call_id"),
        ephemeral: false,
        notification,
    })
}

pub(crate) fn terminal_effect_from_row<R>(row: &R) -> SessionStoreResult<TerminalEffectRecord>
where
    R: Row,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> i64: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    let id_raw = row.get::<String, _>("id");
    let id = uuid::Uuid::parse_str(&id_raw)
        .map_err(|error| SessionStoreError::InvalidData(error.to_string()))?;
    let terminal_event_seq = parse_non_negative_u64(
        row.get::<i64, _>("terminal_event_seq"),
        "session_terminal_effects.terminal_event_seq",
    )?;
    let attempt_count = parse_non_negative_u32(
        row.get::<i64, _>("attempt_count"),
        "session_terminal_effects.attempt_count",
    )?;
    let payload_json = row.get::<String, _>("payload_json");
    let payload = serde_json::from_str::<serde_json::Value>(&payload_json)
        .map_err(|error| SessionStoreError::InvalidData(error.to_string()))?;
    Ok(TerminalEffectRecord {
        id,
        session_id: row.get::<String, _>("session_id"),
        turn_id: row.get::<String, _>("turn_id"),
        terminal_event_seq,
        effect_type: parse_terminal_effect_type(
            row.get::<String, _>("effect_type"),
            "session_terminal_effects.effect_type",
        )?,
        payload,
        status: parse_terminal_effect_status(
            row.get::<String, _>("status"),
            "session_terminal_effects.status",
        )?,
        attempt_count,
        created_at_ms: row.get::<i64, _>("created_at_ms"),
        updated_at_ms: row.get::<i64, _>("updated_at_ms"),
        last_error: row.get::<Option<String>, _>("last_error"),
    })
}

pub(crate) fn runtime_command_from_row<R>(row: &R) -> SessionStoreResult<RuntimeCommandRecord>
where
    R: Row,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> i64: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<i64>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    let id_raw = row.get::<String, _>("id");
    let id = uuid::Uuid::parse_str(&id_raw)
        .map_err(|error| SessionStoreError::InvalidData(error.to_string()))?;
    let payload_json = row.get::<String, _>("payload_json");
    let delivery = serde_json::from_str::<RuntimeDeliveryCommand>(&payload_json)
        .map_err(|error| SessionStoreError::InvalidData(error.to_string()))?;
    let frame_transition = agent_frame_transition_from_row(row)?;
    let frame_transition_id = row.get::<String, _>("frame_transition_id");
    if delivery.frame_transition_id != frame_transition_id
        || frame_transition.id != frame_transition_id
        || delivery.target_frame_id != frame_transition.target_frame_id
    {
        return Err(SessionStoreError::InvalidData(format!(
            "session_runtime_commands {} delivery 与 agent_frame_transitions {} 不一致",
            id, frame_transition.id
        )));
    }
    Ok(RuntimeCommandRecord {
        id,
        session_id: row.get::<String, _>("session_id"),
        frame_transition_id,
        phase_node: row.get::<String, _>("phase_node"),
        status: parse_runtime_command_status(
            row.get::<String, _>("status"),
            "session_runtime_commands.status",
        )?,
        delivery,
        frame_transition,
        created_at_ms: row.get::<i64, _>("created_at_ms"),
        updated_at_ms: row.get::<i64, _>("updated_at_ms"),
        applied_at_ms: row.get::<Option<i64>, _>("applied_at_ms"),
        failed_at_ms: row.get::<Option<i64>, _>("failed_at_ms"),
        last_error: row.get::<Option<String>, _>("last_error"),
    })
}

fn agent_frame_transition_from_row<R>(row: &R) -> SessionStoreResult<AgentFrameTransitionRecord>
where
    R: Row,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> i64: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    let target_frame_id_raw = row.get::<String, _>("frame_transition_target_frame_id");
    let target_frame_id = uuid::Uuid::parse_str(&target_frame_id_raw).map_err(|error| {
        SessionStoreError::InvalidData(format!(
            "agent_frame_transitions.target_frame_id 不是 UUID: {error}"
        ))
    })?;
    let run_id_raw = row.get::<String, _>("frame_transition_run_id");
    let run_id = uuid::Uuid::parse_str(&run_id_raw).map_err(|error| {
        SessionStoreError::InvalidData(format!("agent_frame_transitions.run_id 不是 UUID: {error}"))
    })?;
    let capability_keys_json = row.get::<String, _>("frame_transition_capability_keys_json");
    let capability_keys = serde_json::from_str::<std::collections::BTreeSet<String>>(
        &capability_keys_json,
    )
    .map_err(|error| {
        SessionStoreError::InvalidData(format!(
            "解析 agent_frame_transitions.capability_keys_json 失败: {error}"
        ))
    })?;
    let transition_json = row.get::<String, _>("frame_transition_transition_json");
    let transition = serde_json::from_str::<RuntimeCapabilityTransition>(&transition_json)
        .map_err(|error| {
            SessionStoreError::InvalidData(format!(
                "解析 agent_frame_transitions.transition_json 失败: {error}"
            ))
        })?;
    Ok(AgentFrameTransitionRecord {
        id: row.get::<String, _>("frame_transition_record_id"),
        target_frame_id,
        run_id,
        lifecycle_key: row.get::<String, _>("frame_transition_lifecycle_key"),
        phase_node: row.get::<String, _>("frame_transition_phase_node"),
        capability_keys,
        transition,
        created_at_ms: row.get::<i64, _>("frame_transition_created_at_ms"),
        source_turn_id: row.get::<Option<String>, _>("frame_transition_source_turn_id"),
    })
}

pub(crate) fn compaction_from_row<R>(row: &R) -> SessionStoreResult<SessionCompactionRecord>
where
    R: Row,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> i64: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<i64>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    Ok(SessionCompactionRecord {
        id: row.get::<String, _>("id"),
        session_id: row.get::<String, _>("session_id"),
        projection_kind: row.get::<String, _>("projection_kind"),
        projection_version: parse_non_negative_u64(
            row.get::<i64, _>("projection_version"),
            "session_compactions.projection_version",
        )?,
        lifecycle_item_id: row.get::<String, _>("lifecycle_item_id"),
        start_event_seq: parse_non_negative_u64(
            row.get::<i64, _>("start_event_seq"),
            "session_compactions.start_event_seq",
        )?,
        completed_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("completed_event_seq"),
            "session_compactions.completed_event_seq",
        )?,
        failed_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("failed_event_seq"),
            "session_compactions.failed_event_seq",
        )?,
        status: parse_compaction_status(
            row.get::<String, _>("status"),
            "session_compactions.status",
        )?,
        trigger: row.get::<String, _>("trigger"),
        reason: row.get::<Option<String>, _>("reason"),
        phase: row.get::<Option<String>, _>("phase"),
        strategy: row.get::<String, _>("strategy"),
        budget_scope: row.get::<Option<String>, _>("budget_scope"),
        base_head_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("base_head_event_seq"),
            "session_compactions.base_head_event_seq",
        )?,
        source_start_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("source_start_event_seq"),
            "session_compactions.source_start_event_seq",
        )?,
        source_end_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("source_end_event_seq"),
            "session_compactions.source_end_event_seq",
        )?,
        first_kept_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("first_kept_event_seq"),
            "session_compactions.first_kept_event_seq",
        )?,
        summary: row.get::<String, _>("summary"),
        replacement_projection_json: parse_json_column(
            row.get::<String, _>("replacement_projection_json"),
            "session_compactions.replacement_projection_json",
        )?,
        token_stats_json: parse_json_column(
            row.get::<String, _>("token_stats_json"),
            "session_compactions.token_stats_json",
        )?,
        diagnostics_json: parse_json_column(
            row.get::<String, _>("diagnostics_json"),
            "session_compactions.diagnostics_json",
        )?,
        created_by: row.get::<Option<String>, _>("created_by"),
        created_at_ms: row.get::<i64, _>("created_at_ms"),
        completed_at_ms: row.get::<Option<i64>, _>("completed_at_ms"),
    })
}

pub(crate) fn projection_segment_from_row<R>(
    row: &R,
) -> SessionStoreResult<SessionProjectionSegmentRecord>
where
    R: Row + SessionRow,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> i64: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<i64>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    Ok(SessionProjectionSegmentRecord {
        id: row.get::<String, _>("id"),
        session_id: row.get::<String, _>("session_id"),
        projection_kind: row.get::<String, _>("projection_kind"),
        projection_version: parse_non_negative_u64(
            row.get::<i64, _>("projection_version"),
            "session_projection_segments.projection_version",
        )?,
        sort_order: parse_non_negative_u64(
            row.get::<i64, _>("sort_order"),
            "session_projection_segments.sort_order",
        )?,
        segment_type: row.get::<String, _>("segment_type"),
        origin: row.get::<String, _>("origin"),
        synthetic: row.synthetic_flag(),
        source_start_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("source_start_event_seq"),
            "session_projection_segments.source_start_event_seq",
        )?,
        source_end_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("source_end_event_seq"),
            "session_projection_segments.source_end_event_seq",
        )?,
        source_refs_json: parse_json_column(
            row.get::<String, _>("source_refs_json"),
            "session_projection_segments.source_refs_json",
        )?,
        generated_by_compaction_id: row.get::<Option<String>, _>("generated_by_compaction_id"),
        content_json: parse_json_column(
            row.get::<String, _>("content_json"),
            "session_projection_segments.content_json",
        )?,
        token_estimate: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("token_estimate"),
            "session_projection_segments.token_estimate",
        )?,
        created_at_ms: row.get::<i64, _>("created_at_ms"),
    })
}

pub(crate) fn projection_head_from_row<R>(
    row: &R,
) -> SessionStoreResult<SessionProjectionHeadRecord>
where
    R: Row,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> i64: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<i64>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    Ok(SessionProjectionHeadRecord {
        session_id: row.get::<String, _>("session_id"),
        projection_kind: row.get::<String, _>("projection_kind"),
        projection_version: parse_non_negative_u64(
            row.get::<i64, _>("projection_version"),
            "session_projection_heads.projection_version",
        )?,
        head_event_seq: parse_non_negative_u64(
            row.get::<i64, _>("head_event_seq"),
            "session_projection_heads.head_event_seq",
        )?,
        active_compaction_id: row.get::<Option<String>, _>("active_compaction_id"),
        updated_by_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("updated_by_event_seq"),
            "session_projection_heads.updated_by_event_seq",
        )?,
        updated_at_ms: row.get::<i64, _>("updated_at_ms"),
    })
}

pub(crate) fn lineage_from_row<R>(row: &R) -> SessionStoreResult<SessionLineageRecord>
where
    R: Row,
    for<'a> String: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<String>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> i64: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> Option<i64>: sqlx::Decode<'a, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    Ok(SessionLineageRecord {
        child_session_id: row.get::<String, _>("child_session_id"),
        parent_session_id: row.get::<String, _>("parent_session_id"),
        relation_kind: parse_lineage_relation_kind(
            row.get::<String, _>("relation_kind"),
            "session_lineage.relation_kind",
        )?,
        fork_point_event_seq: parse_optional_non_negative_u64(
            row.get::<Option<i64>, _>("fork_point_event_seq"),
            "session_lineage.fork_point_event_seq",
        )?,
        fork_point_ref_json: parse_json_column(
            row.get::<String, _>("fork_point_ref_json"),
            "session_lineage.fork_point_ref_json",
        )?,
        fork_point_compaction_id: row.get::<Option<String>, _>("fork_point_compaction_id"),
        status: parse_lineage_status(row.get::<String, _>("status"), "session_lineage.status")?,
        created_at_ms: row.get::<i64, _>("created_at_ms"),
        updated_at_ms: row.get::<i64, _>("updated_at_ms"),
        metadata_json: parse_json_column(
            row.get::<String, _>("metadata_json"),
            "session_lineage.metadata_json",
        )?,
    })
}

pub(crate) fn json_string<T: serde::Serialize>(
    value: &T,
    column: &str,
) -> SessionStoreResult<String> {
    serde_json::to_string(value)
        .map_err(|error| SessionStoreError::InvalidData(format!("序列化 {column} 失败: {error}")))
}

pub(crate) fn title_source_to_str(source: TitleSource) -> &'static str {
    match source {
        TitleSource::Auto => "auto",
        TitleSource::Source => "source",
        TitleSource::User => "user",
    }
}

pub(crate) fn parse_execution_status(
    value: String,
    field: &str,
) -> SessionStoreResult<ExecutionStatus> {
    match value.as_str() {
        "idle" => Ok(ExecutionStatus::Idle),
        "running" => Ok(ExecutionStatus::Running),
        "completed" => Ok(ExecutionStatus::Completed),
        "failed" => Ok(ExecutionStatus::Failed),
        "interrupted" => Ok(ExecutionStatus::Interrupted),
        "lost" => Ok(ExecutionStatus::Lost),
        other => Err(SessionStoreError::InvalidData(format!(
            "{field} 非法: {other}"
        ))),
    }
}

pub(crate) fn parse_terminal_effect_type(
    value: String,
    field: &str,
) -> SessionStoreResult<TerminalEffectType> {
    TerminalEffectType::try_from(value.as_str())
        .map_err(|error| SessionStoreError::InvalidData(format!("{field}: {error}")))
}

pub(crate) fn parse_terminal_effect_status(
    value: String,
    field: &str,
) -> SessionStoreResult<TerminalEffectStatus> {
    TerminalEffectStatus::try_from(value.as_str())
        .map_err(|error| SessionStoreError::InvalidData(format!("{field}: {error}")))
}

pub(crate) fn parse_runtime_command_status(
    value: String,
    field: &str,
) -> SessionStoreResult<RuntimeCommandStatus> {
    RuntimeCommandStatus::try_from(value.as_str())
        .map_err(|error| SessionStoreError::InvalidData(format!("{field}: {error}")))
}

pub(crate) fn parse_compaction_status(
    value: String,
    field: &str,
) -> SessionStoreResult<SessionCompactionStatus> {
    SessionCompactionStatus::try_from(value.as_str())
        .map_err(|error| SessionStoreError::InvalidData(format!("{field}: {error}")))
}

pub(crate) fn parse_lineage_relation_kind(
    value: String,
    field: &str,
) -> SessionStoreResult<SessionLineageRelationKind> {
    SessionLineageRelationKind::try_from(value.as_str())
        .map_err(|error| SessionStoreError::InvalidData(format!("{field}: {error}")))
}

pub(crate) fn parse_lineage_status(
    value: String,
    field: &str,
) -> SessionStoreResult<SessionLineageStatus> {
    SessionLineageStatus::try_from(value.as_str())
        .map_err(|error| SessionStoreError::InvalidData(format!("{field}: {error}")))
}

pub(crate) fn parse_title_source(value: String, field: &str) -> SessionStoreResult<TitleSource> {
    match value.as_str() {
        "auto" => Ok(TitleSource::Auto),
        "source" => Ok(TitleSource::Source),
        "user" => Ok(TitleSource::User),
        other => Err(SessionStoreError::InvalidData(format!(
            "{field} 非法: {other}"
        ))),
    }
}

pub(crate) fn encode_u64_as_i64(value: u64, field: &str) -> SessionStoreResult<i64> {
    i64::try_from(value).map_err(|_| {
        SessionStoreError::InvalidData(format!("{field} 超出 i64 可表示范围: {value}"))
    })
}

pub(crate) fn encode_optional_u64_as_i64(
    value: Option<u64>,
    field: &str,
) -> SessionStoreResult<Option<i64>> {
    value
        .map(|inner| encode_u64_as_i64(inner, field))
        .transpose()
}

pub(crate) fn parse_non_negative_u64(value: i64, field: &str) -> SessionStoreResult<u64> {
    u64::try_from(value)
        .map_err(|_| SessionStoreError::InvalidData(format!("{field} 不能为负数: {value}")))
}

pub(crate) fn parse_optional_non_negative_u64(
    value: Option<i64>,
    field: &str,
) -> SessionStoreResult<Option<u64>> {
    value
        .map(|inner| parse_non_negative_u64(inner, field))
        .transpose()
}

pub(crate) fn parse_non_negative_u32(value: i64, field: &str) -> SessionStoreResult<u32> {
    u32::try_from(value)
        .map_err(|_| SessionStoreError::InvalidData(format!("{field} 超出 u32 范围: {value}")))
}

pub(crate) fn parse_json_column(
    raw: String,
    column: &str,
) -> SessionStoreResult<serde_json::Value> {
    serde_json::from_str(&raw)
        .map_err(|error| SessionStoreError::InvalidData(format!("解析 {column} 失败: {error}")))
}

pub(crate) fn validate_commit_session(
    session_id: &str,
    commit: &NewCompactionProjectionCommit,
) -> SessionStoreResult<()> {
    if commit.compaction.session_id != session_id || commit.head.session_id != session_id {
        return Err(SessionStoreError::InvalidInput(format!(
            "compaction projection commit session_id 不一致: {session_id}"
        )));
    }
    if commit
        .segments
        .iter()
        .any(|segment| segment.session_id != session_id)
    {
        return Err(SessionStoreError::InvalidInput(format!(
            "projection segment session_id 不一致: {session_id}"
        )));
    }
    if commit.compaction.projection_kind != commit.head.projection_kind {
        return Err(SessionStoreError::InvalidInput(format!(
            "compaction projection kind {} 与 head kind {} 不一致",
            commit.compaction.projection_kind, commit.head.projection_kind
        )));
    }
    if commit.compaction.projection_version != commit.head.projection_version {
        return Err(SessionStoreError::InvalidInput(format!(
            "compaction projection version {} 与 head version {} 不一致",
            commit.compaction.projection_version, commit.head.projection_version
        )));
    }
    if commit.head.active_compaction_id.as_deref() != Some(commit.compaction.id.as_str()) {
        return Err(SessionStoreError::InvalidInput(format!(
            "projection head active_compaction_id 必须指向当前 compaction {}",
            commit.compaction.id
        )));
    }
    let compaction_range = source_range_pair(
        "session_compactions",
        commit.compaction.source_start_event_seq,
        commit.compaction.source_end_event_seq,
    )?;
    for segment in &commit.segments {
        if segment.projection_kind != commit.compaction.projection_kind {
            return Err(SessionStoreError::InvalidInput(format!(
                "projection segment {} kind {} 与 compaction kind {} 不一致",
                segment.id, segment.projection_kind, commit.compaction.projection_kind
            )));
        }
        if segment.projection_version != commit.compaction.projection_version {
            return Err(SessionStoreError::InvalidInput(format!(
                "projection segment {} version {} 与 compaction version {} 不一致",
                segment.id, segment.projection_version, commit.compaction.projection_version
            )));
        }
        if segment.generated_by_compaction_id.as_deref() != Some(commit.compaction.id.as_str()) {
            return Err(SessionStoreError::InvalidInput(format!(
                "projection segment {} 必须归属于 compaction {}",
                segment.id, commit.compaction.id
            )));
        }
        let segment_range = source_range_pair(
            "session_projection_segments",
            segment.source_start_event_seq,
            segment.source_end_event_seq,
        )?;
        match (compaction_range, segment_range) {
            (Some((compaction_start, compaction_end)), Some((segment_start, segment_end)))
                if segment_start < compaction_start || segment_end > compaction_end =>
            {
                return Err(SessionStoreError::InvalidInput(format!(
                    "projection segment {} source range 不在 compaction {} source range 内",
                    segment.id, commit.compaction.id
                )));
            }
            (None, Some(_)) | (Some(_), None) => {
                return Err(SessionStoreError::InvalidInput(format!(
                    "projection segment {} source range 与 compaction {} 不一致",
                    segment.id, commit.compaction.id
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn source_range_pair(
    label: &str,
    start: Option<u64>,
    end: Option<u64>,
) -> SessionStoreResult<Option<(u64, u64)>> {
    match (start, end) {
        (Some(start), Some(end)) if start <= end => Ok(Some((start, end))),
        (Some(start), Some(end)) => Err(SessionStoreError::InvalidInput(format!(
            "{label} source range 非法: {start}>{end}"
        ))),
        (None, None) => Ok(None),
        _ => Err(SessionStoreError::InvalidInput(format!(
            "{label} source range 必须同时包含 start/end"
        ))),
    }
}

pub(crate) fn sqlx_to_session_store_error(error: sqlx::Error) -> SessionStoreError {
    SessionStoreError::Database(error.to_string())
}

/// 从 envelope 推导出需要回写到 `sessions` 行的投影字段。
pub(crate) struct SessionProjection {
    pub last_delivery_status: Option<String>,
    pub turn_id: Option<String>,
    pub last_terminal_message: Option<String>,
    pub clear_terminal_message: bool,
    pub executor_session_id: Option<String>,
    pub entry_index: Option<u32>,
    pub tool_call_id: Option<String>,
}

pub(crate) fn projection_from_envelope(envelope: &BackboneEnvelope) -> SessionProjection {
    let turn_id = envelope.trace.turn_id.clone();
    let entry_index = envelope.trace.entry_index;
    let tool_call_id = envelope_tool_call_id(envelope);

    let mut projection = SessionProjection {
        last_delivery_status: None,
        turn_id,
        last_terminal_message: None,
        clear_terminal_message: false,
        executor_session_id: None,
        entry_index,
        tool_call_id,
    };

    match &envelope.event {
        BackboneEvent::TurnStarted(_) => {
            projection.last_delivery_status = Some("running".to_string());
            projection.clear_terminal_message = true;
        }
        BackboneEvent::TurnCompleted(n) => {
            let status = match n.turn.status {
                agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::Completed => {
                    "completed"
                }
                agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::Failed => "failed",
                agentdash_agent_protocol::codex_app_server_protocol::TurnStatus::Interrupted => {
                    "interrupted"
                }
                _ => "completed",
            };
            projection.last_delivery_status = Some(status.to_string());
            projection.last_terminal_message = n.turn.error.as_ref().map(|e| e.message.clone());
        }
        BackboneEvent::Error(e) => {
            projection.last_delivery_status = Some("failed".to_string());
            projection.last_terminal_message = Some(e.error.message.clone());
        }
        BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
            executor_session_id,
        }) => {
            projection.executor_session_id = Some(executor_session_id.clone());
        }
        BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate { key, value })
            if key == "turn_terminal" =>
        {
            if let Some(kind) = value.get("terminal_type").and_then(|v| v.as_str()) {
                let status = match kind {
                    "turn_completed" => "completed",
                    "turn_failed" => "failed",
                    "turn_interrupted" => "interrupted",
                    "turn_lost" => "lost",
                    _ => "completed",
                };
                projection.last_delivery_status = Some(status.to_string());
            }
            projection.last_terminal_message = value
                .get("message")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
        _ => {}
    }

    projection
}

pub(crate) fn backbone_event_type_name(event: &BackboneEvent) -> &'static str {
    match event {
        BackboneEvent::AgentMessageDelta(_) => "agent_message_delta",
        BackboneEvent::ReasoningTextDelta(_) => "reasoning_text_delta",
        BackboneEvent::ReasoningSummaryDelta(_) => "reasoning_summary_delta",
        BackboneEvent::ItemStarted(_) => "item_started",
        BackboneEvent::ItemUpdated(_) => "item_updated",
        BackboneEvent::ItemCompleted(_) => "item_completed",
        BackboneEvent::CommandOutputDelta(_) => "command_output_delta",
        BackboneEvent::FileChangeDelta(_) => "file_change_delta",
        BackboneEvent::McpToolCallProgress(_) => "mcp_tool_call_progress",
        BackboneEvent::TurnStarted(_) => "turn_started",
        BackboneEvent::TurnCompleted(_) => "turn_completed",
        BackboneEvent::TurnDiffUpdated(_) => "turn_diff_updated",
        BackboneEvent::UserInputSubmitted(_) => "user_input_submitted",
        BackboneEvent::TurnPlanUpdated(_) => "turn_plan_updated",
        BackboneEvent::PlanDelta(_) => "plan_delta",
        BackboneEvent::TokenUsageUpdated(_) => "token_usage_updated",
        BackboneEvent::ThreadStatusChanged(_) => "thread_status_changed",
        BackboneEvent::ExecutorContextCompacted(_) => "executor_context_compacted",
        BackboneEvent::ApprovalRequest(_) => "approval_request",
        BackboneEvent::Error(_) => "error",
        BackboneEvent::Platform(_) => "platform",
    }
}

fn thread_item_tool_call_id(item: &AgentDashThreadItem) -> Option<String> {
    item.tool_call_id().map(ToString::to_string)
}

fn envelope_tool_call_id(envelope: &BackboneEnvelope) -> Option<String> {
    match &envelope.event {
        BackboneEvent::ItemStarted(n) => thread_item_tool_call_id(&n.item),
        BackboneEvent::ItemUpdated(n) => thread_item_tool_call_id(&n.item),
        BackboneEvent::ItemCompleted(n) => thread_item_tool_call_id(&n.item),
        _ => None,
    }
}
