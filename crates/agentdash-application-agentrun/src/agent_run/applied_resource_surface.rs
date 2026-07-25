use std::collections::BTreeSet;

use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn is_canonical_relative_vfs_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '\0'])
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppliedVfsOperation {
    Read,
    List,
    Search,
    Write,
    Exec,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "snake_case")]
pub enum AppliedVfsPathScope {
    All,
    Exact(String),
    Prefix(String),
}

impl AppliedVfsPathScope {
    pub fn allows(&self, relative_path: &str) -> bool {
        if !is_canonical_relative_vfs_path(relative_path) {
            return false;
        }
        match self {
            Self::All => true,
            Self::Exact(path) => is_canonical_relative_vfs_path(path) && relative_path == path,
            Self::Prefix(prefix) => {
                is_canonical_relative_vfs_path(prefix)
                    && (relative_path == prefix
                        || relative_path
                            .strip_prefix(prefix)
                            .is_some_and(|suffix| suffix.starts_with('/')))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedVfsGrant {
    pub mount_id: String,
    pub operations: BTreeSet<AppliedVfsOperation>,
    pub path_scopes: Vec<AppliedVfsPathScope>,
}

impl AppliedVfsGrant {
    pub fn grants_operation_on_path(
        &self,
        operation: AppliedVfsOperation,
        relative_path: &str,
    ) -> bool {
        self.operations.contains(&operation)
            && self
                .path_scopes
                .iter()
                .any(|scope| scope.allows(relative_path))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedVfsMount {
    pub mount_id: String,
    pub provider: String,
    pub backend_id: String,
    pub root_ref: String,
    pub capabilities: BTreeSet<AppliedVfsOperation>,
    pub default_write: bool,
    pub display_name: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppliedTaskOperation {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppliedTaskScope {
    Project { project_id: Uuid },
    Task { project_id: Uuid, task_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedTaskGrant {
    pub scope: AppliedTaskScope,
    pub operations: BTreeSet<AppliedTaskOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunAppliedResourceSurfaceProvenance {
    pub source_kind: String,
    pub source_id: String,
    pub source_revision: u64,
    pub projection_revision: u64,
    pub captured_at_ms: u64,
}

/// Product-owned runtime authority derived from the binding-pinned AgentFrame and current Product
/// relationships. It is a transient value object, not a durable projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunAppliedResourceSurface {
    pub target: AgentRunTarget,
    pub project_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub vfs_mounts: Vec<AppliedVfsMount>,
    pub default_mount_id: Option<String>,
    pub vfs_grants: Vec<AppliedVfsGrant>,
    pub agent_surface_revision: u64,
    pub agent_surface_digest: String,
    pub vfs_digest: String,
    pub task_grants: Vec<AppliedTaskGrant>,
    pub task_surface_digest: String,
    pub product_binding_digest: String,
    pub provenance: AgentRunAppliedResourceSurfaceProvenance,
}

impl AgentRunAppliedResourceSurface {
    pub fn validate_for(
        &self,
        target: &AgentRunTarget,
    ) -> Result<(), AgentRunAppliedResourceSurfaceQueryError> {
        if self.target != *target {
            return Err(AgentRunAppliedResourceSurfaceQueryError::TargetMismatch);
        }
        if self.agent_surface_digest.is_empty()
            || self.vfs_digest.is_empty()
            || self.product_binding_digest.is_empty()
            || self.task_surface_digest.is_empty()
            || self.provenance.source_kind.is_empty()
            || self.provenance.source_id.is_empty()
        {
            return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                message: "surface digest and provenance identity must be non-empty".to_string(),
            });
        }
        let mut mount_ids = BTreeSet::new();
        for mount in &self.vfs_mounts {
            if mount.mount_id.is_empty()
                || mount.provider.is_empty()
                || mount.root_ref.is_empty()
                || mount.capabilities.is_empty()
                || !mount_ids.insert(mount.mount_id.as_str())
            {
                return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                    message:
                        "VFS mounts must have unique identities, roots and explicit capabilities"
                            .to_string(),
                });
            }
        }
        if self
            .default_mount_id
            .as_deref()
            .is_some_and(|mount_id| !mount_ids.contains(mount_id))
        {
            return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                message: "default mount must reference a VFS mount".to_string(),
            });
        }
        for grant in &self.vfs_grants {
            if grant.mount_id.is_empty()
                || grant.operations.is_empty()
                || grant.path_scopes.is_empty()
            {
                return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                    message: format!(
                        "mount grant `{}` must declare explicit operations and path scopes",
                        grant.mount_id
                    ),
                });
            }
            let Some(mount) = self
                .vfs_mounts
                .iter()
                .find(|mount| mount.mount_id == grant.mount_id)
            else {
                return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                    message: format!(
                        "mount grant `{}` does not reference a VFS mount",
                        grant.mount_id
                    ),
                });
            };
            if !grant.operations.is_subset(&mount.capabilities) {
                return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                    message: format!(
                        "mount grant `{}` exceeds mount capabilities",
                        grant.mount_id
                    ),
                });
            }
            for scope in &grant.path_scopes {
                match scope {
                    AppliedVfsPathScope::All => {}
                    AppliedVfsPathScope::Exact(path) | AppliedVfsPathScope::Prefix(path)
                        if !is_canonical_relative_vfs_path(path) =>
                    {
                        return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                            message: format!(
                                "mount grant `{}` contains a non-canonical relative path scope",
                                grant.mount_id
                            ),
                        });
                    }
                    AppliedVfsPathScope::Exact(_) | AppliedVfsPathScope::Prefix(_) => {}
                }
            }
        }

        let mut task_scopes = BTreeSet::new();
        for grant in &self.task_grants {
            let project_id = match grant.scope {
                AppliedTaskScope::Project { project_id }
                | AppliedTaskScope::Task { project_id, .. } => project_id,
            };
            if project_id != self.project_id
                || grant.operations.is_empty()
                || !task_scopes.insert(grant.scope.clone())
            {
                return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                    message:
                        "Task grants must declare unique Product-owned scopes and explicit operations"
                            .to_string(),
                });
            }
        }
        Ok(())
    }

    pub fn grants_task_operation(
        &self,
        scope: &AppliedTaskScope,
        operation: AppliedTaskOperation,
    ) -> bool {
        self.task_grants
            .iter()
            .find(|grant| &grant.scope == scope)
            .is_some_and(|grant| grant.operations.contains(&operation))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunAppliedResourceSurfaceQueryError {
    #[error("AgentRun resource surface facts are missing")]
    MissingFacts,
    #[error("resource surface target does not match the requested AgentRun target")]
    TargetMismatch,
    #[error("resource surface facts conflict: {message}")]
    Conflict { message: String },
    #[error("resource surface evidence is corrupt: {message}")]
    CorruptEvidence { message: String },
    #[error("resource surface fact query failed: {message}")]
    Repository { message: String },
}

#[async_trait]
pub trait AgentRunAppliedResourceSurfaceQueryPort: Send + Sync {
    async fn applied_resource_surface(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError>;

    async fn applied_resource_surface_at(
        &self,
        target: &AgentRunTarget,
        agent_surface_revision: u64,
    ) -> Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface_with_mount(mount: AppliedVfsMount) -> AgentRunAppliedResourceSurface {
        let target = AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let project_id = Uuid::new_v4();
        let grant = AppliedVfsGrant {
            mount_id: mount.mount_id.clone(),
            operations: mount.capabilities.clone(),
            path_scopes: vec![AppliedVfsPathScope::All],
        };
        AgentRunAppliedResourceSurface {
            target,
            project_id,
            workspace_id: None,
            vfs_mounts: vec![mount],
            default_mount_id: None,
            vfs_grants: vec![grant],
            agent_surface_revision: 1,
            agent_surface_digest: "agent-surface".to_string(),
            vfs_digest: "vfs-surface".to_string(),
            task_grants: Vec::new(),
            task_surface_digest: "task-surface".to_string(),
            product_binding_digest: "product-binding".to_string(),
            provenance: AgentRunAppliedResourceSurfaceProvenance {
                source_kind: "agent_frame".to_string(),
                source_id: Uuid::new_v4().to_string(),
                source_revision: 1,
                projection_revision: 1,
                captured_at_ms: 1,
            },
        }
    }

    #[test]
    fn path_scope_requires_canonical_relative_paths() {
        assert!(AppliedVfsPathScope::All.allows("src/main.rs"));
        assert!(AppliedVfsPathScope::Prefix("src".to_string()).allows("src/main.rs"));
        assert!(AppliedVfsPathScope::Exact("src/main.rs".to_string()).allows("src/main.rs"));
        assert!(!AppliedVfsPathScope::All.allows("../secret"));
        assert!(!AppliedVfsPathScope::Prefix("src".to_string()).allows("src/../secret"));
    }

    #[test]
    fn backendless_logical_mount_is_valid_surface_evidence() {
        let surface = surface_with_mount(AppliedVfsMount {
            mount_id: "canvas:verification".to_string(),
            provider: "canvas_fs".to_string(),
            backend_id: String::new(),
            root_ref: "canvas://verification".to_string(),
            capabilities: BTreeSet::from([AppliedVfsOperation::Read, AppliedVfsOperation::List]),
            default_write: false,
            display_name: "Verification".to_string(),
            metadata: serde_json::json!({}),
        });

        assert_eq!(surface.validate_for(&surface.target), Ok(()));
    }
}
