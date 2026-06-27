use std::collections::HashSet;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::common::MountCapability;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryDiscoveryOwnerKind {
    #[default]
    Unknown,
    Project,
    Story,
    Task,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryDiscoveryUserContext {
    pub user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryDiscoveryContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_identity_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_identity_payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_binding_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub owner_kind: MemoryDiscoveryOwnerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<MemoryDiscoveryUserContext>,
}

/// Runtime mount summary passed to memory discovery providers.
///
/// This intentionally omits `root_ref` and `backend_id`; providers receive only
/// controlled mount identity, capability, and metadata summaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryDiscoveryMount {
    pub mount_id: String,
    pub provider: String,
    pub display_name: String,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_summary: Option<Value>,
}

impl MemoryDiscoveryMount {
    pub fn new(
        mount_id: impl Into<String>,
        provider: impl Into<String>,
        display_name: impl Into<String>,
        capabilities: Vec<MountCapability>,
    ) -> Self {
        Self {
            mount_id: mount_id.into(),
            provider: provider.into(),
            display_name: display_name.into(),
            capabilities,
            purpose: None,
            owner_kind: None,
            metadata_summary: None,
        }
    }
}

/// VFS file discovery rule declared by a memory discovery provider.
///
/// Rules describe what the host may read from already-active VFS mounts. They
/// do not grant filesystem access and do not expand mount capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryDiscoveryVfsRule {
    pub key: String,
    #[serde(default)]
    pub file_names: Vec<String>,
    #[serde(default)]
    pub exact_paths: Vec<String>,
    #[serde(default)]
    pub scan_prefixes: Vec<String>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_files: Option<usize>,
    pub max_size_bytes: u64,
}

