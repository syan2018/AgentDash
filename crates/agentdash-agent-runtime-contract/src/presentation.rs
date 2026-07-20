use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{RuntimePayloadDigest, RuntimeU64, canonical_json_sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimePresentationContentBlock {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        source: String,
        detail: Option<String>,
        digest: RuntimePayloadDigest,
    },
    LocalResource {
        path: String,
        media_type: Option<String>,
        digest: Option<RuntimePayloadDigest>,
    },
    ResourceLink {
        uri: String,
        title: Option<String>,
        media_type: Option<String>,
        digest: Option<RuntimePayloadDigest>,
    },
    SkillReference {
        name: String,
        path: Option<String>,
    },
    Mention {
        label: String,
        reference: String,
    },
    /// AgentDash-owned structured presentation only. `schema_version` makes evolution explicit.
    Structured {
        schema: String,
        schema_version: u32,
        value: Value,
    },
}

impl ManagedRuntimePresentationContentBlock {
    pub fn validate(&self) -> Result<(), ManagedRuntimePresentationViolation> {
        if let Self::Structured {
            schema,
            schema_version,
            ..
        } = self
            && (!schema.starts_with("agentdash.") || *schema_version == 0)
        {
            return Err(ManagedRuntimePresentationViolation::InvalidStructuredSchema);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeFileSearchMode {
    Grep,
    Glob,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeFileSearchMatch {
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimePlanStep {
    pub id: Option<String>,
    pub text: String,
    pub status: ManagedRuntimePlanStepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimePlanStepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeFilePatch {
    pub path: String,
    pub change_kind: ManagedRuntimeFileChangeKind,
    pub patch: String,
    pub moved_to: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeFileChangeKind {
    Add,
    Update,
    Delete,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeCommandOutput {
    pub stream: ManagedRuntimeCommandOutputStream,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeCommandOutputStream {
    Stdout,
    Stderr,
    Combined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeReviewFinding {
    pub title: String,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub severity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeItemBody {
    UserMessage {
        content: Vec<ManagedRuntimePresentationContentBlock>,
    },
    HookPrompt {
        hook_point: String,
        content: Vec<ManagedRuntimePresentationContentBlock>,
    },
    AgentMessage {
        content: Vec<ManagedRuntimePresentationContentBlock>,
        phase: Option<String>,
    },
    Reasoning {
        summary: Vec<ManagedRuntimePresentationContentBlock>,
        content: Vec<ManagedRuntimePresentationContentBlock>,
    },
    Plan {
        explanation: Option<String>,
        steps: Vec<ManagedRuntimePlanStep>,
    },
    CommandExecution {
        command: String,
        cwd: Option<String>,
        output: Vec<ManagedRuntimeCommandOutput>,
    },
    FileChange {
        changes: Vec<ManagedRuntimeFilePatch>,
        output: Vec<ManagedRuntimePresentationContentBlock>,
    },
    FileRead {
        path: String,
        line_start: Option<u32>,
        line_end: Option<u32>,
        content: Vec<ManagedRuntimePresentationContentBlock>,
    },
    FileSearch {
        mode: ManagedRuntimeFileSearchMode,
        query: String,
        path: Option<String>,
        matches: Vec<ManagedRuntimeFileSearchMatch>,
    },
    McpToolCall {
        server: String,
        tool: String,
        arguments: Value,
        result: Option<Value>,
        progress: Vec<ManagedRuntimePresentationContentBlock>,
    },
    DynamicToolCall {
        namespace: Option<String>,
        tool: String,
        arguments: Value,
        result: Option<Value>,
        progress: Vec<ManagedRuntimePresentationContentBlock>,
    },
    CollaborationToolCall {
        action: String,
        target: Option<String>,
        prompt: Option<String>,
        result: Option<Value>,
    },
    SubagentActivity {
        agent_id: String,
        task: String,
        status: String,
        result: Vec<ManagedRuntimePresentationContentBlock>,
    },
    WebSearch {
        action: String,
        query: Option<String>,
        url: Option<String>,
        results: Vec<ManagedRuntimePresentationContentBlock>,
    },
    ImageView {
        path: String,
        detail: Option<String>,
    },
    ImageGeneration {
        prompt: String,
        revised_prompt: Option<String>,
        outputs: Vec<ManagedRuntimePresentationContentBlock>,
    },
    Sleep {
        duration_ms: RuntimeU64,
    },
    Review {
        findings: Vec<ManagedRuntimeReviewFinding>,
        summary: Option<String>,
    },
    TerminalControl {
        terminal_id: String,
        action: String,
        text: Option<String>,
    },
    ContextCompaction {
        summary: Option<Vec<ManagedRuntimePresentationContentBlock>>,
        source_digest: Option<RuntimePayloadDigest>,
    },
    GenericToolActivity {
        name: String,
        arguments: Value,
        result: Option<Value>,
        progress: Vec<ManagedRuntimePresentationContentBlock>,
    },
    Error {
        code: String,
        message: String,
        details: Option<Vec<ManagedRuntimePresentationContentBlock>>,
    },
}

impl ManagedRuntimeItemBody {
    pub fn validate(&self) -> Result<(), ManagedRuntimePresentationViolation> {
        for block in self.content_blocks() {
            block.validate()?;
        }
        Ok(())
    }

    fn content_blocks(&self) -> Vec<&ManagedRuntimePresentationContentBlock> {
        match self {
            Self::UserMessage { content }
            | Self::HookPrompt { content, .. }
            | Self::AgentMessage { content, .. }
            | Self::FileRead { content, .. } => content.iter().collect(),
            Self::Reasoning { summary, content } => summary.iter().chain(content).collect(),
            Self::FileChange { output, .. }
            | Self::McpToolCall {
                progress: output, ..
            }
            | Self::DynamicToolCall {
                progress: output, ..
            }
            | Self::WebSearch {
                results: output, ..
            }
            | Self::ImageGeneration {
                outputs: output, ..
            }
            | Self::GenericToolActivity {
                progress: output, ..
            } => output.iter().collect(),
            Self::SubagentActivity { result, .. } => result.iter().collect(),
            Self::ContextCompaction {
                summary: Some(summary),
                ..
            } => summary.iter().collect(),
            Self::Error {
                details: Some(details),
                ..
            } => details.iter().collect(),
            _ => Vec::new(),
        }
    }

    pub fn digest(&self) -> RuntimePayloadDigest {
        digest(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeProcessExitEvidence {
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimePresentationError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeItemTerminalEvidence {
    pub outcome: ManagedRuntimeTerminalStatus,
    pub completed_at_ms: Option<RuntimeU64>,
    pub duration_ms: Option<RuntimeU64>,
    pub process_exit: Option<ManagedRuntimeProcessExitEvidence>,
    pub error: Option<ManagedRuntimePresentationError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeTerminalStatus {
    Completed,
    Failed,
    Interrupted,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeItemPresentation {
    pub body: ManagedRuntimeItemBody,
    pub started_at_ms: Option<RuntimeU64>,
    pub updated_at_ms: Option<RuntimeU64>,
    pub terminal: Option<ManagedRuntimeItemTerminalEvidence>,
    pub body_digest: RuntimePayloadDigest,
    pub presentation_digest: RuntimePayloadDigest,
}

impl ManagedRuntimeItemPresentation {
    pub fn new(
        body: ManagedRuntimeItemBody,
        started_at_ms: Option<u64>,
        updated_at_ms: Option<u64>,
        terminal: Option<ManagedRuntimeItemTerminalEvidence>,
    ) -> Result<Self, ManagedRuntimePresentationViolation> {
        body.validate()?;
        let body_digest = body.digest();
        let mut value = Self {
            body,
            started_at_ms: started_at_ms.map(RuntimeU64),
            updated_at_ms: updated_at_ms.map(RuntimeU64),
            terminal,
            body_digest,
            presentation_digest: RuntimePayloadDigest::new("pending")
                .expect("fixed non-empty digest"),
        };
        value.presentation_digest = value.calculated_presentation_digest();
        Ok(value)
    }

    pub fn calculated_presentation_digest(&self) -> RuntimePayloadDigest {
        digest(&(
            &self.body,
            self.started_at_ms,
            self.updated_at_ms,
            &self.terminal,
        ))
    }

    pub fn validate_for_status(
        &self,
        status: crate::ManagedRuntimeEntityStatus,
    ) -> Result<(), ManagedRuntimePresentationViolation> {
        self.body.validate()?;
        if self.body_digest != self.body.digest()
            || self.presentation_digest != self.calculated_presentation_digest()
        {
            return Err(ManagedRuntimePresentationViolation::DigestMismatch);
        }
        let terminal = matches!(
            status,
            crate::ManagedRuntimeEntityStatus::Completed
                | crate::ManagedRuntimeEntityStatus::Failed
                | crate::ManagedRuntimeEntityStatus::Interrupted
                | crate::ManagedRuntimeEntityStatus::Lost
        );
        if terminal != self.terminal.is_some() {
            return Err(ManagedRuntimePresentationViolation::TerminalEvidenceMismatch);
        }
        if let Some(evidence) = &self.terminal {
            let expected = match status {
                crate::ManagedRuntimeEntityStatus::Completed => {
                    ManagedRuntimeTerminalStatus::Completed
                }
                crate::ManagedRuntimeEntityStatus::Failed => ManagedRuntimeTerminalStatus::Failed,
                crate::ManagedRuntimeEntityStatus::Interrupted => {
                    ManagedRuntimeTerminalStatus::Interrupted
                }
                crate::ManagedRuntimeEntityStatus::Lost => ManagedRuntimeTerminalStatus::Lost,
                crate::ManagedRuntimeEntityStatus::Accepted
                | crate::ManagedRuntimeEntityStatus::Running => {
                    unreachable!("terminal evidence was rejected above")
                }
            };
            if evidence.outcome != expected {
                return Err(ManagedRuntimePresentationViolation::TerminalEvidenceMismatch);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeInteractionRequest {
    Approval {
        prompt: String,
        reason: Option<String>,
        proposed_action: Option<Value>,
    },
    UserInput {
        prompt: String,
        questions: Vec<ManagedRuntimeInteractionQuestion>,
    },
    McpElicitation {
        server: String,
        prompt: String,
        schema: Value,
    },
    DynamicTool {
        namespace: Option<String>,
        tool: String,
        prompt: String,
        arguments: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct ManagedRuntimeInteractionQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub allows_free_form: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRuntimeInteractionStatus {
    Pending,
    Resolved,
    Cancelled,
    Expired,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedRuntimeInteractionResolution {
    Approved,
    Denied { reason: Option<String> },
    UserInput { answers: Value },
    McpElicitation { response: Value },
    DynamicToolResult { result: Value },
    Cancelled { reason: Option<String> },
    Expired,
    Lost { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManagedRuntimePresentationViolation {
    #[error("structured presentation schemas must be versioned AgentDash-owned schemas")]
    InvalidStructuredSchema,
    #[error("presentation digest does not match canonical body and evidence")]
    DigestMismatch,
    #[error("terminal status and terminal evidence do not agree")]
    TerminalEvidenceMismatch,
}

fn digest<T: Serialize>(value: &T) -> RuntimePayloadDigest {
    RuntimePayloadDigest::new(
        canonical_json_sha256(value).expect("canonical presentation must serialize"),
    )
    .expect("sha256 digest is non-empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal(outcome: ManagedRuntimeTerminalStatus) -> ManagedRuntimeItemTerminalEvidence {
        ManagedRuntimeItemTerminalEvidence {
            outcome,
            completed_at_ms: Some(RuntimeU64(u64::MAX)),
            duration_ms: Some(RuntimeU64(7)),
            process_exit: None,
            error: None,
        }
    }

    #[test]
    fn terminal_and_digest_invariants_are_closed() {
        let running = ManagedRuntimeItemPresentation::new(
            ManagedRuntimeItemBody::AgentMessage {
                content: vec![ManagedRuntimePresentationContentBlock::Text {
                    text: "hello".to_owned(),
                }],
                phase: None,
            },
            Some(1),
            Some(2),
            None,
        )
        .expect("running presentation");
        running
            .validate_for_status(crate::ManagedRuntimeEntityStatus::Running)
            .expect("running evidence");
        assert_eq!(
            running
                .validate_for_status(crate::ManagedRuntimeEntityStatus::Completed)
                .expect_err("terminal requires evidence"),
            ManagedRuntimePresentationViolation::TerminalEvidenceMismatch
        );

        let completed = ManagedRuntimeItemPresentation::new(
            ManagedRuntimeItemBody::AgentMessage {
                content: vec![ManagedRuntimePresentationContentBlock::Text {
                    text: "done".to_owned(),
                }],
                phase: None,
            },
            Some(1),
            Some(3),
            Some(terminal(ManagedRuntimeTerminalStatus::Completed)),
        )
        .expect("completed presentation");
        completed
            .validate_for_status(crate::ManagedRuntimeEntityStatus::Completed)
            .expect("completed evidence");
        assert_ne!(running.presentation_digest, completed.presentation_digest);
        let json = serde_json::to_value(&completed).expect("serialize");
        assert_eq!(
            json["terminal"]["completed_at_ms"],
            serde_json::Value::String(u64::MAX.to_string())
        );
    }

    #[test]
    fn presentation_digest_ignores_nested_json_object_key_order() {
        let left = ManagedRuntimeItemBody::DynamicToolCall {
            namespace: Some("dash".to_owned()),
            tool: "lookup".to_owned(),
            arguments: serde_json::json!({"z": {"b": 2, "a": 1}, "a": 0}),
            result: Some(serde_json::json!({"output": {"y": 2, "x": 1}})),
            progress: Vec::new(),
        };
        let right = ManagedRuntimeItemBody::DynamicToolCall {
            namespace: Some("dash".to_owned()),
            tool: "lookup".to_owned(),
            arguments: serde_json::from_str(r#"{"a":0,"z":{"a":1,"b":2}}"#)
                .expect("equivalent arguments"),
            result: Some(
                serde_json::from_str(r#"{"output":{"x":1,"y":2}}"#).expect("equivalent result"),
            ),
            progress: Vec::new(),
        };

        assert_eq!(left.digest(), right.digest());
        assert_eq!(
            ManagedRuntimeItemPresentation::new(left, Some(1), Some(2), None)
                .expect("left presentation")
                .presentation_digest,
            ManagedRuntimeItemPresentation::new(right, Some(1), Some(2), None)
                .expect("right presentation")
                .presentation_digest
        );
    }

    #[test]
    fn structured_blocks_require_agentdash_schema_and_version() {
        let invalid = ManagedRuntimePresentationContentBlock::Structured {
            schema: "codex.thread_item".to_owned(),
            schema_version: 1,
            value: serde_json::json!({}),
        };
        assert_eq!(
            invalid
                .validate()
                .expect_err("vendor schema is not canonical"),
            ManagedRuntimePresentationViolation::InvalidStructuredSchema
        );
    }
}
