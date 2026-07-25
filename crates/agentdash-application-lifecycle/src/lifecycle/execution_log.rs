//! Lifecycle run data I/O helpers.
//!
//! - Execution log recording (`PendingExecutionLogEntry` → `LifecycleRun.execution_log`)
//! - Activity summary materialization (→ inline_fs `session_records/{node_path}/summary`)
//! - Scoped runtime node port output loading (← inline_fs `port_outputs/`)

use std::collections::{BTreeMap, HashMap};

use chrono::Utc;
use uuid::Uuid;

use agentdash_application_workflow::WorkflowApplicationError;
use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use agentdash_domain::workflow::{
    LifecycleExecutionEntry, LifecycleExecutionEventKind, LifecycleRunRepository,
};
use agentdash_platform_spi::hooks::PendingExecutionLogEntry;

fn parse_event_kind(s: &str) -> Option<LifecycleExecutionEventKind> {
    match s {
        "activity_activated" => Some(LifecycleExecutionEventKind::ActivityActivated),
        "activity_completed" => Some(LifecycleExecutionEventKind::ActivityCompleted),
        "constraint_blocked" => Some(LifecycleExecutionEventKind::ConstraintBlocked),
        "completion_evaluated" => Some(LifecycleExecutionEventKind::CompletionEvaluated),
        "artifact_appended" => Some(LifecycleExecutionEventKind::ArtifactAppended),
        "context_injected" => Some(LifecycleExecutionEventKind::ContextInjected),
        _ => None,
    }
}

fn to_domain_entry(entry: &PendingExecutionLogEntry) -> Option<LifecycleExecutionEntry> {
    Some(LifecycleExecutionEntry {
        timestamp: Utc::now(),
        activity_key: entry.activity_key.clone(),
        event_kind: parse_event_kind(&entry.event_kind)?,
        summary: entry.summary.clone(),
        detail: entry.detail.clone(),
    })
}

/// Flush pending entries grouped by `run_id`.
pub async fn flush_execution_log_entries(
    repo: &dyn LifecycleRunRepository,
    entries: Vec<PendingExecutionLogEntry>,
) -> Result<(), WorkflowApplicationError> {
    let mut by_run: HashMap<String, Vec<LifecycleExecutionEntry>> = HashMap::new();
    for entry in &entries {
        if let Some(domain_entry) = to_domain_entry(entry) {
            by_run
                .entry(entry.run_id.clone())
                .or_default()
                .push(domain_entry);
        }
    }

    for (run_id_str, domain_entries) in by_run {
        let run_id = Uuid::parse_str(&run_id_str).map_err(|e| {
            WorkflowApplicationError::Internal(format!("invalid run_id in execution log: {e}"))
        })?;

        let Some(mut run) = repo.get_by_id(run_id).await? else {
            continue;
        };

        run.append_execution_log(domain_entries);

        repo.update(&run).await?;
    }

    Ok(())
}

pub fn activity_completed_entry(
    run_id: &str,
    activity_key: &str,
    summary: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        activity_key: activity_key.to_string(),
        event_kind: "activity_completed".to_string(),
        summary: summary.to_string(),
        detail: None,
    }
}

pub fn completion_evaluated_entry(
    run_id: &str,
    activity_key: &str,
    satisfied: bool,
    summary: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        activity_key: activity_key.to_string(),
        event_kind: "completion_evaluated".to_string(),
        summary: summary.to_string(),
        detail: Some(serde_json::json!({ "satisfied": satisfied })),
    }
}

pub fn constraint_blocked_entry(
    run_id: &str,
    activity_key: &str,
    reason: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        activity_key: activity_key.to_string(),
        event_kind: "constraint_blocked".to_string(),
        summary: reason.to_string(),
        detail: None,
    }
}

pub fn context_injected_entry(
    run_id: &str,
    activity_key: &str,
    summary: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        activity_key: activity_key.to_string(),
        event_kind: "context_injected".to_string(),
        summary: summary.to_string(),
        detail: None,
    }
}

pub fn artifact_appended_entry(
    run_id: &str,
    activity_key: &str,
    artifact_type: &str,
    title: &str,
) -> PendingExecutionLogEntry {
    PendingExecutionLogEntry {
        run_id: run_id.to_string(),
        activity_key: activity_key.to_string(),
        event_kind: "artifact_appended".to_string(),
        summary: format!("{artifact_type}: {title}"),
        detail: Some(serde_json::json!({
            "artifact_type": artifact_type,
            "title": title,
        })),
    }
}

/// 将 node summary 物化到 inline_fs（`session_records/{activity_key}/summary`）。
pub async fn materialize_activity_summary(
    repo: &dyn InlineFileRepository,
    run_id: Uuid,
    activity_key: &str,
    summary: &str,
) {
    let file = InlineFile::new(
        InlineFileOwnerKind::LifecycleRun,
        run_id,
        "session_records",
        format!("{activity_key}/summary"),
        summary.to_string(),
    );
    let _ = repo.upsert_file(&file).await;
}

/// Runtime node attempt 级别的 port output artifact scope。
#[derive(Debug, Clone)]
pub struct RuntimeNodeArtifactScope {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
}

impl RuntimeNodeArtifactScope {
    pub fn port_ref(&self, port_key: impl Into<String>) -> RuntimeNodePortArtifactRef {
        RuntimeNodePortArtifactRef {
            run_id: self.run_id,
            orchestration_id: self.orchestration_id,
            node_path: self.node_path.clone(),
            attempt: self.attempt,
            port_key: port_key.into(),
        }
    }

    pub(crate) fn path_prefix(&self) -> String {
        format!(
            "{}/{}/{}/",
            self.orchestration_id,
            encode_node_path_segment(&self.node_path),
            self.attempt
        )
    }
}

/// Runtime node attempt scoped port artifact 引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeNodePortArtifactRef {
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: String,
    pub attempt: u32,
    pub port_key: String,
}

impl RuntimeNodePortArtifactRef {
    pub fn inline_path(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.orchestration_id,
            encode_node_path_segment(&self.node_path),
            self.attempt,
            self.port_key
        )
    }
}

/// 加载 runtime node attempt 级别的 port output map（仅含非空内容）。
pub async fn load_scoped_port_output_map(
    repo: &dyn InlineFileRepository,
    scope: &RuntimeNodeArtifactScope,
) -> BTreeMap<String, String> {
    let prefix = scope.path_prefix();
    repo.list_files(
        InlineFileOwnerKind::LifecycleRun,
        scope.run_id,
        "port_outputs",
    )
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|f| {
        let port_key = f.path.strip_prefix(&prefix)?.to_string();
        if port_key.is_empty() || port_key.contains('/') {
            return None;
        }
        let content = f.into_text_content()?;
        (!content.trim().is_empty()).then_some((port_key, content))
    })
    .collect()
}

pub fn encode_node_path_segment(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        let is_safe = byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-');
        if is_safe {
            encoded.push(char::from(*byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 0x0F) as usize]));
        }
    }
    encoded
}

pub fn decode_node_path_segment(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let Some(high) = bytes.get(index + 1).and_then(|value| hex_value(*value)) else {
            return Err(format!(
                "node_path segment percent escape 不完整: `{value}`"
            ));
        };
        let Some(low) = bytes.get(index + 2).and_then(|value| hex_value(*value)) else {
            return Err(format!(
                "node_path segment percent escape 不完整: `{value}`"
            ));
        };
        decoded.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(decoded).map_err(|error| format!("node_path segment 不是有效 UTF-8: {error}"))
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
