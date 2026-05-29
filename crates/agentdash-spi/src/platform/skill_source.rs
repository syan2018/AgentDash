//! SPI port for importing skill assets from remote providers
//! (GitHub / ClawHub / skills.sh).
//!
//! The concrete HTTP traversal and provider-specific API logic live in
//! infrastructure. Application owns content typing (`content_from_bytes`),
//! validation, and persistence — it depends only on this port for the network
//! fetch, receiving raw file bodies it then types itself.

use async_trait::async_trait;

/// Which remote provider a fetched skill originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSkillKind {
    Github,
    Clawhub,
    SkillsSh,
}

/// Raw body of a fetched remote skill file.
///
/// Kept untyped so the application can apply its own content-typing rules.
/// `Bytes` is fed through the application's `content_from_bytes`; `Text` is
/// already known to be UTF-8 text (e.g. ClawHub responses fetched as text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteSkillFileBody {
    Bytes(Vec<u8>),
    Text(String),
}

/// A single file fetched from a remote skill source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSkillFile {
    pub path: String,
    pub body: RemoteSkillFileBody,
}

/// Result of fetching a remote skill: the detected provider, the normalized
/// source URL to record, and the fetched files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSkillFetch {
    pub kind: RemoteSkillKind,
    pub normalized_url: String,
    pub files: Vec<RemoteSkillFile>,
}

/// Error surfaced by a [`RemoteSkillSource`].
#[derive(Debug, Clone)]
pub enum RemoteSkillSourceError {
    /// Invalid input / unsupported source / remote returned an error the user
    /// should be able to correct.
    BadRequest(String),
    /// Unexpected internal failure (e.g. client construction).
    Internal(String),
}

impl std::fmt::Display for RemoteSkillSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(message) | Self::Internal(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RemoteSkillSourceError {}

/// Fetches skill files from a remote provider given a user-supplied URL.
#[async_trait]
pub trait RemoteSkillSource: Send + Sync {
    async fn fetch(&self, url: &str) -> Result<RemoteSkillFetch, RemoteSkillSourceError>;
}
