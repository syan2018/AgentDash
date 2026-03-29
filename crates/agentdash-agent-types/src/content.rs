use serde::{Deserialize, Serialize};

/// 内容片段 — 跨层共享的最小内容单元。
/// 支持 Text、Image、Reasoning 三种载体，覆盖主流 LLM 的输出模式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Image {
        mime_type: String,
        data: String,
    },
    Reasoning {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

impl ContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn reasoning(
        text: impl Into<String>,
        id: Option<String>,
        signature: Option<String>,
    ) -> Self {
        Self::Reasoning {
            text: text.into(),
            id,
            signature,
        }
    }

    pub fn extract_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }

    pub fn extract_reasoning(&self) -> Option<&str> {
        match self {
            Self::Reasoning { text, .. } => Some(text),
            _ => None,
        }
    }
}
