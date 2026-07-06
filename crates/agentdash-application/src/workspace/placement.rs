use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

use agentdash_domain::backend::{
    BackendRepository, BackendType, BackendWorkspaceInventory, BackendWorkspaceInventoryRepository,
    BackendWorkspaceInventorySource, ProjectBackendAccess, ProjectBackendAccessRepository,
};
use agentdash_domain::common::MountCapability;
use agentdash_domain::workspace::{
    P4WorkspaceIdentityContract, P4WorkspaceMatchMode, Workspace, WorkspaceBinding,
    WorkspaceIdentityKind, WorkspaceResolutionPolicy, identity_payload_matches,
    normalize_identity_payload,
};

use super::WorkspaceDetectionResult;
use super::backend_sync::{
    WorkspaceDirectoryFact, WorkspaceDirectoryFactApplyResult, apply_workspace_directory_fact,
    derive_workspace_status_from_bindings, directory_fact_matches_identity,
    workspace_directory_fact_from_detection, workspace_inventory_from_detection,
};
use crate::ApplicationError;
use crate::repository_set::RepositorySet;

#[derive(Clone)]
pub struct WorkspacePlacementService {
    repos: WorkspacePlacementRepositories,
    runtime: Arc<dyn WorkspacePlacementRuntime>,
}

#[derive(Clone)]
struct WorkspacePlacementRepositories {
    workspace_repo: Arc<dyn agentdash_domain::workspace::WorkspaceRepository>,
    backend_repo: Arc<dyn BackendRepository>,
    project_backend_access_repo: Arc<dyn ProjectBackendAccessRepository>,
    backend_workspace_inventory_repo: Arc<dyn BackendWorkspaceInventoryRepository>,
}

impl From<RepositorySet> for WorkspacePlacementRepositories {
    fn from(repos: RepositorySet) -> Self {
        Self {
            workspace_repo: repos.workspace_repo.clone(),
            backend_repo: repos.backend_repo.clone(),
            project_backend_access_repo: repos.project_backend_access_repo,
            backend_workspace_inventory_repo: repos.backend_workspace_inventory_repo.clone(),
        }
    }
}

impl WorkspacePlacementService {
    pub fn new(repos: RepositorySet, runtime: Arc<dyn WorkspacePlacementRuntime>) -> Self {
        Self {
            repos: repos.into(),
            runtime,
        }
    }

    #[cfg(test)]
    fn from_repositories(
        workspace_repo: Arc<dyn agentdash_domain::workspace::WorkspaceRepository>,
        backend_repo: Arc<dyn BackendRepository>,
        project_backend_access_repo: Arc<dyn ProjectBackendAccessRepository>,
        backend_workspace_inventory_repo: Arc<dyn BackendWorkspaceInventoryRepository>,
        runtime: Arc<dyn WorkspacePlacementRuntime>,
    ) -> Self {
        Self {
            repos: WorkspacePlacementRepositories {
                workspace_repo,
                backend_repo,
                project_backend_access_repo,
                backend_workspace_inventory_repo,
            },
            runtime,
        }
    }

    pub async fn register_backend_inventory(
        &self,
        input: RegisterBackendInventoryInput,
    ) -> Result<BackendWorkspaceInventory, ApplicationError> {
        let root_ref = normalize_required("root_ref", &input.root_ref)?;
        let access = self
            .repos
            .project_backend_access_repo
            .get_by_id(input.access_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound("ProjectBackendAccess 不存在".into()))?;
        if access.project_id != input.project_id {
            return Err(ApplicationError::NotFound(
                "ProjectBackendAccess 不存在".into(),
            ));
        }
        if !access.is_active() {
            return Err(ApplicationError::Conflict(
                "ProjectBackendAccess 当前未启用".into(),
            ));
        }

        let detected = self
            .runtime
            .detect_workspace(WorkspacePlacementDetectInput {
                project_id: input.project_id,
                workspace_id: None,
                user_id: input.user_id,
                backend_id: access.backend_id.clone(),
                root_ref: root_ref.clone(),
            })
            .await?;
        let item = workspace_inventory_from_detection(
            access.backend_id,
            root_ref,
            &detected,
            BackendWorkspaceInventorySource::ManualRegister,
            None,
        );
        self.repos
            .backend_workspace_inventory_repo
            .upsert(&item)
            .await?;
        Ok(item)
    }

    pub async fn create_workspace(
        &self,
        input: CreateWorkspacePlacementInput,
    ) -> Result<Workspace, ApplicationError> {
        let shape = self
            .derive_workspace_shape(DeriveWorkspaceShapeInput {
                project_id: input.project_id,
                user_id: input.user_id,
                identity_kind: input.identity_kind,
                identity_payload: input.identity_payload,
                bindings: input.bindings,
            })
            .await?;

        let mut workspace = Workspace::new(
            input.project_id,
            input.name,
            shape.identity_kind,
            shape.identity_payload,
            input.resolution_policy,
        );
        workspace.set_bindings(shape.bindings);
        workspace.default_binding_id = input.default_binding_id.or(workspace.default_binding_id);
        workspace.mount_capabilities = input.mount_capabilities;
        workspace.status = derive_workspace_status_from_bindings(&workspace.bindings);
        workspace.refresh_default_binding();

        if !shape.inventory_items.is_empty() {
            self.repos
                .backend_workspace_inventory_repo
                .upsert_many(&shape.inventory_items)
                .await?;
        }
        self.repos.workspace_repo.create(&workspace).await?;
        self.repos
            .workspace_repo
            .get_by_id(workspace.id)
            .await?
            .ok_or_else(|| ApplicationError::Internal("Workspace 创建后读取失败".into()))
    }

