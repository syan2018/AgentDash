use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentdash_domain::backend::{
    BackendWorkspaceInventory, BackendWorkspaceInventorySource, BackendWorkspaceInventoryStatus,
    ProjectBackendAccess,
};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::workspace::{
    Workspace, WorkspaceBinding, WorkspaceBindingStatus, WorkspaceStatus, identity_payload_matches,
};

use super::detection::WorkspaceDetectionResult;
use crate::repository_set::RepositorySet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInventoryCandidate {
    pub backend_id: String,
    pub root_ref: String,
    pub identity_kind: agentdash_domain::workspace::WorkspaceIdentityKind,
    pub identity_payload: serde_json::Value,
    pub detected_facts: serde_json::Value,
    pub status: BackendWorkspaceInventoryStatus,
    pub matched_workspace_ids: Vec<Uuid>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceBindingSyncResult {
    pub updated_workspace_ids: Vec<Uuid>,
    pub created_bindings: usize,
    pub updated_bindings: usize,
    pub candidates: Vec<WorkspaceInventoryCandidate>,
    pub conflicts: Vec<WorkspaceInventoryCandidate>,
}

#[derive(Debug, Clone)]
pub(in crate::workspace) struct WorkspaceDirectoryFact {
    pub binding: WorkspaceBinding,
    pub inventory: BackendWorkspaceInventory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::workspace) enum WorkspaceDirectoryFactApplyResult {
    Created,
    Updated,
}

pub(in crate::workspace) fn workspace_inventory_from_detection(
    backend_id: String,
    root_ref: String,
    detected: &WorkspaceDetectionResult,
    source: BackendWorkspaceInventorySource,
    last_error: Option<String>,
) -> BackendWorkspaceInventory {
    let mut item = BackendWorkspaceInventory::available(
        backend_id,
        root_ref,
        detected.identity_kind.clone(),
        detected.identity_payload.clone(),
        detected.binding.detected_facts.clone(),
        source,
    );
    item.status = BackendWorkspaceInventoryStatus::Available;
    item.last_error = last_error;
    item
}

pub(in crate::workspace) fn workspace_directory_fact_from_detection(
    seed_binding: &WorkspaceBinding,
    detected: &WorkspaceDetectionResult,
    source: BackendWorkspaceInventorySource,
) -> WorkspaceDirectoryFact {
    let binding = WorkspaceBinding {
        id: seed_binding.id,
        workspace_id: seed_binding.workspace_id,
        backend_id: detected.binding.backend_id.clone(),
        root_ref: detected.binding.root_ref.clone(),
        status: detected.binding.status.clone(),
        detected_facts: detected.binding.detected_facts.clone(),
        last_verified_at: detected.binding.last_verified_at,
        priority: seed_binding.priority,
        created_at: seed_binding.created_at,
        updated_at: seed_binding.updated_at,
    };
    let inventory = workspace_inventory_from_detection(
        detected.binding.backend_id.clone(),
        detected.binding.root_ref.clone(),
        detected,
        source,
        None,
    );
    WorkspaceDirectoryFact { binding, inventory }
}

pub(in crate::workspace) fn directory_fact_matches_identity(
    identity_kind: agentdash_domain::workspace::WorkspaceIdentityKind,
    identity_payload: &serde_json::Value,
    fact: &WorkspaceDirectoryFact,
) -> bool {
    identity_kind == fact.inventory.identity_kind
        && identity_payload_matches(
            identity_kind,
            identity_payload,
            &fact.inventory.identity_payload,
            Some(&fact.inventory.detected_facts),
        )
}

pub(in crate::workspace) fn apply_workspace_directory_fact(
    workspace: &mut Workspace,
    fact: WorkspaceDirectoryFact,
    priority: i32,
) -> WorkspaceDirectoryFactApplyResult {
    let now = Utc::now();
    if let Some(existing) = workspace.bindings.iter_mut().find(|binding| {
        binding.backend_id == fact.binding.backend_id && binding.root_ref == fact.binding.root_ref
    }) {
        existing.status = fact.binding.status;
        existing.detected_facts = fact.binding.detected_facts;
        existing.last_verified_at = Some(fact.inventory.last_seen_at);
        existing.priority = priority;
        existing.updated_at = now;
        workspace.status = derive_workspace_status_from_bindings(&workspace.bindings);
        workspace.refresh_default_binding();
        return WorkspaceDirectoryFactApplyResult::Updated;
    }

    let mut binding = fact.binding;
    binding.workspace_id = workspace.id;
    binding.priority = priority;
    binding.last_verified_at = Some(fact.inventory.last_seen_at);
    binding.updated_at = now;
    workspace.bindings.push(binding);
    workspace.status = derive_workspace_status_from_bindings(&workspace.bindings);
    workspace.refresh_default_binding();
    WorkspaceDirectoryFactApplyResult::Created
}

pub(in crate::workspace) fn derive_workspace_status_from_bindings(
    bindings: &[WorkspaceBinding],
) -> WorkspaceStatus {
    if bindings
        .iter()
        .any(|binding| matches!(binding.status, WorkspaceBindingStatus::Ready))
    {
        WorkspaceStatus::Ready
    } else if bindings
        .iter()
        .any(|binding| matches!(binding.status, WorkspaceBindingStatus::Error))
    {
        WorkspaceStatus::Error
    } else {
        WorkspaceStatus::Pending
    }
}

pub async fn list_project_workspace_candidates(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<Vec<WorkspaceInventoryCandidate>, DomainError> {
    let accesses = repos
        .project_backend_access_repo
        .list_active_by_project(project_id)
        .await?;
    let inventories = list_access_inventory(repos, &accesses).await?;
    let workspaces = repos.workspace_repo.list_by_project(project_id).await?;
    Ok(build_candidates(&workspaces, inventories).0)
}

pub async fn sync_project_backend_workspace_bindings(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<WorkspaceBindingSyncResult, DomainError> {
    let accesses = repos
        .project_backend_access_repo
        .list_active_by_project(project_id)
        .await?;
    let inventories = list_access_inventory(repos, &accesses).await?;
    let mut workspaces = repos.workspace_repo.list_by_project(project_id).await?;
    let (candidates, conflicts) = build_candidates(&workspaces, inventories.clone());

    let priority_by_backend = accesses
        .iter()
        .map(|access| (access.backend_id.clone(), access.priority))
        .collect::<std::collections::HashMap<_, _>>();
    let mut updated_workspace_ids = Vec::new();
    let mut created_bindings = 0;
    let mut updated_bindings = 0;

    for inventory in inventories {
        if inventory.status != BackendWorkspaceInventoryStatus::Available {
            continue;
        }
        let matching_indexes = workspaces
            .iter()
            .enumerate()
            .filter(|(_, workspace)| workspace_matches_inventory(workspace, &inventory))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matching_indexes.len() != 1 {
            continue;
        }

        let workspace = &mut workspaces[matching_indexes[0]];
        let priority = priority_by_backend
            .get(&inventory.backend_id)
            .copied()
            .unwrap_or_default();
        let mut binding = WorkspaceBinding::new(
            workspace.id,
            inventory.backend_id.clone(),
            inventory.root_ref.clone(),
            inventory.detected_facts.clone(),
        );
        binding.status = WorkspaceBindingStatus::Ready;
        let fact = WorkspaceDirectoryFact { binding, inventory };
        match apply_workspace_directory_fact(workspace, fact, priority) {
            WorkspaceDirectoryFactApplyResult::Created => created_bindings += 1,
            WorkspaceDirectoryFactApplyResult::Updated => updated_bindings += 1,
        }
        repos.workspace_repo.update(workspace).await?;
        updated_workspace_ids.push(workspace.id);
    }

    updated_workspace_ids.sort_unstable();
    updated_workspace_ids.dedup();

    Ok(WorkspaceBindingSyncResult {
        updated_workspace_ids,
        created_bindings,
        updated_bindings,
        candidates,
        conflicts,
    })
}

async fn list_access_inventory(
    repos: &RepositorySet,
    accesses: &[ProjectBackendAccess],
) -> Result<Vec<BackendWorkspaceInventory>, DomainError> {
    let backend_ids = accesses
        .iter()
        .map(|access| access.backend_id.clone())
        .collect::<Vec<_>>();
    repos
        .backend_workspace_inventory_repo
        .list_by_backends(&backend_ids)
        .await
}

fn build_candidates(
    workspaces: &[Workspace],
    inventories: Vec<BackendWorkspaceInventory>,
) -> (
    Vec<WorkspaceInventoryCandidate>,
    Vec<WorkspaceInventoryCandidate>,
) {
    let mut candidates = Vec::new();
    let mut conflicts = Vec::new();
    for inventory in inventories {
        let matched_workspace_ids = workspaces
            .iter()
            .filter(|workspace| workspace_matches_inventory(workspace, &inventory))
            .map(|workspace| workspace.id)
            .collect::<Vec<_>>();
        if matched_workspace_ids.len() == 1 {
            continue;
        }
        let reason = if matched_workspace_ids.is_empty() {
            "未匹配现有 Workspace，需要用户确认后创建".to_string()
        } else {
            "匹配到多个 Workspace，需要人工消歧".to_string()
        };
        let item = WorkspaceInventoryCandidate {
            backend_id: inventory.backend_id,
            root_ref: inventory.root_ref,
            identity_kind: inventory.identity_kind,
            identity_payload: inventory.identity_payload,
            detected_facts: inventory.detected_facts,
            status: inventory.status,
            matched_workspace_ids,
            reason,
        };
        if item.matched_workspace_ids.is_empty() {
            candidates.push(item);
        } else {
            conflicts.push(item);
        }
    }
    (candidates, conflicts)
}

fn workspace_matches_inventory(
    workspace: &Workspace,
    inventory: &BackendWorkspaceInventory,
) -> bool {
    workspace.identity_kind == inventory.identity_kind
        && identity_payload_matches(
            workspace.identity_kind.clone(),
            &workspace.identity_payload,
            &inventory.identity_payload,
            Some(&inventory.detected_facts),
        )
}