impl MemoryDiscoveryVfsRule {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            file_names: Vec::new(),
            exact_paths: Vec::new(),
            scan_prefixes: Vec::new(),
            recursive: false,
            max_depth: None,
            max_files: None,
            max_size_bytes: 32 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryDiscoveryVfsFile {
    pub rule_key: String,
    pub mount_id: String,
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySourceScope {
    Agent,
    Project,
    User,
    External,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySourceFormat {
    #[default]
    #[serde(rename = "agentdash")]
    AgentDash,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryIndexStatus {
    #[default]
    Missing,
    Present,
    TooLarge,
    Invalid,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySourceTrustLevel {
    #[default]
    FirstParty,
    Organization,
    User,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DiscoveredMemorySource {
    pub provider_key: String,
    pub source_key: String,
    pub display_name: String,
    pub source_uri: String,
    pub index_uri: String,
    pub mount_id: String,
    pub scope: MemorySourceScope,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    #[serde(default)]
    pub format: MemorySourceFormat,
    #[serde(default)]
    pub index_status: MemoryIndexStatus,
    #[serde(default)]
    pub trust_level: MemorySourceTrustLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounded_index_content: Option<String>,
}

impl DiscoveredMemorySource {
    pub fn has_controlled_uris(&self) -> bool {
        is_controlled_vfs_memory_uri(&self.source_uri, true)
            && is_controlled_vfs_memory_uri(&self.index_uri, false)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryDiscoveryCluster {
    pub provider_key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_count: Option<usize>,
    #[serde(default)]
    pub sources: Vec<DiscoveredMemorySource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryDiscoveryDiagnostic {
    pub provider_key: String,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MemoryDiscoveryOutput {
    #[serde(default)]
    pub clusters: Vec<MemoryDiscoveryCluster>,
    #[serde(default)]
    pub diagnostics: Vec<MemoryDiscoveryDiagnostic>,
}

impl MemoryDiscoveryOutput {
    pub fn normalized(self, fallback_provider_key: &str) -> Self {
        let mut diagnostics = self.diagnostics;
        let mut seen_sources = HashSet::new();
        let clusters = self
            .clusters
            .into_iter()
            .map(|mut cluster| {
                let cluster_provider_key =
                    normalize_provider_key(&cluster.provider_key, fallback_provider_key);
                let mut kept_sources = Vec::new();

                for mut source in cluster.sources {
                    let provider_key =
                        normalize_provider_key(&source.provider_key, &cluster_provider_key);
                    source.provider_key = provider_key.clone();
                    source.source_key = source.source_key.trim().to_string();

                    if source.source_key.is_empty() {
                        diagnostics.push(MemoryDiscoveryDiagnostic {
                            provider_key,
                            code: "empty_source_key".to_string(),
                            message: "memory source_key must not be empty".to_string(),
                            source_key: None,
                            uri: Some(source.source_uri),
                        });
                        continue;
                    }

                    if !source.has_controlled_uris() {
                        diagnostics.push(MemoryDiscoveryDiagnostic {
                            provider_key,
                            code: "invalid_memory_source_uri".to_string(),
                            message: format!(
                                "memory source `{}` returned a non-controlled VFS URI",
                                source.source_key
                            ),
                            source_key: Some(source.source_key),
                            uri: Some(source.source_uri),
                        });
                        continue;
                    }

                    if !seen_sources.insert((provider_key.clone(), source.source_key.clone())) {
                        diagnostics.push(MemoryDiscoveryDiagnostic {
                            provider_key,
                            code: "duplicate_source_key".to_string(),
                            message: format!(
                                "memory source `{}` is duplicated within provider output; keeping the first item",
                                source.source_key
                            ),
                            source_key: Some(source.source_key),
                            uri: Some(source.source_uri),
                        });
                        continue;
                    }

                    kept_sources.push(source);
                }

                cluster.provider_key = cluster_provider_key;
                cluster.sources = kept_sources;
                cluster
            })
            .collect();

        Self {
            clusters,
            diagnostics,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryDiscoveryError {
    #[error("memory discovery provider `{provider_key}` failed: {message}")]
    ProviderFailed {
        provider_key: String,
        message: String,
    },
}

#[async_trait]
pub trait MemoryDiscoveryProvider: Send + Sync {
    fn provider_key(&self) -> &str;

    fn vfs_discovery_rules(&self) -> Vec<MemoryDiscoveryVfsRule> {
        Vec::new()
    }

    async fn discover_from_vfs(
        &self,
        context: MemoryDiscoveryContext,
        _mounts: Vec<MemoryDiscoveryMount>,
        _files: Vec<MemoryDiscoveryVfsFile>,
    ) -> Result<MemoryDiscoveryOutput, MemoryDiscoveryError> {
        self.discover(context).await
    }

    async fn discover(
        &self,
        _context: MemoryDiscoveryContext,
    ) -> Result<MemoryDiscoveryOutput, MemoryDiscoveryError> {
        Ok(MemoryDiscoveryOutput::default())
    }
}

pub fn is_controlled_vfs_memory_uri(uri: &str, allow_empty_tail: bool) -> bool {
    let Some((scheme, tail)) = uri.trim().split_once("://") else {
        return false;
    };
    let scheme = scheme.trim();
    if scheme.is_empty()
        || scheme.eq_ignore_ascii_case("file")
        || (scheme.len() == 1 && scheme.chars().all(|ch| ch.is_ascii_alphabetic()))
    {
        return false;
    }

    if tail.is_empty() {
        return allow_empty_tail;
    }
    if tail.starts_with('/') || tail.starts_with('\\') || tail.contains('\\') {
        return false;
    }
    if tail.len() >= 2 && tail.as_bytes()[1] == b':' && tail.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }

    !tail.split('/').any(|segment| segment == "..")
}

fn normalize_provider_key(raw: &str, fallback_provider_key: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback_provider_key.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DefaultMemoryDiscoveryProvider;

    #[async_trait]
    impl MemoryDiscoveryProvider for DefaultMemoryDiscoveryProvider {
        fn provider_key(&self) -> &str {
            "test.default"
        }
    }

    fn source(source_key: &str, source_uri: &str, index_uri: &str) -> DiscoveredMemorySource {
        DiscoveredMemorySource {
            provider_key: String::new(),
            source_key: source_key.to_string(),
            display_name: "Agent Memory".to_string(),
            source_uri: source_uri.to_string(),
            index_uri: index_uri.to_string(),
            mount_id: "agent".to_string(),
            scope: MemorySourceScope::Agent,
            capabilities: vec![MountCapability::Read, MountCapability::Write],
            format: MemorySourceFormat::AgentDash,
            index_status: MemoryIndexStatus::Missing,
            trust_level: MemorySourceTrustLevel::FirstParty,
            summary: None,
            bounded_index_content: None,
        }
    }

    #[test]
    fn vfs_rule_defaults_are_bounded() {
        let rule = MemoryDiscoveryVfsRule::new("memory-index");

        assert_eq!(rule.key, "memory-index");
        assert!(rule.file_names.is_empty());
        assert!(rule.exact_paths.is_empty());
        assert!(rule.scan_prefixes.is_empty());
        assert!(!rule.recursive);
        assert_eq!(rule.max_depth, None);
        assert_eq!(rule.max_files, None);
        assert_eq!(rule.max_size_bytes, 32 * 1024);
    }

    #[tokio::test]
    async fn provider_defaults_return_empty_output() {
        let provider = DefaultMemoryDiscoveryProvider;

        assert!(provider.vfs_discovery_rules().is_empty());
        let output = provider
            .discover_from_vfs(
                MemoryDiscoveryContext::default(),
                vec![MemoryDiscoveryMount::new(
                    "agent",
                    "inline_fs",
                    "Agent Memory",
                    vec![MountCapability::Read],
                )],
                Vec::new(),
            )
            .await
            .expect("default discovery");

        assert!(output.clusters.is_empty());
        assert!(output.diagnostics.is_empty());
    }

    #[test]
    fn controlled_vfs_memory_uri_validation_rejects_local_path_shapes() {
        assert!(is_controlled_vfs_memory_uri("agent://", true));
        assert!(is_controlled_vfs_memory_uri("agent://MEMORY.md", false));
        assert!(is_controlled_vfs_memory_uri(
            "agent://topics/project.md",
            false
        ));
        assert!(!is_controlled_vfs_memory_uri("agent://", false));
        assert!(!is_controlled_vfs_memory_uri(
            "file:///tmp/MEMORY.md",
            false
        ));
        assert!(!is_controlled_vfs_memory_uri(
            "agent:///tmp/MEMORY.md",
            false
        ));
        assert!(!is_controlled_vfs_memory_uri(
            "agent://C:/tmp/MEMORY.md",
            false
        ));
        assert!(!is_controlled_vfs_memory_uri(
            "agent://topics\\project.md",
            false
        ));
        assert!(!is_controlled_vfs_memory_uri(
            "agent://topics/../MEMORY.md",
            false
        ));
    }

    #[test]
    fn normalized_output_fills_provider_key_and_drops_duplicate_sources() {
        let output = MemoryDiscoveryOutput {
            clusters: vec![MemoryDiscoveryCluster {
                provider_key: String::new(),
                display_name: "Test".to_string(),
                sources: vec![
                    source("agent", "agent://", "agent://MEMORY.md"),
                    source("agent", "agent://", "agent://MEMORY.md"),
                    source("bad", "C:\\workspace\\memory", "agent://MEMORY.md"),
                ],
                ..Default::default()
            }],
            diagnostics: Vec::new(),
        };

        let normalized = output.normalized("test.provider");

        assert_eq!(normalized.clusters[0].provider_key, "test.provider");
        assert_eq!(normalized.clusters[0].sources.len(), 1);
        assert_eq!(
            normalized.clusters[0].sources[0].provider_key,
            "test.provider"
        );
        assert_eq!(normalized.diagnostics.len(), 2);
        assert_eq!(normalized.diagnostics[0].code, "duplicate_source_key");
        assert_eq!(normalized.diagnostics[1].code, "invalid_memory_source_uri");
    }
}