    pub async fn update_workspace(
        &self,
        input: UpdateWorkspacePlacementInput,
    ) -> Result<Workspace, ApplicationError> {
        let mut workspace = input.workspace;

        if let Some(name) = input.name {
            workspace.name = name;
        }
        if let Some(identity_kind) = input.identity_kind {
            workspace.identity_kind = identity_kind;
        }
        if let Some(identity_payload) = input.identity_payload {
            workspace.identity_payload = normalize_workspace_identity_payload(
                workspace.identity_kind.clone(),
                identity_payload,
            )?;
        }
        if let Some(resolution_policy) = input.resolution_policy {
            workspace.resolution_policy = resolution_policy;
        }

        let mut inventory_items = Vec::new();
        if let Some(bindings) = input.bindings {
            ensure_unique_bindings(&bindings)?;
            let (hydrated_bindings, next_inventory_items) = self
                .hydrate_workspace_bindings(
                    workspace.project_id,
                    input.user_id,
                    workspace.identity_kind.clone(),
                    &workspace.identity_payload,
                    bindings,
                )
                .await?;
            inventory_items = next_inventory_items;
            workspace.set_bindings(hydrated_bindings);
        }
        if let Some(default_binding_id) = input.default_binding_id {
            workspace.default_binding_id = Some(default_binding_id);
        }
        if let Some(mount_capabilities) = input.mount_capabilities {
            workspace.mount_capabilities = mount_capabilities;
        }
        workspace.status = derive_workspace_status_from_bindings(&workspace.bindings);
        workspace.refresh_default_binding();

        if !inventory_items.is_empty() {
            self.repos
                .backend_workspace_inventory_repo
                .upsert_many(&inventory_items)
                .await?;
        }
        self.repos.workspace_repo.update(&workspace).await?;
        self.repos
            .workspace_repo
            .get_by_id(workspace.id)
            .await?
            .ok_or_else(|| ApplicationError::Internal("Workspace 更新后读取失败".into()))
    }

    pub async fn bind_discovered(
        &self,
        input: BindDiscoveredWorkspaceBindingsInput,
    ) -> Result<BindDiscoveredWorkspaceBindingsResult, ApplicationError> {
        if input.bindings.is_empty() {
            return Err(ApplicationError::BadRequest("bindings 不能为空".into()));
        }

        let mut commands = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for binding in input.bindings {
            let backend_id = normalize_required("binding.backend_id", &binding.backend_id)?;
            let root_ref = normalize_required("binding.root_ref", &binding.root_ref)?;
            let key = (
                binding.workspace_id,
                binding_unique_key(&backend_id, &root_ref),
            );
            if seen.insert(key) {
                commands.push(BindDiscoveredWorkspaceBindingCommand {
                    workspace_id: binding.workspace_id,
                    backend_id,
                    root_ref,
                });
            }
        }

        let backend_id = commands
            .first()
            .map(|command| command.backend_id.clone())
            .ok_or_else(|| ApplicationError::BadRequest("bindings 不能为空".into()))?;
        if commands
            .iter()
            .any(|command| command.backend_id != backend_id)
        {
            return Err(ApplicationError::BadRequest(
                "bind-discovered 单次请求只能绑定同一个 backend".into(),
            ));
        }
        let access = self
            .ensure_local_project_backend_access(input.project_id, &backend_id)
            .await?;

        let mut workspaces = self
            .repos
            .workspace_repo
            .list_by_project(input.project_id)
            .await?
            .into_iter()
            .map(|workspace| (workspace.id, workspace))
            .collect::<std::collections::HashMap<_, _>>();
        let mut touched_workspace_ids = std::collections::HashSet::new();
        let mut created_bindings = 0;
        let mut updated_bindings = 0;
        let mut inventory_items = Vec::new();
        let mut warnings = Vec::new();

        for command in commands {
            let workspace = workspaces.get_mut(&command.workspace_id).ok_or_else(|| {
                ApplicationError::NotFound("Workspace 不存在或不属于当前 Project".into())
            })?;
            let detected = self
                .runtime
                .detect_workspace(WorkspacePlacementDetectInput {
                    project_id: input.project_id,
                    workspace_id: Some(workspace.id),
                    user_id: input.user_id.clone(),
                    backend_id: command.backend_id.clone(),
                    root_ref: command.root_ref.clone(),
                })
                .await?;

            warnings.extend(detected.warnings.clone());
            let seed_binding = WorkspaceBinding::new(
                workspace.id,
                command.backend_id.clone(),
                command.root_ref.clone(),
                json!({}),
            );
            let fact = workspace_directory_fact_from_detection(
                &seed_binding,
                &detected,
                BackendWorkspaceInventorySource::IdentityDiscovery,
            );
            if detected.identity_kind != workspace.identity_kind
                || !discovery_identity_payload_matches(
                    workspace.identity_kind.clone(),
                    &workspace.identity_payload,
                    &fact.inventory.identity_payload,
                    Some(&fact.inventory.detected_facts),
                )
            {
                return Err(ApplicationError::BadRequest(format!(
                    "目录 `{}` 与 Workspace `{}` 的 identity 不匹配",
                    command.root_ref, workspace.name
                )));
            }
            self.repos
                .backend_workspace_inventory_repo
                .upsert(&fact.inventory)
                .await?;

            match apply_workspace_directory_fact(workspace, fact.clone(), access.priority) {
                WorkspaceDirectoryFactApplyResult::Created => created_bindings += 1,
                WorkspaceDirectoryFactApplyResult::Updated => updated_bindings += 1,
            }
            touched_workspace_ids.insert(workspace.id);
            inventory_items.push(fact.inventory);
        }

        let mut stored_workspaces = Vec::new();
        let mut bound_workspace_ids = touched_workspace_ids.into_iter().collect::<Vec<_>>();
        bound_workspace_ids.sort_unstable();
        for workspace_id in &bound_workspace_ids {
            let workspace = workspaces
                .get(workspace_id)
                .ok_or_else(|| ApplicationError::Internal("Workspace 更新缓存缺失".into()))?;
            self.repos.workspace_repo.update(workspace).await?;
            let stored = self
                .repos
                .workspace_repo
                .get_by_id(*workspace_id)
                .await?
                .ok_or_else(|| ApplicationError::Internal("Workspace 更新后读取失败".into()))?;
            stored_workspaces.push(stored);
        }

        Ok(BindDiscoveredWorkspaceBindingsResult {
            backend_id,
            workspaces: stored_workspaces,
            bound_workspace_ids,
            created_bindings,
            updated_bindings,
            inventory_items,
            warnings,
        })
    }

