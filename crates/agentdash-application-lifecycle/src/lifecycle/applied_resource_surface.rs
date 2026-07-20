use std::{collections::BTreeSet, sync::Arc};

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_application_agentrun::agent_run::{
    AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError,
    AgentRunAppliedResourceSurfaceQueryPort, AppliedVfsGrant, AppliedVfsMount, AppliedVfsOperation,
    AppliedVfsPathScope,
};
use agentdash_application_vfs::append_lifecycle_skill_asset_projection;
use agentdash_domain::{
    agent_run_target::AgentRunTarget,
    common::{Mount, MountCapability, Vfs},
};
use async_trait::async_trait;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::vfs_mount::{build_agent_run_lifecycle_mount, build_lifecycle_mount_with_node_scope};

const LIFECYCLE_MOUNT_ID: &str = "lifecycle";

/// Product-owned evidence required to expose one AgentRun's canonical history through Lifecycle
/// VFS.
///
/// The query implementation must resolve these values from the committed Product Runtime binding
/// and immutable launch frame. The Lifecycle layer never derives them from Runtime timing,
/// conversation payloads, or journal records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLifecycleMountFacts {
    pub target: AgentRunTarget,
    pub runtime_thread_id: RuntimeThreadId,
    pub launch_frame_id: Uuid,
    pub product_binding_digest: String,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
    pub node_lifecycle_key: Option<String>,
    pub writable_port_keys: Vec<String>,
    pub skill_asset_keys: Vec<String>,
}

