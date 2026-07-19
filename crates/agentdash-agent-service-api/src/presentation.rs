use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{AgentPayloadDigest, AgentServiceU64, canonical_json_sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentContentBlock {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        source: String,
        detail: Option<String>,
        digest: AgentPayloadDigest,
    },
    LocalResource {
        path: String,
        media_type: Option<String>,
        digest: Option<AgentPayloadDigest>,
    },
    ResourceLink {
        uri: String,
        title: Option<String>,
        media_type: Option<String>,
        digest: Option<AgentPayloadDigest>,
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

impl AgentContentBlock {
    pub fn validate(&self) -> Result<(), AgentPresentationViolation> {
        if let Self::Structured {
            schema,
            schema_version,
            ..
        } = self
            && (!schema.starts_with("agentdash.") || *schema_version == 0)
        {
            return Err(AgentPresentationViolation::InvalidStructuredSchema);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentFileSearchMode {
    Grep,
    Glob,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentFileSearchMatch {
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentPlanStep {
    pub id: Option<String>,
    pub text: String,
    pub status: AgentPlanStepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentPlanStepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentFilePatch {
    pub path: String,
    pub change_kind: AgentFileChangeKind,
    pub patch: String,
    pub moved_to: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentFileChangeKind {
    Add,
    Update,
    Delete,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentCommandOutput {
    pub stream: AgentCommandOutputStream,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentCommandOutputStream {
    Stdout,
    Stderr,
    Combined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentReviewFinding {
    pub title: String,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub severity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentItemBody {
    UserMessage {
        content: Vec<AgentContentBlock>,
    },
    HookPrompt {
        hook_point: String,
        content: Vec<AgentContentBlock>,
    },
    AgentMessage {
        content: Vec<AgentContentBlock>,
        phase: Option<String>,
    },
    Reasoning {
        summary: Vec<AgentContentBlock>,
        content: Vec<AgentContentBlock>,
    },
    Plan {
        explanation: Option<String>,
        steps: Vec<AgentPlanStep>,
    },
    CommandExecution {
        command: String,
        cwd: Option<String>,
        output: Vec<AgentCommandOutput>,
    },
    FileChange {
        changes: Vec<AgentFilePatch>,
        output: Vec<AgentContentBlock>,
    },
    FileRead {
        path: String,
        line_start: Option<u32>,
        line_end: Option<u32>,
        content: Vec<AgentContentBlock>,
    },
    FileSearch {
        mode: AgentFileSearchMode,
        query: String,
        path: Option<String>,
        matches: Vec<AgentFileSearchMatch>,
    },
    McpToolCall {
        server: String,
        tool: String,
        arguments: Value,
        result: Option<Value>,
        progress: Vec<AgentContentBlock>,
    },
    DynamicToolCall {
        namespace: Option<String>,
        tool: String,
        arguments: Value,
        result: Option<Value>,
        progress: Vec<AgentContentBlock>,
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
        result: Vec<AgentContentBlock>,
    },
    WebSearch {
        action: String,
        query: Option<String>,
        url: Option<String>,
        results: Vec<AgentContentBlock>,
    },
    ImageView {
        path: String,
        detail: Option<String>,
    },
    ImageGeneration {
        prompt: String,
        revised_prompt: Option<String>,
        outputs: Vec<AgentContentBlock>,
    },
    Sleep {
        duration_ms: AgentServiceU64,
    },
    Review {
        findings: Vec<AgentReviewFinding>,
        summary: Option<String>,
    },
    TerminalControl {
        terminal_id: String,
        action: String,
        text: Option<String>,
    },
    ContextCompaction {
        summary: Option<Vec<AgentContentBlock>>,
        source_digest: Option<AgentPayloadDigest>,
    },
    GenericToolActivity {
        name: String,
        arguments: Value,
        result: Option<Value>,
        progress: Vec<AgentContentBlock>,
    },
    Error {
        code: String,
        message: String,
        details: Option<Vec<AgentContentBlock>>,
    },
}

impl AgentItemBody {
    pub fn validate(&self) -> Result<(), AgentPresentationViolation> {
        for block in self.content_blocks() {
            block.validate()?;
        }
        Ok(())
    }

    fn content_blocks(&self) -> Vec<&AgentContentBlock> {
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

    pub fn digest(&self) -> AgentPayloadDigest {
        digest(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentProcessExitEvidence {
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentPresentationError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentItemTerminalEvidence {
    pub outcome: AgentTerminalStatus,
    pub completed_at_ms: Option<AgentServiceU64>,
    pub duration_ms: Option<AgentServiceU64>,
    pub process_exit: Option<AgentProcessExitEvidence>,
    pub error: Option<AgentPresentationError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentTerminalStatus {
    Completed,
    Failed,
    Interrupted,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentItemPresentation {
    pub body: AgentItemBody,
    pub started_at_ms: Option<AgentServiceU64>,
    pub updated_at_ms: Option<AgentServiceU64>,
    pub terminal: Option<AgentItemTerminalEvidence>,
    pub body_digest: AgentPayloadDigest,
    pub presentation_digest: AgentPayloadDigest,
}

impl AgentItemPresentation {
    pub fn new(
        body: AgentItemBody,
        started_at_ms: Option<u64>,
        updated_at_ms: Option<u64>,
        terminal: Option<AgentItemTerminalEvidence>,
    ) -> Result<Self, AgentPresentationViolation> {
        body.validate()?;
        let body_digest = body.digest();
        let mut value = Self {
            body,
            started_at_ms: started_at_ms.map(AgentServiceU64),
            updated_at_ms: updated_at_ms.map(AgentServiceU64),
            terminal,
            body_digest,
            presentation_digest: AgentPayloadDigest::new("pending")
                .expect("fixed non-empty digest"),
        };
        value.presentation_digest = value.calculated_presentation_digest();
        Ok(value)
    }

    pub fn calculated_presentation_digest(&self) -> AgentPayloadDigest {
        digest(&(
            &self.body,
            self.started_at_ms,
            self.updated_at_ms,
            &self.terminal,
        ))
    }

    pub fn validate_for_status(
        &self,
        status: crate::AgentEntityStatus,
    ) -> Result<(), AgentPresentationViolation> {
        self.body.validate()?;
        if self.body_digest != self.body.digest()
            || self.presentation_digest != self.calculated_presentation_digest()
        {
            return Err(AgentPresentationViolation::DigestMismatch);
        }
        let terminal = matches!(
            status,
            crate::AgentEntityStatus::Completed
                | crate::AgentEntityStatus::Failed
                | crate::AgentEntityStatus::Interrupted
                | crate::AgentEntityStatus::Lost
        );
        if terminal != self.terminal.is_some() {
            return Err(AgentPresentationViolation::TerminalEvidenceMismatch);
        }
        if let Some(evidence) = &self.terminal {
            let expected = match status {
                crate::AgentEntityStatus::Completed => AgentTerminalStatus::Completed,
                crate::AgentEntityStatus::Failed => AgentTerminalStatus::Failed,
                crate::AgentEntityStatus::Interrupted => AgentTerminalStatus::Interrupted,
                crate::AgentEntityStatus::Lost => AgentTerminalStatus::Lost,
                crate::AgentEntityStatus::Accepted | crate::AgentEntityStatus::Running => {
                    unreachable!("terminal evidence was rejected above")
                }
            };
            if evidence.outcome != expected {
                return Err(AgentPresentationViolation::TerminalEvidenceMismatch);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentItemUpdate {
    TextAppended {
        text: String,
    },
    ReasoningAppended {
        text: String,
    },
    ContentAppended {
        content: Vec<AgentContentBlock>,
    },
    CommandOutputAppended {
        output: AgentCommandOutput,
    },
    PatchChanged {
        changes: Vec<AgentFilePatch>,
    },
    PlanChanged {
        explanation: Option<String>,
        steps: Vec<AgentPlanStep>,
    },
    ToolProgress {
        content: Vec<AgentContentBlock>,
    },
    CollaborationChanged {
        status: String,
        result: Option<Value>,
    },
    BodyReplaced {
        body: AgentItemBody,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum AgentItemTransition {
    Started {
        presentation: AgentItemPresentation,
    },
    Updated {
        update: AgentItemUpdate,
        presentation: AgentItemPresentation,
    },
    Terminal {
        presentation: AgentItemPresentation,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentInteractionRequest {
    Approval {
        prompt: String,
        reason: Option<String>,
        proposed_action: Option<Value>,
    },
    UserInput {
        prompt: String,
        questions: Vec<AgentInteractionQuestion>,
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
pub struct AgentInteractionQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub allows_free_form: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentInteractionStatus {
    Pending,
    Resolved,
    Cancelled,
    Expired,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentInteractionResolution {
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
pub enum AgentPresentationViolation {
    #[error("structured presentation schemas must be versioned AgentDash-owned schemas")]
    InvalidStructuredSchema,
    #[error("presentation digest does not match canonical body and evidence")]
    DigestMismatch,
    #[error("terminal status and terminal evidence do not agree")]
    TerminalEvidenceMismatch,
}

fn digest<T: Serialize>(value: &T) -> AgentPayloadDigest {
    AgentPayloadDigest::new(
        canonical_json_sha256(value).expect("canonical presentation must serialize"),
    )
    .expect("sha256 digest is non-empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal(outcome: AgentTerminalStatus) -> AgentItemTerminalEvidence {
        AgentItemTerminalEvidence {
            outcome,
            completed_at_ms: Some(AgentServiceU64(u64::MAX)),
            duration_ms: Some(AgentServiceU64(7)),
            process_exit: None,
            error: None,
        }
    }

    #[test]
    fn terminal_and_digest_invariants_are_closed() {
        let running = AgentItemPresentation::new(
            AgentItemBody::AgentMessage {
                content: vec![AgentContentBlock::Text {
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
            .validate_for_status(crate::AgentEntityStatus::Running)
            .expect("running evidence");
        assert_eq!(
            running
                .validate_for_status(crate::AgentEntityStatus::Completed)
                .expect_err("terminal requires evidence"),
            AgentPresentationViolation::TerminalEvidenceMismatch
        );

        let completed = AgentItemPresentation::new(
            AgentItemBody::AgentMessage {
                content: vec![AgentContentBlock::Text {
                    text: "done".to_owned(),
                }],
                phase: None,
            },
            Some(1),
            Some(3),
            Some(terminal(AgentTerminalStatus::Completed)),
        )
        .expect("completed presentation");
        completed
            .validate_for_status(crate::AgentEntityStatus::Completed)
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
        let left = AgentItemBody::DynamicToolCall {
            namespace: Some("dash".to_owned()),
            tool: "lookup".to_owned(),
            arguments: serde_json::json!({"z": {"b": 2, "a": 1}, "a": 0}),
            result: Some(serde_json::json!({"output": {"y": 2, "x": 1}})),
            progress: Vec::new(),
        };
        let right = AgentItemBody::DynamicToolCall {
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
            AgentItemPresentation::new(left, Some(1), Some(2), None)
                .expect("left presentation")
                .presentation_digest,
            AgentItemPresentation::new(right, Some(1), Some(2), None)
                .expect("right presentation")
                .presentation_digest
        );
    }

    #[test]
    fn structured_blocks_require_agentdash_schema_and_version() {
        let invalid = AgentContentBlock::Structured {
            schema: "codex.thread_item".to_owned(),
            schema_version: 1,
            value: serde_json::json!({}),
        };
        assert_eq!(
            invalid
                .validate()
                .expect_err("vendor schema is not canonical"),
            AgentPresentationViolation::InvalidStructuredSchema
        );
    }

    #[test]
    fn every_canonical_item_body_family_round_trips_without_vendor_payloads() {
        let text = || AgentContentBlock::Text {
            text: "value".to_owned(),
        };
        let bodies = vec![
            AgentItemBody::UserMessage {
                content: vec![text()],
            },
            AgentItemBody::HookPrompt {
                hook_point: "before_tool".to_owned(),
                content: vec![text()],
            },
            AgentItemBody::AgentMessage {
                content: vec![text()],
                phase: Some("final".to_owned()),
            },
            AgentItemBody::Reasoning {
                summary: vec![text()],
                content: vec![text()],
            },
            AgentItemBody::Plan {
                explanation: Some("plan".to_owned()),
                steps: vec![AgentPlanStep {
                    id: Some("step-1".to_owned()),
                    text: "work".to_owned(),
                    status: AgentPlanStepStatus::InProgress,
                }],
            },
            AgentItemBody::CommandExecution {
                command: "cargo test".to_owned(),
                cwd: Some("repo".to_owned()),
                output: vec![AgentCommandOutput {
                    stream: AgentCommandOutputStream::Stdout,
                    text: "ok".to_owned(),
                }],
            },
            AgentItemBody::FileChange {
                changes: vec![AgentFilePatch {
                    path: "src/lib.rs".to_owned(),
                    change_kind: AgentFileChangeKind::Update,
                    patch: "@@".to_owned(),
                    moved_to: None,
                }],
                output: vec![text()],
            },
            AgentItemBody::FileRead {
                path: "src/lib.rs".to_owned(),
                line_start: Some(1),
                line_end: Some(2),
                content: vec![text()],
            },
            AgentItemBody::FileSearch {
                mode: AgentFileSearchMode::Grep,
                query: "needle".to_owned(),
                path: Some("src".to_owned()),
                matches: vec![AgentFileSearchMatch {
                    path: "src/lib.rs".to_owned(),
                    line: Some(1),
                    column: Some(2),
                    preview: Some("needle".to_owned()),
                }],
            },
            AgentItemBody::McpToolCall {
                server: "server".to_owned(),
                tool: "tool".to_owned(),
                arguments: serde_json::json!({"input": 1}),
                result: Some(serde_json::json!({"output": 2})),
                progress: vec![text()],
            },
            AgentItemBody::DynamicToolCall {
                namespace: Some("namespace".to_owned()),
                tool: "tool".to_owned(),
                arguments: serde_json::json!({"input": 1}),
                result: Some(serde_json::json!({"output": 2})),
                progress: vec![text()],
            },
            AgentItemBody::CollaborationToolCall {
                action: "spawn".to_owned(),
                target: Some("agent-1".to_owned()),
                prompt: Some("work".to_owned()),
                result: Some(serde_json::json!({"status": "done"})),
            },
            AgentItemBody::SubagentActivity {
                agent_id: "agent-1".to_owned(),
                task: "work".to_owned(),
                status: "completed".to_owned(),
                result: vec![text()],
            },
            AgentItemBody::WebSearch {
                action: "search".to_owned(),
                query: Some("query".to_owned()),
                url: None,
                results: vec![text()],
            },
            AgentItemBody::ImageView {
                path: "image.png".to_owned(),
                detail: Some("high".to_owned()),
            },
            AgentItemBody::ImageGeneration {
                prompt: "image".to_owned(),
                revised_prompt: Some("better image".to_owned()),
                outputs: vec![text()],
            },
            AgentItemBody::Sleep {
                duration_ms: AgentServiceU64(10),
            },
            AgentItemBody::Review {
                findings: vec![AgentReviewFinding {
                    title: "finding".to_owned(),
                    body: "body".to_owned(),
                    path: Some("src/lib.rs".to_owned()),
                    line: Some(1),
                    severity: Some("high".to_owned()),
                }],
                summary: Some("summary".to_owned()),
            },
            AgentItemBody::TerminalControl {
                terminal_id: "terminal-1".to_owned(),
                action: "write".to_owned(),
                text: Some("input".to_owned()),
            },
            AgentItemBody::ContextCompaction {
                summary: Some(vec![text()]),
                source_digest: Some(AgentPayloadDigest::new("sha256:source").expect("digest")),
            },
            AgentItemBody::GenericToolActivity {
                name: "tool".to_owned(),
                arguments: serde_json::json!({"input": 1}),
                result: Some(serde_json::json!({"output": 2})),
                progress: vec![text()],
            },
            AgentItemBody::Error {
                code: "failure".to_owned(),
                message: "failed".to_owned(),
                details: Some(vec![text()]),
            },
        ];

        for body in bodies {
            body.validate().expect("canonical body");
            let encoded = serde_json::to_value(&body).expect("serialize");
            let decoded: AgentItemBody = serde_json::from_value(encoded).expect("deserialize");
            assert_eq!(decoded, body);
        }
    }
}