    async fn derive_workspace_shape(
        &self,
        input: DeriveWorkspaceShapeInput,
    ) -> Result<DeriveWorkspaceShapeResult, ApplicationError> {
        ensure_unique_bindings(&input.bindings)?;

        if let Some(identity_kind) = input.identity_kind {
            let identity_payload = input.identity_payload.ok_or_else(|| {
                ApplicationError::BadRequest(
                    "显式提供 identity_kind 时，identity_payload 不能为空".into(),
                )
            })?;
            let normalized_payload =
                normalize_workspace_identity_payload(identity_kind.clone(), identity_payload)?;
            let (bindings, inventory_items) = self
                .hydrate_workspace_bindings(
                    input.project_id,
                    input.user_id,
                    identity_kind.clone(),
                    &normalized_payload,
                    input.bindings,
                )
                .await?;
            return Ok(DeriveWorkspaceShapeResult {
                identity_kind,
                identity_payload: normalized_payload,
                bindings,
                inventory_items,
            });
        }

        let Some(first_binding) = input.bindings.first().cloned() else {
            return Err(ApplicationError::BadRequest(
                "创建 Workspace 时，必须提供 identity 或至少一个 binding".into(),
            ));
        };

        let (first_fact, detected) = self
            .detect_workspace_binding_fact(input.project_id, input.user_id.clone(), &first_binding)
            .await?;
        let detected_identity_kind = detected.identity_kind.clone();
        let identity_payload = input
            .identity_payload
            .map(|payload| normalize_workspace_identity_payload(detected_identity_kind, payload))
            .transpose()?
            .unwrap_or(detected.identity_payload);
        if !directory_fact_matches_identity(
            detected.identity_kind.clone(),
            &identity_payload,
            &first_fact,
        ) {
            return Err(ApplicationError::BadRequest(format!(
                "目录 `{}` 与 Workspace identity 不匹配",
                first_binding.root_ref
            )));
        }

        let mut hydrated_bindings = vec![first_fact.binding];
        let mut inventory_items = vec![first_fact.inventory];
        let (remaining_bindings, remaining_inventory_items) = self
            .hydrate_workspace_bindings(
                input.project_id,
                input.user_id,
                detected.identity_kind.clone(),
                &identity_payload,
                input.bindings.into_iter().skip(1).collect(),
            )
            .await?;
        hydrated_bindings.extend(remaining_bindings);
        inventory_items.extend(remaining_inventory_items);
        ensure_unique_bindings(&hydrated_bindings)?;
        Ok(DeriveWorkspaceShapeResult {
            identity_kind: detected.identity_kind,
            identity_payload,
            bindings: hydrated_bindings,
            inventory_items,
        })
    }

    async fn hydrate_workspace_bindings(
        &self,
        project_id: Uuid,
        user_id: Option<String>,
        identity_kind: WorkspaceIdentityKind,
        identity_payload: &Value,
        bindings: Vec<WorkspaceBinding>,
    ) -> Result<(Vec<WorkspaceBinding>, Vec<BackendWorkspaceInventory>), ApplicationError> {
        let mut hydrated_bindings = Vec::with_capacity(bindings.len());
        let mut inventory_items = Vec::new();
        for binding in bindings {
            let (fact, _detected) = self
                .detect_workspace_binding_fact(project_id, user_id.clone(), &binding)
                .await?;
            if !directory_fact_matches_identity(identity_kind.clone(), identity_payload, &fact) {
                return Err(ApplicationError::BadRequest(format!(
                    "目录 `{}` 与 Workspace identity 不匹配",
                    binding.root_ref
                )));
            }
            hydrated_bindings.push(fact.binding);
            inventory_items.push(fact.inventory);
        }
        Ok((hydrated_bindings, inventory_items))
    }

    async fn detect_workspace_binding_fact(
        &self,
        project_id: Uuid,
        user_id: Option<String>,
        binding: &WorkspaceBinding,
    ) -> Result<(WorkspaceDirectoryFact, WorkspaceDetectionResult), ApplicationError> {
        self.ensure_project_backend_access(project_id, &binding.backend_id)
            .await?;
        let detected = self
            .runtime
            .detect_workspace(WorkspacePlacementDetectInput {
                project_id,
                workspace_id: (binding.workspace_id != Uuid::nil()).then_some(binding.workspace_id),
                user_id,
                backend_id: binding.backend_id.clone(),
                root_ref: binding.root_ref.clone(),
            })
            .await?;
        let fact = workspace_directory_fact_from_detection(
            binding,
            &detected,
            BackendWorkspaceInventorySource::ManualRegister,
        );
        Ok((fact, detected))
    }

