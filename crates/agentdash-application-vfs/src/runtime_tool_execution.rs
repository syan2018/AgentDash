use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfsToolExecutionResult {
    pub content: Vec<VfsToolContent>,
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl VfsToolExecutionResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![VfsToolContent::text(text)],
            is_error: false,
            details: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VfsToolContent {
    Text { text: String },
    Image { mime_type: String, data: String },
}

impl VfsToolContent {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn extract_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            Self::Image { .. } => None,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum VfsToolExecutionError {
    #[error("invalid VFS tool arguments: {0}")]
    InvalidArguments(String),
    #[error("VFS tool execution failed: {0}")]
    ExecutionFailed(String),
    #[error("VFS tool execution was cancelled")]
    Cancelled,
}

pub type VfsToolUpdateSink = Arc<dyn Fn(VfsToolExecutionResult) + Send + Sync>;
