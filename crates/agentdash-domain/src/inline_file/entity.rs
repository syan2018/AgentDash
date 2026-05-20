use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 内联文件 — inline_fs 的独立存储实体
///
/// 统一存储所有「文件内容嵌套在父实体」的场景：
/// - Context Container inline files（owner_kind = "project" / "story"）
/// - Lifecycle VFS port outputs（owner_kind = "lifecycle_run"）
/// - Lifecycle record artifact content（owner_kind = "lifecycle_run"）
/// - Agent Knowledge files（owner_kind = "project_agent"）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineFile {
    pub id: Uuid,
    /// 归属实体类型
    pub owner_kind: InlineFileOwnerKind,
    /// 归属实体 ID
    pub owner_id: Uuid,
    /// 容器标识（对应 ContextContainerDefinition.id，或 "port_outputs" / "record_artifacts"）
    pub container_id: String,
    /// 归一化文件路径
    pub path: String,
    /// 文件内容
    pub content: InlineFileContent,
    /// 文件大小（字节）
    pub size_bytes: u64,
    pub updated_at: DateTime<Utc>,
}

impl InlineFile {
    pub fn new(
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: impl Into<String>,
        path: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::new_text(owner_kind, owner_id, container_id, path, content)
    }

    pub fn new_text(
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: impl Into<String>,
        path: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let content = content.into();
        let size_bytes = content.len() as u64;
        Self {
            id: Uuid::new_v4(),
            owner_kind,
            owner_id,
            container_id: container_id.into(),
            path: path.into(),
            content: InlineFileContent::Text { content },
            size_bytes,
            updated_at: Utc::now(),
        }
    }

    pub fn new_binary(
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: impl Into<String>,
        path: impl Into<String>,
        bytes: Vec<u8>,
        mime_type: impl Into<String>,
    ) -> Self {
        let size_bytes = bytes.len() as u64;
        Self {
            id: Uuid::new_v4(),
            owner_kind,
            owner_id,
            container_id: container_id.into(),
            path: path.into(),
            content: InlineFileContent::Binary {
                bytes,
                mime_type: mime_type.into(),
            },
            size_bytes,
            updated_at: Utc::now(),
        }
    }

    pub fn content_kind(&self) -> InlineFileContentKind {
        self.content.kind()
    }

    pub fn content_kind_str(&self) -> &'static str {
        self.content.kind().as_str()
    }

    pub fn text_content(&self) -> Option<&str> {
        match &self.content {
            InlineFileContent::Text { content } => Some(content),
            InlineFileContent::Binary { .. } => None,
        }
    }

    pub fn into_text_content(self) -> Option<String> {
        match self.content {
            InlineFileContent::Text { content } => Some(content),
            InlineFileContent::Binary { .. } => None,
        }
    }

    pub fn binary_content(&self) -> Option<&[u8]> {
        match &self.content {
            InlineFileContent::Text { .. } => None,
            InlineFileContent::Binary { bytes, .. } => Some(bytes),
        }
    }

    pub fn mime_type(&self) -> Option<&str> {
        match &self.content {
            InlineFileContent::Text { .. } => None,
            InlineFileContent::Binary { mime_type, .. } => Some(mime_type),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InlineFileContent {
    Text { content: String },
    Binary { bytes: Vec<u8>, mime_type: String },
}

impl InlineFileContent {
    pub fn kind(&self) -> InlineFileContentKind {
        match self {
            Self::Text { .. } => InlineFileContentKind::Text,
            Self::Binary { .. } => InlineFileContentKind::Binary,
        }
    }

    pub fn into_text(self) -> Option<String> {
        match self {
            Self::Text { content } => Some(content),
            Self::Binary { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InlineFileContentKind {
    Text,
    Binary,
}

impl InlineFileContentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Binary => "binary",
        }
    }
}

impl std::str::FromStr for InlineFileContentKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Self::Text),
            "binary" => Ok(Self::Binary),
            _ => Err(()),
        }
    }
}

/// 内联文件归属类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InlineFileOwnerKind {
    /// Project 级 context container
    Project,
    /// Story 级 context container
    Story,
    /// Lifecycle run 的 port outputs / record artifacts
    LifecycleRun,
    /// ProjectAgent 级 knowledge container
    ProjectAgent,
}

impl InlineFileOwnerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Story => "story",
            Self::LifecycleRun => "lifecycle_run",
            Self::ProjectAgent => "project_agent",
        }
    }
}

impl std::str::FromStr for InlineFileOwnerKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "project" => Ok(Self::Project),
            "story" => Ok(Self::Story),
            "lifecycle_run" => Ok(Self::LifecycleRun),
            "project_agent" => Ok(Self::ProjectAgent),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for InlineFileOwnerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