    async fn ensure_project_backend_access(
        &self,
        project_id: Uuid,
        backend_id: &str,
    ) -> Result<ProjectBackendAccess, ApplicationError> {
        let accesses = self
            .repos
            .project_backend_access_repo
            .list_by_project(project_id)
            .await?;
        let access = accesses
            .into_iter()
            .find(|access| access.backend_id == backend_id)
            .ok_or_else(|| {
                ApplicationError::Forbidden(format!(
                    "Project 尚未授权访问 backend `{}`",
                    backend_id.trim()
                ))
            })?;
        if !access.is_active() {
            return Err(ApplicationError::Conflict(
                "ProjectBackendAccess 当前未启用".into(),
            ));
        }
        Ok(access)
    }

    async fn ensure_local_project_backend_access(
        &self,
        project_id: Uuid,
        backend_id: &str,
    ) -> Result<ProjectBackendAccess, ApplicationError> {
        let access = self
            .ensure_project_backend_access(project_id, backend_id)
            .await?;
        let backend = self
            .repos
            .backend_repo
            .get_backend(&access.backend_id)
            .await?;
        if backend.backend_type != BackendType::Local {
            return Err(ApplicationError::BadRequest(
                "本机 Workspace discovery 仅支持 local backend".into(),
            ));
        }
        Ok(access)
    }
}

#[async_trait]
pub trait WorkspacePlacementRuntime: Send + Sync {
    async fn detect_workspace(
        &self,
        input: WorkspacePlacementDetectInput,
    ) -> Result<WorkspaceDetectionResult, ApplicationError>;
}

#[derive(Debug, Clone)]
pub struct WorkspacePlacementDetectInput {
    pub project_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub user_id: Option<String>,
    pub backend_id: String,
    pub root_ref: String,
}

#[derive(Debug, Clone)]
pub struct RegisterBackendInventoryInput {
    pub project_id: Uuid,
    pub access_id: Uuid,
    pub user_id: Option<String>,
    pub root_ref: String,
}

#[derive(Debug, Clone)]
pub struct CreateWorkspacePlacementInput {
    pub project_id: Uuid,
    pub user_id: Option<String>,
    pub name: String,
    pub identity_kind: Option<WorkspaceIdentityKind>,
    pub identity_payload: Option<Value>,
    pub resolution_policy: WorkspaceResolutionPolicy,
    pub default_binding_id: Option<Uuid>,
    pub bindings: Vec<WorkspaceBinding>,
    pub mount_capabilities: Vec<MountCapability>,
}

#[derive(Debug, Clone)]
pub struct UpdateWorkspacePlacementInput {
    pub workspace: Workspace,
    pub user_id: Option<String>,
    pub name: Option<String>,
    pub identity_kind: Option<WorkspaceIdentityKind>,
    pub identity_payload: Option<Value>,
    pub resolution_policy: Option<WorkspaceResolutionPolicy>,
    pub default_binding_id: Option<Uuid>,
    pub bindings: Option<Vec<WorkspaceBinding>>,
    pub mount_capabilities: Option<Vec<MountCapability>>,
}

#[derive(Debug, Clone)]
pub struct BindDiscoveredWorkspaceBindingCommand {
    pub workspace_id: Uuid,
    pub backend_id: String,
    pub root_ref: String,
}

#[derive(Debug, Clone)]
pub struct BindDiscoveredWorkspaceBindingsInput {
    pub project_id: Uuid,
    pub user_id: Option<String>,
    pub bindings: Vec<BindDiscoveredWorkspaceBindingCommand>,
}

#[derive(Debug, Clone)]
pub struct BindDiscoveredWorkspaceBindingsResult {
    pub backend_id: String,
    pub workspaces: Vec<Workspace>,
    pub bound_workspace_ids: Vec<Uuid>,
    pub created_bindings: usize,
    pub updated_bindings: usize,
    pub inventory_items: Vec<BackendWorkspaceInventory>,
    pub warnings: Vec<String>,
}

struct DeriveWorkspaceShapeInput {
    project_id: Uuid,
    user_id: Option<String>,
    identity_kind: Option<WorkspaceIdentityKind>,
    identity_payload: Option<Value>,
    bindings: Vec<WorkspaceBinding>,
}

struct DeriveWorkspaceShapeResult {
    identity_kind: WorkspaceIdentityKind,
    identity_payload: Value,
    bindings: Vec<WorkspaceBinding>,
    inventory_items: Vec<BackendWorkspaceInventory>,
}

fn normalize_required(field: &str, raw: &str) -> Result<String, ApplicationError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ApplicationError::BadRequest(format!("{field} 不能为空")));
    }
    Ok(value.to_string())
}

fn ensure_unique_bindings(bindings: &[WorkspaceBinding]) -> Result<(), ApplicationError> {
    let mut seen = std::collections::HashSet::new();
    for binding in bindings {
        let key = binding_unique_key(&binding.backend_id, &binding.root_ref);
        if !seen.insert(key) {
            return Err(ApplicationError::BadRequest(
                "同一个 Workspace 中不能重复绑定相同 backend/root".into(),
            ));
        }
    }
    Ok(())
}

fn binding_unique_key(backend_id: &str, root_ref: &str) -> String {
    let root = root_ref.trim().replace('\\', "/");
    let root = root.trim_end_matches('/');
    format!("{}:{root}", backend_id.trim())
}

fn normalize_workspace_identity_payload(
    kind: WorkspaceIdentityKind,
    payload: Value,
) -> Result<Value, ApplicationError> {
    normalize_identity_payload(kind, &payload).map_err(ApplicationError::BadRequest)
}