#[async_trait]
pub trait AgentRunLifecycleMountFactsQueryPort: Send + Sync {
    async fn lifecycle_mount_facts(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunLifecycleMountFacts, AgentRunAppliedResourceSurfaceQueryError>;
}

/// Final Product surface compiler decorator.
///
/// The inner compiler owns the complete Product VFS/Task grant policy. This decorator adds the
/// single Lifecycle mount with canonical-history read grants and, for an active workflow node,
/// independently scoped node-output write grants. It then recalculates the final VFS digest and
/// is evaluated on demand by Product consumers.
pub struct AgentRunLifecycleAppliedResourceSurfaceCompiler {
    inner: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
    facts: Arc<dyn AgentRunLifecycleMountFactsQueryPort>,
}

impl AgentRunLifecycleAppliedResourceSurfaceCompiler {
    pub fn new(
        inner: Arc<dyn AgentRunAppliedResourceSurfaceQueryPort>,
        facts: Arc<dyn AgentRunLifecycleMountFactsQueryPort>,
    ) -> Self {
        Self { inner, facts }
    }
}

#[async_trait]
impl AgentRunAppliedResourceSurfaceQueryPort for AgentRunLifecycleAppliedResourceSurfaceCompiler {
    async fn applied_resource_surface(
        &self,
        target: &AgentRunTarget,
    ) -> Result<AgentRunAppliedResourceSurface, AgentRunAppliedResourceSurfaceQueryError> {
        let mut surface = self.inner.applied_resource_surface(target).await?;
        let facts = self.facts.lifecycle_mount_facts(target).await?;
        install_agent_run_lifecycle_applied_mount(&mut surface, &facts)?;
        Ok(surface)
    }
}

/// Installs the unique canonical-history mount into a fully compiled Product resource surface.
///
/// Existing Product mounts and grants remain untouched. Any prior Lifecycle mount/grant is
/// replaced so stale Runtime-thread or frame coordinates cannot survive rematerialization.
pub fn install_agent_run_lifecycle_applied_mount(
    surface: &mut AgentRunAppliedResourceSurface,
    facts: &AgentRunLifecycleMountFacts,
) -> Result<(), AgentRunAppliedResourceSurfaceQueryError> {
    if surface.target != facts.target {
        return Err(AgentRunAppliedResourceSurfaceQueryError::Conflict {
            message: "Lifecycle mount target does not match the compiled Product surface"
                .to_string(),
        });
    }
    if surface.product_binding_digest != facts.product_binding_digest
        || facts.product_binding_digest.trim().is_empty()
    {
        return Err(AgentRunAppliedResourceSurfaceQueryError::Conflict {
            message: "Lifecycle mount facts do not match the compiled Product binding".to_string(),
        });
    }

    let node_scope = match (
        facts.orchestration_id,
        facts.node_path.as_deref(),
        facts.node_lifecycle_key.as_deref(),
    ) {
        (Some(orchestration_id), Some(node_path), Some(lifecycle_key)) => {
            Some((orchestration_id, node_path, lifecycle_key))
        }
        (None, None, None) => None,
        _ => {
            return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                message:
                    "Lifecycle workflow mount facts must provide orchestration, node path and lifecycle key together"
                        .to_string(),
            });
        }
    };
    if node_scope.is_none() && !facts.writable_port_keys.is_empty() {
        return Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
            message: "Lifecycle writable ports require an active workflow node".to_string(),
        });
    }
    let mut writable_port_keys = facts.writable_port_keys.clone();
    writable_port_keys.sort();
    writable_port_keys.dedup();
    let mut mount = if let Some((orchestration_id, node_path, lifecycle_key)) = node_scope {
        let mut mount = build_lifecycle_mount_with_node_scope(
            facts.target.run_id,
            Some(facts.target.agent_id),
            orchestration_id,
            node_path,
            lifecycle_key,
            &writable_port_keys,
            facts.node_attempt,
        );
        mount.metadata["runtime_thread_id"] = serde_json::json!(facts.runtime_thread_id.as_str());
        mount.metadata["launch_frame_id"] = serde_json::json!(facts.launch_frame_id.to_string());
        mount
    } else {
        build_agent_run_lifecycle_mount(
            facts.target.run_id,
            facts.target.agent_id,
            facts.runtime_thread_id.as_str(),
            facts.launch_frame_id,
            None,
            None,
            facts.node_attempt,
        )
    };
    if !facts.skill_asset_keys.is_empty() {
        let mut vfs = Vfs {
            mounts: vec![mount],
            ..Vfs::default()
        };
        append_lifecycle_skill_asset_projection(
            &mut vfs,
            surface.project_id,
            &facts.skill_asset_keys,
        );
        mount = vfs
            .mounts
            .pop()
            .expect("Lifecycle projection always contains one mount");
    }
    let applied_mount = applied_mount(mount)?;

    surface
        .vfs_mounts
        .retain(|candidate| candidate.mount_id != LIFECYCLE_MOUNT_ID);
    surface.vfs_mounts.push(applied_mount);
    surface
        .vfs_grants
        .retain(|candidate| candidate.mount_id != LIFECYCLE_MOUNT_ID);
    surface.vfs_grants.push(AppliedVfsGrant {
        mount_id: LIFECYCLE_MOUNT_ID.to_string(),
        operations: BTreeSet::from([
            AppliedVfsOperation::Read,
            AppliedVfsOperation::List,
            AppliedVfsOperation::Search,
        ]),
        path_scopes: vec![AppliedVfsPathScope::All],
    });
    if node_scope.is_some() {
        let mut write_scopes = writable_port_keys
            .iter()
            .map(|port_key| AppliedVfsPathScope::Exact(format!("node/artifacts/{port_key}")))
            .collect::<Vec<_>>();
        write_scopes.push(AppliedVfsPathScope::Prefix("node/records".to_string()));
        surface.vfs_grants.push(AppliedVfsGrant {
            mount_id: LIFECYCLE_MOUNT_ID.to_string(),
            operations: BTreeSet::from([AppliedVfsOperation::Write]),
            path_scopes: write_scopes,
        });
    }
    surface.vfs_digest = canonical_vfs_digest(
        &surface.vfs_mounts,
        surface.default_mount_id.as_deref(),
        &surface.vfs_grants,
    )?;
    surface.validate_for(&facts.target).map_err(|error| {
        AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
            message: error.to_string(),
        }
    })
}

fn applied_mount(
    mount: Mount,
) -> Result<AppliedVfsMount, AgentRunAppliedResourceSurfaceQueryError> {
    let capabilities = mount
        .capabilities
        .into_iter()
        .map(|capability| match capability {
            MountCapability::Read => Ok(AppliedVfsOperation::Read),
            MountCapability::List => Ok(AppliedVfsOperation::List),
            MountCapability::Search => Ok(AppliedVfsOperation::Search),
            MountCapability::Write => Ok(AppliedVfsOperation::Write),
            MountCapability::Exec => Ok(AppliedVfsOperation::Exec),
            MountCapability::Watch => {
                Err(AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
                    message: "AppliedResourceSurface has no Watch authorization operation"
                        .to_string(),
                })
            }
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(AppliedVfsMount {
        mount_id: mount.id,
        provider: mount.provider,
        backend_id: mount.backend_id,
        root_ref: mount.root_ref,
        capabilities,
        default_write: mount.default_write,
        display_name: mount.display_name,
        metadata: mount.metadata,
    })
}

#[derive(Serialize)]
struct CanonicalVfsDigest<'a> {
    mounts: &'a [AppliedVfsMount],
    default_mount_id: Option<&'a str>,
    grants: &'a [AppliedVfsGrant],
}

