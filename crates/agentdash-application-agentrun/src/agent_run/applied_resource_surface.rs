use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_domain::agent_run_target::AgentRunTarget;
use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppliedVfsOperation {
    Read,
    List,
    Search,
    Write,
    Exec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppliedVfsPathScope {
    All,
    Exact(String),
    Prefix(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedVfsGrant {
    pub mount_id: String,
    pub operations: BTreeSet<AppliedVfsOperation>,
    pub path_scopes: Vec<AppliedVfsPathScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedVfsMount {
    pub mount_id: String,
    pub provider: String,
    pub backend_id: String,
    pub root_ref: String,
    pub capabilities: BTreeSet<AppliedVfsOperation>,
    pub default_write: bool,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppliedTaskOperation {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppliedTaskScope {
    Project { project_id: Uuid },
    Task { project_id: Uuid, task_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedTaskGrant {
    pub scope: AppliedTaskScope,
    pub operations: BTreeSet<AppliedTaskOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunAppliedResourceSurfaceProvenance {
    pub source_kind: String,
    pub source_id: String,
    pub source_revision: u64,
    pub projection_revision: u64,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub task_surface_revision: u64,
    pub task_surface_digest: String,
    pub task_provenance: AgentRunAppliedResourceSurfaceProvenance,
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
            || self.task_provenance.source_kind.is_empty()
            || self.task_provenance.source_id.is_empty()
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
                || mount.backend_id.is_empty()
                || mount.root_ref.is_empty()
                || mount.capabilities.is_empty()
                || !mount_ids.insert(mount.mount_id.as_str())
            {
                return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                    message: "applied VFS mounts must have unique identities, roots and explicit capabilities"
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
                message: "default mount must reference an applied VFS mount".to_string(),
            });
        }
        let mut granted_mount_ids = BTreeSet::new();
        for grant in &self.vfs_grants {
            let applied_mount = self
                .vfs_mounts
                .iter()
                .find(|mount| mount.mount_id == grant.mount_id);
            if grant.mount_id.is_empty()
                || applied_mount.is_none()
                || grant.operations.is_empty()
                || grant.path_scopes.is_empty()
                || !granted_mount_ids.insert(grant.mount_id.as_str())
            {
                return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                    message: format!(
                        "mount grant `{}` must reference one applied mount exactly once and declare explicit operations and path scopes",
                        grant.mount_id
                    ),
                });
            }
            if !grant
                .operations
                .is_subset(&applied_mount.expect("checked").capabilities)
            {
                return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                    message: format!(
                        "mount grant `{}` exceeds applied mount capabilities",
                        grant.mount_id
                    ),
                });
            }
            for scope in &grant.path_scopes {
                match scope {
                    AppliedVfsPathScope::All => {}
                    AppliedVfsPathScope::Exact(path) | AppliedVfsPathScope::Prefix(path)
                        if path.is_empty() || path.starts_with('/') || path.contains('\\') =>
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
                    message: "Task grants must declare unique Product-owned scopes and explicit operations"
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunAppliedResourceSurfaceSnapshot {
    pub snapshot_revision: u64,
    pub surface: AgentRunAppliedResourceSurface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareAgentRunAppliedResourceSurface {
    pub expected_current_snapshot_revision: Option<u64>,
    pub next: AgentRunAppliedResourceSurfaceSnapshot,
}

impl PrepareAgentRunAppliedResourceSurface {
    pub fn validate(&self) -> Result<(), AgentRunAppliedResourceSurfaceWriteError> {
        self.next
            .surface
            .validate_for(&self.next.surface.target)
            .map_err(|error| AgentRunAppliedResourceSurfaceWriteError::CorruptEvidence {
                message: error.to_string(),
            })?;
        let expected_next_revision = match self.expected_current_snapshot_revision {
            Some(revision) => revision.checked_add(1).ok_or_else(|| {
                AgentRunAppliedResourceSurfaceWriteError::Conflict {
                    message: "resource surface snapshot revision exhausted u64".to_string(),
                }
            })?,
            None => 1,
        };
        if self.next.snapshot_revision != expected_next_revision {
            return Err(AgentRunAppliedResourceSurfaceWriteError::Stale {
                expected_revision: expected_next_revision,
                actual_revision: self.next.snapshot_revision,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunAppliedResourceSurfaceCommitOutcome {
    Committed,
    AlreadyCurrent,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunAppliedResourceSurfaceWriteError {
    #[error("resource surface snapshot is missing")]
    Missing,
    #[error(
        "resource surface snapshot is stale: expected revision {expected_revision}, actual revision {actual_revision}"
    )]
    Stale {
        expected_revision: u64,
        actual_revision: u64,
    },
    #[error("resource surface snapshot conflicts with current evidence: {message}")]
    Conflict { message: String },
    #[error("resource surface evidence is corrupt: {message}")]
    CorruptEvidence { message: String },
    #[error("resource surface repository failed: {message}")]
    Repository { message: String },
}

#[async_trait]
pub trait AgentRunAppliedResourceSurfaceRepository: Send + Sync {
    async fn load_current(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunAppliedResourceSurfaceSnapshot>, AgentRunAppliedResourceSurfaceWriteError>;

    async fn commit(
        &self,
        prepared: PrepareAgentRunAppliedResourceSurface,
    ) -> Result<AgentRunAppliedResourceSurfaceCommitOutcome, AgentRunAppliedResourceSurfaceWriteError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunAppliedResourceSurfaceMaterializeRequest {
    pub target: AgentRunTarget,
    pub expected_current_snapshot_revision: Option<u64>,
    pub product_binding_digest: String,
}

#[async_trait]
pub trait AgentRunAppliedResourceSurfaceCompilerPort: Send + Sync {
    async fn compile_applied_resource_surface(
        &self,
        request: &AgentRunAppliedResourceSurfaceMaterializeRequest,
    ) -> Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceWriteError>;
}

pub struct AgentRunAppliedResourceSurfaceMaterializer {
    compiler: Arc<dyn AgentRunAppliedResourceSurfaceCompilerPort>,
    repository: Arc<dyn AgentRunAppliedResourceSurfaceRepository>,
}

impl AgentRunAppliedResourceSurfaceMaterializer {
    pub fn new(
        compiler: Arc<dyn AgentRunAppliedResourceSurfaceCompilerPort>,
        repository: Arc<dyn AgentRunAppliedResourceSurfaceRepository>,
    ) -> Self {
        Self {
            compiler,
            repository,
        }
    }

    /// Product provision/materialize phase entrypoint. This must complete before Runtime activation.
    pub async fn materialize(
        &self,
        request: AgentRunAppliedResourceSurfaceMaterializeRequest,
    ) -> Result<AgentRunAppliedResourceSurfaceCommitOutcome, AgentRunAppliedResourceSurfaceWriteError>
    {
        if request.product_binding_digest.is_empty() {
            return Err(AgentRunAppliedResourceSurfaceWriteError::CorruptEvidence {
                message: "product binding digest must be non-empty".to_string(),
            });
        }
        let surface = self
            .compiler
            .compile_applied_resource_surface(&request)
            .await?;
        if surface.target != request.target
            || surface.product_binding_digest != request.product_binding_digest
        {
            return Err(AgentRunAppliedResourceSurfaceWriteError::Conflict {
                message: "compiler output does not match materialize target/binding".to_string(),
            });
        }
        let next_revision = match request.expected_current_snapshot_revision {
            Some(revision) => revision.checked_add(1).ok_or_else(|| {
                AgentRunAppliedResourceSurfaceWriteError::Conflict {
                    message: "resource surface snapshot revision exhausted u64".to_string(),
                }
            })?,
            None => 1,
        };
        let prepared = PrepareAgentRunAppliedResourceSurface {
            expected_current_snapshot_revision: request.expected_current_snapshot_revision,
            next: AgentRunAppliedResourceSurfaceSnapshot {
                snapshot_revision: next_revision,
                surface,
            },
        };
        prepared.validate()?;
        self.repository.commit(prepared).await
    }
}

#[derive(Debug, Default)]
pub struct InMemoryAgentRunAppliedResourceSurfaceRepository {
    current: RwLock<BTreeMap<AgentRunTarget, AgentRunAppliedResourceSurfaceSnapshot>>,
}

#[async_trait]
impl AgentRunAppliedResourceSurfaceRepository
    for InMemoryAgentRunAppliedResourceSurfaceRepository
{
    async fn load_current(
        &self,
        target: &AgentRunTarget,
    ) -> Result<Option<AgentRunAppliedResourceSurfaceSnapshot>, AgentRunAppliedResourceSurfaceWriteError>
    {
        Ok(self.current.read().await.get(target).cloned())
    }

    async fn commit(
        &self,
        prepared: PrepareAgentRunAppliedResourceSurface,
    ) -> Result<AgentRunAppliedResourceSurfaceCommitOutcome, AgentRunAppliedResourceSurfaceWriteError>
    {
        prepared.validate()?;
        let target = prepared.next.surface.target.clone();
        let mut current = self.current.write().await;
        match current.get(&target) {
            Some(existing) if existing == &prepared.next => {
                Ok(AgentRunAppliedResourceSurfaceCommitOutcome::AlreadyCurrent)
            }
            Some(existing)
                if existing.snapshot_revision == prepared.next.snapshot_revision
                    || prepared.expected_current_snapshot_revision
                        != Some(existing.snapshot_revision) =>
            {
                Err(AgentRunAppliedResourceSurfaceWriteError::Conflict {
                    message: format!(
                        "CAS expected {:?}, current {}, next {}",
                        prepared.expected_current_snapshot_revision,
                        existing.snapshot_revision,
                        prepared.next.snapshot_revision
                    ),
                })
            }
            None if prepared.expected_current_snapshot_revision.is_some() => {
                Err(AgentRunAppliedResourceSurfaceWriteError::Missing)
            }
            None | Some(_) => {
                current.insert(target, prepared.next);
                Ok(AgentRunAppliedResourceSurfaceCommitOutcome::Committed)
            }
        }
    }
}

#[async_trait]
impl AgentRunAppliedResourceSurfaceQueryPort
    for InMemoryAgentRunAppliedResourceSurfaceRepository
{
    async fn applied_resource_surface(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError> {
        let snapshot = self
            .current
            .read()
            .await
            .get(target)
            .cloned()
            .ok_or(AgentRunAppliedResourceSurfaceQueryError::TargetNotBound)?;
        snapshot.surface.validate_for(target)?;
        Ok(snapshot.surface)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRunAppliedResourceSurfaceQueryError {
    #[error("AgentRun target is not bound")]
    TargetNotBound,
    #[error("AgentRun resource surface has not been applied")]
    SurfaceNotApplied,
    #[error("resource surface target does not match the requested AgentRun target")]
    TargetMismatch,
    #[error(
        "resource surface projection is stale: expected revision {expected_revision}, actual revision {actual_revision}"
    )]
    ProjectionStale {
        expected_revision: u64,
        actual_revision: u64,
    },
    #[error("resource surface evidence is corrupt: {message}")]
    CorruptEvidence { message: String },
    #[error("resource surface repository failed: {message}")]
    Repository { message: String },
}

#[async_trait]
pub trait AgentRunAppliedResourceSurfaceQueryPort: Send + Sync {
    async fn applied_resource_surface(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError>;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn target() -> AgentRunTarget {
        AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        }
    }

    fn surface(target: AgentRunTarget) -> AgentRunAppliedResourceSurface {
        let project_id = Uuid::new_v4();
        AgentRunAppliedResourceSurface {
            target,
            project_id,
            workspace_id: Some(Uuid::new_v4()),
            vfs_mounts: vec![AppliedVfsMount {
                    mount_id: "workspace".to_string(),
                    provider: "workspace".to_string(),
                    backend_id: "backend".to_string(),
                    root_ref: "workspace-root".to_string(),
                    capabilities: BTreeSet::from([
                        AppliedVfsOperation::Read,
                        AppliedVfsOperation::List,
                        AppliedVfsOperation::Write,
                    ]),
                    default_write: false,
                    display_name: "Workspace".to_string(),
                }],
            default_mount_id: Some("workspace".to_string()),
            vfs_grants: vec![AppliedVfsGrant {
                mount_id: "workspace".to_string(),
                operations: BTreeSet::from([
                    AppliedVfsOperation::Read,
                    AppliedVfsOperation::List,
                ]),
                path_scopes: vec![AppliedVfsPathScope::Prefix("src".to_string())],
            }],
            agent_surface_revision: 7,
            agent_surface_digest: "sha256:agent-surface".to_string(),
            vfs_digest: "sha256:vfs".to_string(),
            task_grants: vec![AppliedTaskGrant {
                scope: AppliedTaskScope::Project {
                    project_id,
                },
                operations: BTreeSet::from([AppliedTaskOperation::Read]),
            }],
            task_surface_revision: 3,
            task_surface_digest: "sha256:task-surface".to_string(),
            task_provenance: AgentRunAppliedResourceSurfaceProvenance {
                source_kind: "project_task_surface".to_string(),
                source_id: "task-surface-1".to_string(),
                source_revision: 2,
                projection_revision: 3,
                captured_at_ms: 9,
            },
            product_binding_digest: "sha256:binding".to_string(),
            provenance: AgentRunAppliedResourceSurfaceProvenance {
                source_kind: "lifecycle_agent_surface".to_string(),
                source_id: "surface-1".to_string(),
                source_revision: 5,
                projection_revision: 7,
                captured_at_ms: 10,
            },
        }
    }

    #[test]
    fn validates_explicit_mount_grants_without_inferred_write_or_exec() {
        let target = target();
        let surface = surface(target.clone());

        surface.validate_for(&target).expect("valid surface");
        assert_eq!(
            surface.vfs_grants[0].operations,
            BTreeSet::from([AppliedVfsOperation::Read, AppliedVfsOperation::List])
        );
    }

    #[test]
    fn rejects_target_mismatch() {
        let surface = surface(target());

        assert_eq!(
            surface.validate_for(&target()),
            Err(AgentRunAppliedResourceSurfaceQueryError::TargetMismatch)
        );
    }

    #[test]
    fn rejects_missing_or_implicit_mount_grants() {
        let target = target();
        let mut surface = surface(target.clone());
        surface.vfs_grants[0].operations.clear();

        assert!(matches!(
            surface.validate_for(&target),
            Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence { .. })
        ));
    }

    #[test]
    fn rejects_non_canonical_path_scopes() {
        let target = target();
        let mut surface = surface(target.clone());
        surface.vfs_grants[0].path_scopes =
            vec![AppliedVfsPathScope::Prefix("/absolute".to_string())];

        assert!(matches!(
            surface.validate_for(&target),
            Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence { .. })
        ));
    }

    #[test]
    fn absent_task_grant_is_an_explicit_deny() {
        let target = target();
        let mut surface = surface(target.clone());
        surface.task_grants.clear();
        let scope = AppliedTaskScope::Project {
            project_id: surface.project_id,
        };

        surface.validate_for(&target).expect("valid deny surface");
        assert!(!surface.grants_task_operation(&scope, AppliedTaskOperation::Read));
        assert!(!surface.grants_task_operation(&scope, AppliedTaskOperation::Write));
    }

    #[test]
    fn task_read_grant_does_not_imply_write() {
        let target = target();
        let surface = surface(target.clone());
        let scope = AppliedTaskScope::Project {
            project_id: surface.project_id,
        };

        surface.validate_for(&target).expect("valid surface");
        assert!(surface.grants_task_operation(&scope, AppliedTaskOperation::Read));
        assert!(!surface.grants_task_operation(&scope, AppliedTaskOperation::Write));
    }

    #[test]
    fn rejects_task_grant_for_another_project() {
        let target = target();
        let mut surface = surface(target.clone());
        surface.task_grants[0].scope = AppliedTaskScope::Project {
            project_id: Uuid::new_v4(),
        };

        assert!(matches!(
            surface.validate_for(&target),
            Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence { .. })
        ));
    }

    fn prepared(
        surface: AgentRunAppliedResourceSurface,
        expected: Option<u64>,
        revision: u64,
    ) -> PrepareAgentRunAppliedResourceSurface {
        PrepareAgentRunAppliedResourceSurface {
            expected_current_snapshot_revision: expected,
            next: AgentRunAppliedResourceSurfaceSnapshot {
                snapshot_revision: revision,
                surface,
            },
        }
    }

    #[tokio::test]
    async fn prepare_without_commit_leaves_no_authoritative_snapshot() {
        let repository = InMemoryAgentRunAppliedResourceSurfaceRepository::default();
        let target = target();
        let prepared = prepared(surface(target.clone()), None, 1);
        prepared.validate().expect("prepared");

        assert_eq!(
            repository.load_current(&target).await.expect("load"),
            None
        );
    }

    #[tokio::test]
    async fn exact_commit_replay_is_idempotent() {
        let repository = InMemoryAgentRunAppliedResourceSurfaceRepository::default();
        let target = target();
        let prepared = prepared(surface(target), None, 1);

        assert_eq!(
            repository.commit(prepared.clone()).await.expect("commit"),
            AgentRunAppliedResourceSurfaceCommitOutcome::Committed
        );
        assert_eq!(
            repository.commit(prepared).await.expect("replay"),
            AgentRunAppliedResourceSurfaceCommitOutcome::AlreadyCurrent
        );
    }

    #[tokio::test]
    async fn same_revision_with_different_digest_conflicts() {
        let repository = InMemoryAgentRunAppliedResourceSurfaceRepository::default();
        let target = target();
        let first = prepared(surface(target.clone()), None, 1);
        repository.commit(first).await.expect("commit");
        let mut changed = surface(target);
        changed.agent_surface_digest = "sha256:different".to_string();

        assert!(matches!(
            repository.commit(prepared(changed, None, 1)).await,
            Err(AgentRunAppliedResourceSurfaceWriteError::Conflict { .. })
        ));
    }

    #[tokio::test]
    async fn same_declared_digests_with_different_payload_conflicts() {
        let repository = InMemoryAgentRunAppliedResourceSurfaceRepository::default();
        let target = target();
        repository
            .commit(prepared(surface(target.clone()), None, 1))
            .await
            .expect("commit");
        let mut changed = surface(target);
        changed.provenance.captured_at_ms += 1;

        assert!(matches!(
            repository.commit(prepared(changed, None, 1)).await,
            Err(AgentRunAppliedResourceSurfaceWriteError::Conflict { .. })
        ));
    }

    #[tokio::test]
    async fn same_revision_and_declared_digests_with_stale_task_grants_conflicts() {
        let repository = InMemoryAgentRunAppliedResourceSurfaceRepository::default();
        let target = target();
        repository
            .commit(prepared(surface(target.clone()), None, 1))
            .await
            .expect("commit");
        let mut changed = surface(target);
        changed.task_grants[0]
            .operations
            .insert(AppliedTaskOperation::Write);

        assert!(matches!(
            repository.commit(prepared(changed, None, 1)).await,
            Err(AgentRunAppliedResourceSurfaceWriteError::Conflict { .. })
        ));
    }

    #[test]
    fn rejects_snapshot_revision_overflow() {
        let prepared = prepared(surface(target()), Some(u64::MAX), u64::MAX);

        assert!(matches!(
            prepared.validate(),
            Err(AgentRunAppliedResourceSurfaceWriteError::Conflict { .. })
        ));
    }

    #[tokio::test]
    async fn binding_or_surface_change_requires_next_snapshot_revision() {
        let repository = InMemoryAgentRunAppliedResourceSurfaceRepository::default();
        let target = target();
        repository
            .commit(prepared(surface(target.clone()), None, 1))
            .await
            .expect("commit");
        let mut changed = surface(target.clone());
        changed.product_binding_digest = "sha256:binding-2".to_string();
        changed.agent_surface_revision = 8;
        changed.agent_surface_digest = "sha256:surface-2".to_string();

        assert_eq!(
            repository
                .commit(prepared(changed, Some(1), 2))
                .await
                .expect("next revision"),
            AgentRunAppliedResourceSurfaceCommitOutcome::Committed
        );
        assert_eq!(
            repository
                .load_current(&target)
                .await
                .expect("load")
                .expect("current")
                .snapshot_revision,
            2
        );
    }
}