fn discovery_identity_payload_matches(
    kind: WorkspaceIdentityKind,
    expected_payload: &Value,
    actual_payload: &Value,
    actual_binding_facts: Option<&Value>,
) -> bool {
    if identity_payload_matches(
        kind.clone(),
        expected_payload,
        actual_payload,
        actual_binding_facts,
    ) {
        return true;
    }

    if kind != WorkspaceIdentityKind::P4Workspace {
        return false;
    }
    let Some(relaxed_payload) = relaxed_p4_discovery_payload(expected_payload) else {
        return false;
    };
    identity_payload_matches(
        WorkspaceIdentityKind::P4Workspace,
        &relaxed_payload,
        actual_payload,
        actual_binding_facts,
    )
}

fn relaxed_p4_discovery_payload(expected_payload: &Value) -> Option<Value> {
    let normalized =
        normalize_identity_payload(WorkspaceIdentityKind::P4Workspace, expected_payload).ok()?;
    let mut contract = serde_json::from_value::<P4WorkspaceIdentityContract>(normalized).ok()?;
    if contract.match_mode != P4WorkspaceMatchMode::ServerStreamClient {
        return None;
    }
    contract.match_mode = P4WorkspaceMatchMode::ServerStream;
    contract.client_name = None;
    serde_json::to_value(contract).ok()
}

