use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InteractionError {
    #[error("Interaction 实体不存在: {entity} ({id})")]
    NotFound { entity: &'static str, id: String },

    #[error("Interaction 字段无效: {field}: {reason}")]
    InvalidField {
        field: &'static str,
        reason: &'static str,
    },

    #[error("source path 无效: {path}: {reason}")]
    InvalidSourcePath { path: String, reason: &'static str },

    #[error("source bundle entry 不存在: {entry_file}")]
    MissingEntryFile { entry_file: String },

    #[error("摘要无效: {field}")]
    InvalidDigest { field: &'static str },

    #[error(
        "definition revision 冲突: definition={definition_id}, expected={expected_revision_id}, actual={actual_revision_id}"
    )]
    DefinitionRevisionConflict {
        definition_id: Uuid,
        expected_revision_id: Uuid,
        actual_revision_id: Uuid,
    },

    #[error(
        "instance state revision 冲突: instance={instance_id}, expected={expected}, actual={actual}"
    )]
    StateRevisionConflict {
        instance_id: Uuid,
        expected: u64,
        actual: u64,
    },

    #[error("command idempotency 冲突: instance={instance_id}, command={command_id}")]
    CommandIdempotencyConflict { instance_id: Uuid, command_id: Uuid },

    #[error("Agent 无权执行 human_only command: {command_type}")]
    HumanOnlyCommand { command_type: String },

    #[error("state patch 操作数超限: actual={actual}, maximum={maximum}")]
    PatchLimitExceeded { actual: usize, maximum: usize },

    #[error("state patch path 不在 allowlist: {path}")]
    PatchPathDenied { path: String },

    #[error("state patch value 缺失: operation={operation}, path={path}")]
    MissingPatchValue {
        operation: &'static str,
        path: String,
    },

    #[error("state patch remove 禁止携带 value: path={path}")]
    UnexpectedPatchValue { path: String },

    #[error("canonical state 大小超限: actual={actual_bytes}, maximum={maximum_bytes}")]
    StateSizeExceeded {
        actual_bytes: usize,
        maximum_bytes: usize,
    },

    #[error("Interaction 状态迁移非法: {from} -> {to}")]
    InvalidStatusTransition {
        from: &'static str,
        to: &'static str,
    },

    #[error("可靠副作用必须引用 replay-safe 或 idempotent Operation")]
    EffectNotReplaySafe,

    #[error("OperationRef 无效: {reason}")]
    InvalidOperationRef { reason: String },

    #[error("Interaction 序列化失败: {context}")]
    Serialization {
        context: &'static str,
        message: String,
    },

    #[error("Interaction persistence 失败: {operation}")]
    Persistence {
        operation: &'static str,
        message: String,
    },

    #[error("Interaction persistence 约束冲突: {entity}.{constraint}")]
    PersistenceConflict {
        entity: &'static str,
        constraint: String,
    },
}

pub type InteractionResult<T> = Result<T, InteractionError>;