fn canonical_vfs_digest(
    mounts: &[AppliedVfsMount],
    default_mount_id: Option<&str>,
    grants: &[AppliedVfsGrant],
) -> Result<String, AgentRunAppliedResourceSurfaceQueryError> {
    let mut mounts = mounts.to_vec();
    mounts.sort_by(|left, right| left.mount_id.cmp(&right.mount_id));
    let mut grants = grants.to_vec();
    grants.sort_by(|left, right| {
        left.mount_id
            .cmp(&right.mount_id)
            .then_with(|| left.operations.cmp(&right.operations))
            .then_with(|| left.path_scopes.cmp(&right.path_scopes))
    });
    let canonical = serde_json::to_vec(&CanonicalVfsDigest {
        mounts: &mounts,
        default_mount_id,
        grants: &grants,
    })
    .map_err(
        |error| AgentRunAppliedResourceSurfaceQueryError::CorruptEvidence {
            message: format!("canonical VFS digest serialization failed: {error}"),
        },
    )?;
    Ok(format!("sha256:{:x}", Sha256::digest(canonical)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_agentrun::agent_run::{
        AgentRunAppliedResourceSurfaceProvenance, AppliedTaskGrant, AppliedTaskOperation,
        AppliedTaskScope,
    };

    fn target() -> AgentRunTarget {
        AgentRunTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        }
    }

    fn provenance() -> AgentRunAppliedResourceSurfaceProvenance {
        AgentRunAppliedResourceSurfaceProvenance {
            source_kind: "agent_frame".to_string(),
            source_id: Uuid::new_v4().to_string(),
            source_revision: 1,
            projection_revision: 1,
            captured_at_ms: 1,
        }
    }

    fn base_surface(target: AgentRunTarget, project_id: Uuid) -> AgentRunAppliedResourceSurface {
        let main_operations =
            BTreeSet::from([AppliedVfsOperation::Read, AppliedVfsOperation::List]);
        AgentRunAppliedResourceSurface {
            target,
            project_id,
            workspace_id: None,
            vfs_mounts: vec![AppliedVfsMount {
                mount_id: "main".to_string(),
                provider: "workspace".to_string(),
                backend_id: "workspace-backend".to_string(),
                root_ref: "workspace://root".to_string(),
                capabilities: main_operations.clone(),
                default_write: false,
                display_name: "Workspace".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("main".to_string()),
            vfs_grants: vec![AppliedVfsGrant {
                mount_id: "main".to_string(),
                operations: main_operations,
                path_scopes: vec![AppliedVfsPathScope::All],
            }],
            agent_surface_revision: 3,
            agent_surface_digest: "sha256:agent-surface".to_string(),
            vfs_digest: "sha256:before-lifecycle".to_string(),
            task_grants: vec![AppliedTaskGrant {
                scope: AppliedTaskScope::Project { project_id },
                operations: BTreeSet::from([AppliedTaskOperation::Read]),
            }],
            task_surface_digest: "sha256:task".to_string(),
            product_binding_digest: "sha256:binding".to_string(),
            provenance: provenance(),
        }
    }

    fn facts(target: AgentRunTarget) -> AgentRunLifecycleMountFacts {
        AgentRunLifecycleMountFacts {
            target,
            runtime_thread_id: RuntimeThreadId::new("runtime-thread-1").expect("thread"),
            launch_frame_id: Uuid::new_v4(),
            product_binding_digest: "sha256:binding".to_string(),
            orchestration_id: Some(Uuid::new_v4()),
            node_path: Some("plan/implement".to_string()),
            node_attempt: Some(2),
            node_lifecycle_key: Some("workflow-node".to_string()),
            writable_port_keys: vec!["result".to_string(), "result".to_string()],
            skill_asset_keys: vec![
                "companion-system".to_string(),
                "companion-system".to_string(),
            ],
        }
    }

    #[test]
    fn installs_canonical_history_read_and_scoped_node_write_grants() {
        let target = target();
        let project_id = Uuid::new_v4();
        let mut surface = base_surface(target.clone(), project_id);
        let facts = facts(target.clone());

        install_agent_run_lifecycle_applied_mount(&mut surface, &facts)
            .expect("install Lifecycle mount");

        assert_eq!(surface.vfs_mounts.len(), 2);
        let mount = surface
            .vfs_mounts
            .iter()
            .find(|mount| mount.mount_id == LIFECYCLE_MOUNT_ID)
            .expect("Lifecycle mount");
        assert_eq!(mount.provider, "lifecycle_vfs");
        assert_eq!(
            mount.backend_id,
            format!(
                "lifecycle-node:{}:{}:plan%2Fimplement",
                facts.target.run_id,
                facts.orchestration_id.expect("orchestration")
            )
        );
        assert_eq!(
            mount.metadata["runtime_thread_id"],
            facts.runtime_thread_id.as_str()
        );
        assert_eq!(
            mount.metadata["launch_frame_id"],
            facts.launch_frame_id.to_string()
        );
        assert_eq!(mount.metadata["scope"], "node_runtime");
        assert_eq!(
            mount.metadata["writable_port_keys"],
            serde_json::json!(["result"])
        );
        assert_eq!(
            mount.metadata["skill_asset_keys"],
            serde_json::json!(["companion-system"])
        );
        let lifecycle_read_grant = surface
            .vfs_grants
            .iter()
            .find(|grant| {
                grant.mount_id == LIFECYCLE_MOUNT_ID
                    && grant.operations.contains(&AppliedVfsOperation::Read)
            })
            .expect("Lifecycle read grant");
        assert_eq!(
            lifecycle_read_grant.operations,
            BTreeSet::from([
                AppliedVfsOperation::Read,
                AppliedVfsOperation::List,
                AppliedVfsOperation::Search,
            ])
        );
        assert_eq!(
            lifecycle_read_grant.path_scopes,
            vec![AppliedVfsPathScope::All]
        );
        let lifecycle_write_grant = surface
            .vfs_grants
            .iter()
            .find(|grant| {
                grant.mount_id == LIFECYCLE_MOUNT_ID
                    && grant.operations.contains(&AppliedVfsOperation::Write)
            })
            .expect("Lifecycle node write grant");
        assert_eq!(
            lifecycle_write_grant.path_scopes,
            vec![
                AppliedVfsPathScope::Exact("node/artifacts/result".to_string()),
                AppliedVfsPathScope::Prefix("node/records".to_string()),
            ]
        );
        assert_eq!(
            surface
                .vfs_grants
                .iter()
                .find(|grant| grant.mount_id == "main")
                .expect("existing Product grant")
                .operations,
            BTreeSet::from([AppliedVfsOperation::Read, AppliedVfsOperation::List])
        );
        assert!(surface.vfs_digest.starts_with("sha256:"));
        assert_ne!(surface.vfs_digest, "sha256:before-lifecycle");
        surface.validate_for(&target).expect("valid final surface");
    }

    #[test]
    fn rematerialization_replaces_stale_lifecycle_coordinates_and_is_digest_stable() {
        let target = target();
        let mut surface = base_surface(target.clone(), Uuid::new_v4());
        let facts = facts(target);
        install_agent_run_lifecycle_applied_mount(&mut surface, &facts).expect("first install");
        let first_digest = surface.vfs_digest.clone();

        install_agent_run_lifecycle_applied_mount(&mut surface, &facts).expect("reinstall");

        assert_eq!(
            surface
                .vfs_mounts
                .iter()
                .filter(|mount| mount.mount_id == LIFECYCLE_MOUNT_ID)
                .count(),
            1
        );
        assert_eq!(surface.vfs_digest, first_digest);
    }

    #[test]
    fn rejects_cross_binding_or_cross_target_mount_facts() {
        let surface_target = target();
        let mut surface = base_surface(surface_target.clone(), Uuid::new_v4());
        let mut wrong_binding = facts(surface_target);
        wrong_binding.product_binding_digest = "sha256:other".to_string();
        assert!(matches!(
            install_agent_run_lifecycle_applied_mount(&mut surface, &wrong_binding),
            Err(AgentRunAppliedResourceSurfaceQueryError::Conflict { .. })
        ));

        let wrong_target = facts(target());
        assert!(matches!(
            install_agent_run_lifecycle_applied_mount(&mut surface, &wrong_target),
            Err(AgentRunAppliedResourceSurfaceQueryError::Conflict { .. })
        ));
    }
}
