use agentdash_diagnostics::{Subsystem, diag};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use agentdash_domain::workflow::{
    AgentRunAcceptedRefs, AgentRunCommandClaim, AgentRunCommandKind, AgentRunCommandReceipt,
    AgentRunCommandReceiptRepository, AgentRunCommandStatus, NewAgentRunCommandReceipt,
};

use crate::error::WorkflowApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunCommandReceiptView {
    pub client_command_id: String,
    pub status: String,
    pub duplicate: bool,
    pub message: Option<String>,
}

impl AgentRunCommandReceiptView {
    pub fn from_record(record: &AgentRunCommandReceipt, duplicate: bool) -> Self {
        Self {
            client_command_id: record.client_command_id.clone(),
            status: record.status.as_str().to_string(),
            duplicate,
            message: record.error_message.clone(),
        }
    }
}

pub(crate) struct ClaimedAgentRunCommandReceipt {
    pub record: AgentRunCommandReceipt,
    pub duplicate: bool,
}

pub(crate) async fn claim_agent_run_command_receipt(
    repo: &dyn AgentRunCommandReceiptRepository,
    scope_kind: impl Into<String>,
    scope_key: impl Into<String>,
    command_kind: AgentRunCommandKind,
    client_command_id: impl Into<String>,
    request_digest: impl Into<String>,
) -> Result<ClaimedAgentRunCommandReceipt, WorkflowApplicationError> {
    let claim = repo
        .claim(NewAgentRunCommandReceipt {
            scope_kind: scope_kind.into(),
            scope_key: scope_key.into(),
            command_kind,
            client_command_id: client_command_id.into(),
            request_digest: request_digest.into(),
        })
        .await?;
    Ok(match claim {
        AgentRunCommandClaim::Created(record) => ClaimedAgentRunCommandReceipt {
            record,
            duplicate: false,
        },
        AgentRunCommandClaim::Duplicate(record) => ClaimedAgentRunCommandReceipt {
            record,
            duplicate: true,
        },
    })
}

pub(crate) fn accepted_refs_from_record(
    record: &AgentRunCommandReceipt,
) -> Result<AgentRunAcceptedRefs, WorkflowApplicationError> {
    match record.status {
        AgentRunCommandStatus::Accepted => record.accepted_refs.clone().ok_or_else(|| {
            WorkflowApplicationError::Internal(format!(
                "command receipt {} 缺少 accepted refs",
                record.id
            ))
        }),
        AgentRunCommandStatus::Pending => Err(WorkflowApplicationError::Conflict(
            "命令仍在处理中，请刷新 AgentRun workspace 获取最新状态".to_string(),
        )),
        AgentRunCommandStatus::TerminalFailed => Err(WorkflowApplicationError::Conflict(
            record
                .error_message
                .clone()
                .unwrap_or_else(|| "命令已失败".to_string()),
        )),
    }
}

pub(crate) async fn mark_command_terminal_failed(
    repo: &dyn AgentRunCommandReceiptRepository,
    receipt_id: uuid::Uuid,
    error: &WorkflowApplicationError,
) {
    if let Err(mark_error) = repo
        .mark_terminal_failed(receipt_id, error.to_string())
        .await
    {
        diag!(Warn, Subsystem::AgentRun,

            receipt_id = %receipt_id,
            error = %mark_error,
            "写入 AgentRun command terminal_failed receipt 失败"
        );
    }
}

pub(crate) fn digest_command_request<T: Serialize>(
    request: &T,
) -> Result<String, WorkflowApplicationError> {
    let value = serde_json::to_value(request).map_err(|error| {
        WorkflowApplicationError::BadRequest(format!("命令请求无法序列化: {error}"))
    })?;
    let canonical = canonicalize_json_value(value);
    let bytes = serde_json::to_vec(&canonical).map_err(|error| {
        WorkflowApplicationError::BadRequest(format!("命令请求 digest 无法序列化: {error}"))
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn canonicalize_json_value(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(canonicalize_json_value)
                .collect::<Vec<_>>(),
        ),
        Value::Object(map) => {
            let mut entries = map.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = Map::new();
            for (key, value) in entries {
                sorted.insert(key, canonicalize_json_value(value));
            }
            Value::Object(sorted)
        }
        other => other,
    }
}