#[cfg(test)]
mod workspace_placement_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use agentdash_domain::backend::{
        BackendConfig, BackendRepository, BackendShareScopeKind, BackendType, BackendVisibility,
        BackendWorkspaceInventory, BackendWorkspaceInventoryRepository, ProjectBackendAccess,
        ProjectBackendAccessRepository, ProjectBackendAccessStatus, ViewConfig,
    };
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::workspace::{
        Workspace, WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind,
        WorkspaceRepository, WorkspaceResolutionPolicy,
    };

    use super::*;

    #[tokio::test]
    async fn register_backend_inventory_detects_and_upserts_manual_inventory() {
        let project_id = Uuid::new_v4();
        let access = ProjectBackendAccess::new(project_id, "backend-a".to_string(), None);
        let access_repo = Arc::new(FixtureAccessRepository::with_access(access.clone()));
        let inventory_repo = Arc::new(FixtureInventoryRepository::default());
        let runtime = Arc::new(FakePlacementRuntime::with_detection(detection(
            &access.backend_id,
            "D:/work",
        )));
        let service = service_with_repos(access_repo, inventory_repo.clone(), runtime.clone());

        let inventory = service
            .register_backend_inventory(RegisterBackendInventoryInput {
                project_id,
                access_id: access.id,
                user_id: Some("user-a".to_string()),
                root_ref: "  D:/work  ".to_string(),
            })
            .await
            .expect("manual inventory should register");

        assert_eq!(inventory.backend_id, "backend-a");
        assert_eq!(inventory.root_ref, "D:/work");
        assert_eq!(
            inventory.source,
            BackendWorkspaceInventorySource::ManualRegister
        );
        assert_eq!(inventory_repo.items.lock().await.len(), 1);
        let calls = runtime.calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].project_id, project_id);
        assert_eq!(calls[0].user_id.as_deref(), Some("user-a"));
        assert_eq!(calls[0].backend_id, "backend-a");
        assert_eq!(calls[0].root_ref, "D:/work");
    }

    #[tokio::test]
    async fn register_backend_inventory_rejects_inactive_access_without_detect() {
        let project_id = Uuid::new_v4();
        let mut access = ProjectBackendAccess::new(project_id, "backend-a".to_string(), None);
        access.status = ProjectBackendAccessStatus::Paused;
        let runtime = Arc::new(FakePlacementRuntime::with_detection(detection(
            &access.backend_id,
            "D:/work",
        )));
        let service = service_with_repos(
            Arc::new(FixtureAccessRepository::with_access(access.clone())),
            Arc::new(FixtureInventoryRepository::default()),
            runtime.clone(),
        );

        let error = service
            .register_backend_inventory(RegisterBackendInventoryInput {
                project_id,
                access_id: access.id,
                user_id: None,
                root_ref: "D:/work".to_string(),
            })
            .await
            .expect_err("inactive access should be rejected");

        assert!(matches!(error, ApplicationError::Conflict(_)));
        assert!(runtime.calls.lock().await.is_empty());
    }

    #[tokio::test]
    async fn register_backend_inventory_rejects_wrong_project_access() {
        let project_id = Uuid::new_v4();
        let access = ProjectBackendAccess::new(Uuid::new_v4(), "backend-a".to_string(), None);
        let runtime = Arc::new(FakePlacementRuntime::with_detection(detection(
            &access.backend_id,
            "D:/work",
        )));
        let service = service_with_repos(
            Arc::new(FixtureAccessRepository::with_access(access.clone())),
            Arc::new(FixtureInventoryRepository::default()),
            runtime.clone(),
        );

        let error = service
            .register_backend_inventory(RegisterBackendInventoryInput {
                project_id,
                access_id: access.id,
                user_id: None,
                root_ref: "D:/work".to_string(),
            })
            .await
            .expect_err("wrong project access should be hidden");

        assert!(matches!(error, ApplicationError::NotFound(_)));
        assert!(runtime.calls.lock().await.is_empty());
    }

    #[tokio::test]
    async fn register_backend_inventory_rejects_empty_root_without_detect() {
        let project_id = Uuid::new_v4();
        let access = ProjectBackendAccess::new(project_id, "backend-a".to_string(), None);
        let runtime = Arc::new(FakePlacementRuntime::with_detection(detection(
            &access.backend_id,
            "D:/work",
        )));
        let service = service_with_repos(
            Arc::new(FixtureAccessRepository::with_access(access.clone())),
            Arc::new(FixtureInventoryRepository::default()),
            runtime.clone(),
        );

        let error = service
            .register_backend_inventory(RegisterBackendInventoryInput {
                project_id,
                access_id: access.id,
                user_id: None,
                root_ref: "   ".to_string(),
            })
            .await
            .expect_err("empty root should be rejected");

        assert!(matches!(error, ApplicationError::BadRequest(_)));
        assert!(runtime.calls.lock().await.is_empty());
    }

    #[tokio::test]
    async fn create_workspace_infers_identity_from_binding_and_upserts_inventory() {
        let project_id = Uuid::new_v4();
        let access = ProjectBackendAccess::new(project_id, "backend-a".to_string(), None);
        let workspace_repo = Arc::new(FixtureWorkspaceRepository::default());
        let inventory_repo = Arc::new(FixtureInventoryRepository::default());
        let runtime = Arc::new(FakePlacementRuntime::with_detection(detection(
            &access.backend_id,
            "D:/work",
        )));
        let service = WorkspacePlacementService::from_repositories(
            workspace_repo.clone(),
            Arc::new(FixtureBackendRepository::with_backend(backend_config(
                "backend-a",
                BackendType::Local,
            ))),
            Arc::new(FixtureAccessRepository::with_access(access.clone())),
            inventory_repo.clone(),
            runtime.clone(),
        );

        let workspace = service
            .create_workspace(CreateWorkspacePlacementInput {
                project_id,
                user_id: Some("user-a".to_string()),
                name: "Main".to_string(),
                identity_kind: None,
                identity_payload: None,
                resolution_policy: WorkspaceResolutionPolicy::PreferOnline,
                default_binding_id: None,
                bindings: vec![WorkspaceBinding::new(
                    Uuid::nil(),
                    access.backend_id.clone(),
                    "D:/work".to_string(),
                    serde_json::json!({}),
                )],
                mount_capabilities: Workspace::default_mount_capabilities(),
            })
            .await
            .expect("workspace should be created");

        assert_eq!(workspace.identity_kind, WorkspaceIdentityKind::LocalDir);
        assert_eq!(workspace.bindings.len(), 1);
        assert_eq!(inventory_repo.items.lock().await.len(), 1);
        assert_eq!(workspace_repo.workspaces.lock().await.len(), 1);
        let calls = runtime.calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].workspace_id, None);
    }

    #[tokio::test]
    async fn update_workspace_hydrates_bindings_and_upserts_inventory() {
        let project_id = Uuid::new_v4();
        let access = ProjectBackendAccess::new(project_id, "backend-a".to_string(), None);
        let workspace = Workspace::new(
            project_id,
            "Main".to_string(),
            WorkspaceIdentityKind::LocalDir,
            serde_json::json!({ "match_mode": "path_key", "path_key": "d:/work" }),
            WorkspaceResolutionPolicy::PreferOnline,
        );
        let workspace_repo = Arc::new(FixtureWorkspaceRepository::with_workspace(
            workspace.clone(),
        ));
        let inventory_repo = Arc::new(FixtureInventoryRepository::default());
        let runtime = Arc::new(FakePlacementRuntime::with_detection(detection(
            &access.backend_id,
            "D:/work",
        )));
        let service = WorkspacePlacementService::from_repositories(
            workspace_repo.clone(),
            Arc::new(FixtureBackendRepository::with_backend(backend_config(
                "backend-a",
                BackendType::Local,
            ))),
            Arc::new(FixtureAccessRepository::with_access(access.clone())),
            inventory_repo.clone(),
            runtime.clone(),
        );

        let updated = service
            .update_workspace(UpdateWorkspacePlacementInput {
                workspace: workspace.clone(),
                user_id: Some("user-a".to_string()),
                name: None,
                identity_kind: None,
                identity_payload: None,
                resolution_policy: None,
                default_binding_id: None,
                bindings: Some(vec![WorkspaceBinding::new(
                    workspace.id,
                    access.backend_id.clone(),
                    "D:/work".to_string(),
                    serde_json::json!({}),
                )]),
                mount_capabilities: None,
            })
            .await
            .expect("workspace bindings should hydrate");

        assert_eq!(updated.bindings.len(), 1);
        assert_eq!(updated.bindings[0].status, WorkspaceBindingStatus::Ready);
        assert_eq!(inventory_repo.items.lock().await.len(), 1);
        let calls = runtime.calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].workspace_id, Some(workspace.id));
    }

    #[tokio::test]
    async fn create_workspace_rejects_identity_mismatch_without_writes() {
        let project_id = Uuid::new_v4();
        let access = ProjectBackendAccess::new(project_id, "backend-a".to_string(), None);
        let workspace_repo = Arc::new(FixtureWorkspaceRepository::default());
        let inventory_repo = Arc::new(FixtureInventoryRepository::default());
        let runtime = Arc::new(FakePlacementRuntime::with_detection(detection(
            &access.backend_id,
            "D:/work",
        )));
        let service = WorkspacePlacementService::from_repositories(
            workspace_repo.clone(),
            Arc::new(FixtureBackendRepository::with_backend(backend_config(
                "backend-a",
                BackendType::Local,
            ))),
            Arc::new(FixtureAccessRepository::with_access(access.clone())),
            inventory_repo.clone(),
            runtime.clone(),
        );

        let error = service
            .create_workspace(CreateWorkspacePlacementInput {
                project_id,
                user_id: Some("user-a".to_string()),
                name: "Main".to_string(),
                identity_kind: Some(WorkspaceIdentityKind::LocalDir),
                identity_payload: Some(
                    serde_json::json!({ "match_mode": "path_key", "path_key": "d:/other" }),
                ),
                resolution_policy: WorkspaceResolutionPolicy::PreferOnline,
                default_binding_id: None,
                bindings: vec![WorkspaceBinding::new(
                    Uuid::nil(),
                    access.backend_id.clone(),
                    "D:/work".to_string(),
                    serde_json::json!({}),
                )],
                mount_capabilities: Workspace::default_mount_capabilities(),
            })
            .await
            .expect_err("identity mismatch should be rejected");

        assert!(matches!(error, ApplicationError::BadRequest(_)));
        assert!(workspace_repo.workspaces.lock().await.is_empty());
        assert!(inventory_repo.items.lock().await.is_empty());
        assert_eq!(runtime.calls.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn bind_discovered_redetects_and_applies_identity_discovery_inventory() {
        let project_id = Uuid::new_v4();
        let mut access = ProjectBackendAccess::new(project_id, "backend-a".to_string(), None);
        access.priority = 7;
        let mut workspace = Workspace::new(
            project_id,
            "Main".to_string(),
            WorkspaceIdentityKind::LocalDir,
            serde_json::json!({ "match_mode": "path_key", "path_key": "d:/work" }),
            WorkspaceResolutionPolicy::PreferOnline,
        );
        let workspace_repo = Arc::new(FixtureWorkspaceRepository::with_workspace(
            workspace.clone(),
        ));
        let inventory_repo = Arc::new(FixtureInventoryRepository::default());
        let runtime = Arc::new(FakePlacementRuntime::with_detection(detection(
            &access.backend_id,
            "D:/work",
        )));
        let service = WorkspacePlacementService::from_repositories(
            workspace_repo.clone(),
            Arc::new(FixtureBackendRepository::with_backend(backend_config(
                "backend-a",
                BackendType::Local,
            ))),
            Arc::new(FixtureAccessRepository::with_access(access.clone())),
            inventory_repo.clone(),
            runtime.clone(),
        );

        let result = service
            .bind_discovered(BindDiscoveredWorkspaceBindingsInput {
                project_id,
                user_id: Some("user-a".to_string()),
                bindings: vec![BindDiscoveredWorkspaceBindingCommand {
                    workspace_id: workspace.id,
                    backend_id: access.backend_id.clone(),
                    root_ref: "D:/work".to_string(),
                }],
            })
            .await
            .expect("discovered binding should apply");

        assert_eq!(result.backend_id, "backend-a");
        assert_eq!(result.bound_workspace_ids, vec![workspace.id]);
        assert_eq!(result.created_bindings, 1);
        assert_eq!(result.updated_bindings, 0);
        assert_eq!(result.inventory_items.len(), 1);
        assert_eq!(
            result.inventory_items[0].source,
            BackendWorkspaceInventorySource::IdentityDiscovery
        );
        workspace = workspace_repo
            .get_by_id(workspace.id)
            .await
            .expect("repo should load")
            .expect("workspace should exist");
        assert_eq!(workspace.bindings.len(), 1);
        assert_eq!(workspace.bindings[0].priority, 7);
        assert_eq!(runtime.calls.lock().await.len(), 1);
    }

    fn service_with_repos(
        access_repo: Arc<dyn ProjectBackendAccessRepository>,
        inventory_repo: Arc<dyn BackendWorkspaceInventoryRepository>,
        runtime: Arc<dyn WorkspacePlacementRuntime>,
    ) -> WorkspacePlacementService {
        WorkspacePlacementService::from_repositories(
            Arc::new(FixtureWorkspaceRepository::default()),
            Arc::new(FixtureBackendRepository::with_backend(backend_config(
                "backend-a",
                BackendType::Local,
            ))),
            access_repo,
            inventory_repo,
            runtime,
        )
    }

    #[derive(Default)]
    struct FixtureAccessRepository {
        accesses: Mutex<HashMap<Uuid, ProjectBackendAccess>>,
    }

    impl FixtureAccessRepository {
        fn with_access(access: ProjectBackendAccess) -> Self {
            Self {
                accesses: Mutex::new(HashMap::from([(access.id, access)])),
            }
        }
    }

    #[async_trait]
    impl ProjectBackendAccessRepository for FixtureAccessRepository {
        async fn create(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            self.accesses.lock().await.insert(access.id, access.clone());
            Ok(())
        }

        async fn update(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
            self.accesses.lock().await.insert(access.id, access.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(self.accesses.lock().await.get(&id).cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .values()
                .filter(|access| access.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_active_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .list_by_project(project_id)
                .await?
                .into_iter()
                .filter(ProjectBackendAccess::is_active)
                .collect())
        }

        async fn get_active_for_project_backend(
            &self,
            project_id: Uuid,
            backend_id: &str,
        ) -> Result<Option<ProjectBackendAccess>, DomainError> {
            Ok(self
                .list_active_by_project(project_id)
                .await?
                .into_iter()
                .find(|access| access.backend_id == backend_id))
        }

        async fn list_active_by_backend(
            &self,
            backend_id: &str,
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .values()
                .filter(|access| access.backend_id == backend_id && access.is_active())
                .cloned()
                .collect())
        }

        async fn list_active_by_backends(
            &self,
            backend_ids: &[String],
        ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
            Ok(self
                .accesses
                .lock()
                .await
                .values()
                .filter(|access| backend_ids.contains(&access.backend_id) && access.is_active())
                .cloned()
                .collect())
        }

        async fn set_status(
            &self,
            id: Uuid,
            status: ProjectBackendAccessStatus,
        ) -> Result<(), DomainError> {
            if let Some(access) = self.accesses.lock().await.get_mut(&id) {
                access.status = status;
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct FixtureInventoryRepository {
        items: Mutex<Vec<BackendWorkspaceInventory>>,
    }

    #[async_trait]
    impl BackendWorkspaceInventoryRepository for FixtureInventoryRepository {
        async fn upsert(&self, item: &BackendWorkspaceInventory) -> Result<(), DomainError> {
            self.items.lock().await.push(item.clone());
            Ok(())
        }

        async fn upsert_many(
            &self,
            items: &[BackendWorkspaceInventory],
        ) -> Result<(), DomainError> {
            self.items.lock().await.extend_from_slice(items);
            Ok(())
        }

        async fn list_by_backend(
            &self,
            backend_id: &str,
        ) -> Result<Vec<BackendWorkspaceInventory>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|item| item.backend_id == backend_id)
                .cloned()
                .collect())
        }

        async fn list_by_backends(
            &self,
            backend_ids: &[String],
        ) -> Result<Vec<BackendWorkspaceInventory>, DomainError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .filter(|item| backend_ids.contains(&item.backend_id))
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct FixtureWorkspaceRepository {
        workspaces: Mutex<HashMap<Uuid, Workspace>>,
    }

    impl FixtureWorkspaceRepository {
        fn with_workspace(workspace: Workspace) -> Self {
            Self {
                workspaces: Mutex::new(HashMap::from([(workspace.id, workspace)])),
            }
        }
    }

    #[async_trait]
    impl WorkspaceRepository for FixtureWorkspaceRepository {
        async fn create(&self, workspace: &Workspace) -> Result<(), DomainError> {
            self.workspaces
                .lock()
                .await
                .insert(workspace.id, workspace.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<Workspace>, DomainError> {
            Ok(self.workspaces.lock().await.get(&id).cloned())
        }

        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Workspace>, DomainError> {
            Ok(self
                .workspaces
                .lock()
                .await
                .values()
                .filter(|workspace| workspace.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, workspace: &Workspace) -> Result<(), DomainError> {
            self.workspaces
                .lock()
                .await
                .insert(workspace.id, workspace.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.workspaces.lock().await.remove(&id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FixtureBackendRepository {
        backends: Mutex<HashMap<String, BackendConfig>>,
    }

    impl FixtureBackendRepository {
        fn with_backend(backend: BackendConfig) -> Self {
            Self {
                backends: Mutex::new(HashMap::from([(backend.id.clone(), backend)])),
            }
        }
    }

    #[async_trait]
    impl BackendRepository for FixtureBackendRepository {
        async fn add_backend(&self, config: &BackendConfig) -> Result<(), DomainError> {
            self.backends
                .lock()
                .await
                .insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn list_backends(&self) -> Result<Vec<BackendConfig>, DomainError> {
            Ok(self.backends.lock().await.values().cloned().collect())
        }

        async fn get_backend(&self, id: &str) -> Result<BackendConfig, DomainError> {
            self.backends
                .lock()
                .await
                .get(id)
                .cloned()
                .ok_or_else(|| DomainError::NotFound {
                    entity: "Backend",
                    id: id.to_string(),
                })
        }

        async fn get_backend_by_auth_token(
            &self,
            _token: &str,
        ) -> Result<BackendConfig, DomainError> {
            unreachable!("test does not load backend by auth token")
        }

        async fn ensure_local_backend(
            &self,
            _claim: &agentdash_domain::backend::LocalBackendClaim,
        ) -> Result<BackendConfig, DomainError> {
            unreachable!("test does not ensure local backend")
        }

        async fn remove_backend(&self, id: &str) -> Result<(), DomainError> {
            self.backends.lock().await.remove(id);
            Ok(())
        }

        async fn list_views(&self) -> Result<Vec<ViewConfig>, DomainError> {
            Ok(Vec::new())
        }

        async fn save_view(&self, _view: &ViewConfig) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct FakePlacementRuntime {
        result: WorkspaceDetectionResult,
        calls: Mutex<Vec<WorkspacePlacementDetectInput>>,
    }

    impl FakePlacementRuntime {
        fn with_detection(result: WorkspaceDetectionResult) -> Self {
            Self {
                result,
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl WorkspacePlacementRuntime for FakePlacementRuntime {
        async fn detect_workspace(
            &self,
            input: WorkspacePlacementDetectInput,
        ) -> Result<WorkspaceDetectionResult, ApplicationError> {
            self.calls.lock().await.push(input);
            Ok(self.result.clone())
        }
    }

    fn detection(backend_id: &str, root_ref: &str) -> WorkspaceDetectionResult {
        let mut binding = WorkspaceBinding::new(
            Uuid::nil(),
            backend_id.to_string(),
            root_ref.to_string(),
            serde_json::json!({ "kind": "test" }),
        );
        binding.status = WorkspaceBindingStatus::Ready;
        WorkspaceDetectionResult {
            identity_kind: WorkspaceIdentityKind::LocalDir,
            identity_payload: serde_json::json!({ "match_mode": "path_key", "path_key": "d:/work" }),
            binding,
            confidence: "high".to_string(),
            warnings: Vec::new(),
        }
    }

    fn backend_config(id: &str, backend_type: BackendType) -> BackendConfig {
        BackendConfig {
            id: id.to_string(),
            name: id.to_string(),
            endpoint: "http://localhost".to_string(),
            auth_token: None,
            enabled: true,
            backend_type,
            owner_user_id: Some("user-a".to_string()),
            profile_id: None,
            device_id: None,
            machine_id: None,
            machine_label: None,
            visibility: BackendVisibility::Private,
            share_scope_kind: BackendShareScopeKind::User,
            share_scope_id: Some("user-a".to_string()),
            capability_slot: "default".to_string(),
            device: serde_json::json!({}),
            last_claimed_at: None,
        }
    }
}
