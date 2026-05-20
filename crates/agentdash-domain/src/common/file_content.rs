use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoredFileContent {
    Text { content: String },
    Binary { bytes: Vec<u8>, mime_type: String },
}

impl StoredFileContent {
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text {
            content: content.into(),
        }
    }

    pub fn binary(bytes: Vec<u8>, mime_type: impl Into<String>) -> Self {
        Self::Binary {
            bytes,
            mime_type: mime_type.into(),
        }
    }

    pub fn kind(&self) -> StoredFileContentKind {
        match self {
            Self::Text { .. } => StoredFileContentKind::Text,
            Self::Binary { .. } => StoredFileContentKind::Binary,
        }
    }

    pub fn text_content(&self) -> Option<&str> {
        match self {
            Self::Text { content } => Some(content),
            Self::Binary { .. } => None,
        }
    }

    pub fn into_text(self) -> Option<String> {
        match self {
            Self::Text { content } => Some(content),
            Self::Binary { .. } => None,
        }
    }

    pub fn binary_content(&self) -> Option<&[u8]> {
        match self {
            Self::Text { .. } => None,
            Self::Binary { bytes, .. } => Some(bytes),
        }
    }

    pub fn mime_type(&self) -> Option<&str> {
        match self {
            Self::Text { .. } => None,
            Self::Binary { mime_type, .. } => Some(mime_type),
        }
    }

    pub fn size_bytes(&self) -> u64 {
        match self {
            Self::Text { content } => content.len() as u64,
            Self::Binary { bytes, .. } => bytes.len() as u64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredFileContentKind {
    Text,
    Binary,
}

impl StoredFileContentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Binary => "binary",
        }
    }
}

impl std::str::FromStr for StoredFileContentKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Self::Text),
            "binary" => Ok(Self::Binary),
            _ => Err(()),
        }
    }
}
