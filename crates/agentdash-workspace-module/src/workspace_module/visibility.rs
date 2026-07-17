use std::sync::Arc;

use agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView;
use agentdash_application_vfs::PROVIDER_CANVAS_FS;
use agentdash_contracts::workspace_module::{WorkspaceModuleDescriptor, WorkspaceModuleKind};
use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::{Vfs, WorkspaceModuleDimension};
use uuid::Uuid;

use crate::extension_runtime::extension_runtime_projection_from_installations;
use crate::workspace_module::{
    WorkspaceModuleOperationContext, build_workspace_modules_with_operation_context,
};

#[derive(Debug, Clone, Copy)]
pub struct WorkspaceModuleVisibilityInput<'a> {
    pub base_visibility: &'a WorkspaceModuleDimension,
    pub runtime_vfs: &'a Vfs,
}

impl<'a> From<&'a AgentRunEffectiveCapabilityView> for WorkspaceModuleVisibilityInput<'a> {
    fn from(view: &'a AgentRunEffectiveCapabilityView) -> Self {
        Self {
            base_visibility: &view.capability_state.workspace_module,
            runtime_vfs: &view.vfs_surface,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceModuleVisibilityDiagnostic {
    pub code: String,
    pub message: String,
    pub module_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceModuleVisibilityProjection {
    pub modules: Vec<WorkspaceModuleDescriptor>,
    pub base_visibility: WorkspaceModuleDimension,
    pub diagnostics: Vec<WorkspaceModuleVisibilityDiagnostic>,
}

fn runtime_canvas_module_refs(vfs: &Vfs) -> Vec<String> {
    let mut refs = vfs
        .mounts
        .iter()
        .filter(|mount| mount.provider == PROVIDER_CANVAS_FS)
        .map(|mount| mount.id.trim())
        .filter(|mount_id| !mount_id.is_empty())
        .map(|mount_id| format!("canvas:{mount_id}"))
        .collect::<Vec<_>>();
    refs.sort();
    refs.dedup();
    refs
}

pub async fn resolve_workspace_module_visibility(
    installation_repo: &Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    input: WorkspaceModuleVisibilityInput<'_>,
) -> Result<WorkspaceModuleVisibilityProjection, String> {
    resolve_workspace_module_visibility_with_operation_context(
        installation_repo,
        canvas_repo,
        project_id,
        input,
        &WorkspaceModuleOperationContext::default(),
    )
    .await
}

pub async fn resolve_workspace_module_visibility_with_operation_context(
    installation_repo: &Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    input: WorkspaceModuleVisibilityInput<'_>,
    operation_context: &WorkspaceModuleOperationContext,
) -> Result<WorkspaceModuleVisibilityProjection, String> {
    let installations = installation_repo
        .list_enabled_by_project(project_id)
        .await
        .map_err(|error| error.to_string())?;
    let projection =
        extension_runtime_projection_from_installations(installations).map_err(|error| {
            format!("failed to project enabled workspace module extensions: {error}")
        })?;
    let canvases: Vec<Canvas> = canvas_repo
        .list_by_project(project_id)
        .await
        .map_err(|error| error.to_string())?;

    let modules =
        build_workspace_modules_with_operation_context(&projection, &canvases, operation_context);
    Ok(project_workspace_module_visibility(modules, input))
}

pub fn project_workspace_module_visibility(
    modules: Vec<WorkspaceModuleDescriptor>,
    input: WorkspaceModuleVisibilityInput<'_>,
) -> WorkspaceModuleVisibilityProjection {
    let base_visibility = input.base_visibility.clone();
    let runtime_canvas_refs = runtime_canvas_module_refs(input.runtime_vfs);
    let mut diagnostics = Vec::new();
    let visible_modules = modules
        .into_iter()
        .filter(|module| {
            base_visibility.allows(&module.summary.module_id)
                || runtime_canvas_refs
                    .iter()
                    .any(|module_ref| module_ref == &module.summary.module_id)
        })
        .collect::<Vec<_>>();

    for module_ref in runtime_canvas_refs.iter().filter(|module_ref| {
        !visible_modules
            .iter()
            .any(|module| module.summary.module_id == **module_ref)
    }) {
        diagnostics.push(WorkspaceModuleVisibilityDiagnostic {
            code: "mounted_canvas_not_found".to_string(),
            message: format!(
                "mounted Canvas workspace module `{module_ref}` is not backed by a current project asset"
            ),
            module_ref: Some(module_ref.clone()),
        });
    }

    WorkspaceModuleVisibilityProjection {
        modules: visible_modules,
        base_visibility,
        diagnostics,
    }
}

pub fn project_agent_run_workspace_module_visibility(
    modules: Vec<WorkspaceModuleDescriptor>,
    input: WorkspaceModuleVisibilityInput<'_>,
) -> WorkspaceModuleVisibilityProjection {
    let runtime_canvas_refs = runtime_canvas_module_refs(input.runtime_vfs);
    let mut projection = project_workspace_module_visibility(modules, input);
    projection.modules.retain(|module| {
        module.summary.kind != WorkspaceModuleKind::Canvas
            || runtime_canvas_refs
                .iter()
                .any(|module_ref| module_ref == &module.summary.module_id)
    });
    projection
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use agentdash_domain::DomainError;
    use agentdash_domain::canvas::{Canvas, CanvasRepository};
    use agentdash_domain::common::{Mount, MountCapability};
    use agentdash_domain::extension_package::ExtensionPackageMetadata;
    use agentdash_domain::shared_library::{
        ExtensionBundleKind, ExtensionBundleRef, ExtensionRuntimeActionDefinition,
        ExtensionRuntimeActionKind, ExtensionTemplatePayload, InstalledAssetSource,
        ProjectExtensionInstallation, ProjectExtensionInstallationRepository,
    };
    use agentdash_spi::{
        CapabilityState, ToolCluster, WorkspaceModuleDimension, WorkspaceModuleVisibilityMode,
    };
    use uuid::Uuid;

    use super::*;
    use crate::canvas::build_canvas;
    use agentdash_application_ports::runtime_surface_adoption::AgentFrameRuntimeTarget;

    #[derive(Default)]
    struct FixtureInstallationRepo {
        installations: Mutex<Vec<ProjectExtensionInstallation>>,
    }

    #[async_trait::async_trait]
    impl ProjectExtensionInstallationRepository for FixtureInstallationRepo {
        async fn create(&self, item: &ProjectExtensionInstallation) -> Result<(), DomainError> {
            self.installations.lock().unwrap().push(item.clone());
            Ok(())
        }

        async fn update(&self, _item: &ProjectExtensionInstallation) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            extension_key: &str,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.project_id == project_id && item.extension_key == extension_key)
                .cloned())
        }

        async fn get_by_project_and_id(
            &self,
            project_id: Uuid,
            installation_id: Uuid,
        ) -> Result<Option<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .find(|item| item.project_id == project_id && item.id == installation_id)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn list_enabled_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<ProjectExtensionInstallation>, DomainError> {
            Ok(self
                .installations
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.project_id == project_id && item.enabled)
                .cloned()
                .collect())
        }

        async fn delete(
            &self,
            _project_id: Uuid,
            _installation_id: Uuid,
        ) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    #[derive(Default)]
    struct FixtureCanvasRepo {
        canvases: Mutex<HashMap<(Uuid, String), Canvas>>,
    }

    #[async_trait::async_trait]
    impl CanvasRepository for FixtureCanvasRepo {
        async fn create(&self, canvas: &Canvas) -> Result<(), DomainError> {
            self.canvases
                .lock()
                .unwrap()
                .insert((canvas.project_id, canvas.mount_id.clone()), canvas.clone());
            Ok(())
        }

        async fn update(&self, canvas: &Canvas) -> Result<(), DomainError> {
            self.create(canvas).await
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<Canvas>, DomainError> {
            Ok(self
                .canvases
                .lock()
                .unwrap()
                .values()
                .find(|canvas| canvas.id == id)
                .cloned())
        }

        async fn get_by_mount_id(
            &self,
            project_id: Uuid,
            mount_id: &str,
        ) -> Result<Option<Canvas>, DomainError> {
            Ok(self
                .canvases
                .lock()
                .unwrap()
                .values()
                .find(|canvas| canvas.project_id == project_id && canvas.mount_id == mount_id)
                .cloned())
        }

        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Canvas>, DomainError> {
            Ok(self
                .canvases
                .lock()
                .unwrap()
                .values()
                .filter(|canvas| canvas.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.canvases
                .lock()
                .unwrap()
                .retain(|_, canvas| canvas.id != id);
            Ok(())
        }
    }

    fn manifest(extension_id: &str) -> ExtensionTemplatePayload {
        ExtensionTemplatePayload {
            manifest_version: "2".to_string(),
            extension_id: extension_id.to_string(),
            package: ExtensionPackageMetadata {
                name: extension_id.to_string(),
                version: "0.1.0".to_string(),
            },
            asset_version: "0.1.0".to_string(),
            commands: vec![],
            flags: vec![],
            message_renderers: vec![],
            capability_directives: vec![],
            asset_refs: vec![],
            runtime_actions: vec![ExtensionRuntimeActionDefinition {
                action_key: format!("{extension_id}.profile"),
                kind: ExtensionRuntimeActionKind::SessionRuntime,
                description: "read profile".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "object"}),
                permissions: Vec::new(),
            }],
            protocol_channels: vec![],
            extension_dependencies: vec![],
            workspace_tabs: vec![],
            permissions: vec![],
            fetch_routes: vec![],
            operation_catalog: vec![],
            backend_services: vec![],
            bundles: vec![ExtensionBundleRef {
                kind: ExtensionBundleKind::ExtensionHost,
                entry: "dist/extension.js".to_string(),
                digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            }],
        }
    }

    fn installation(project_id: Uuid, key: &str) -> ProjectExtensionInstallation {
        ProjectExtensionInstallation::new(
            project_id,
            key,
            format!("{key} Extension"),
            manifest(key),
            InstalledAssetSource::new(
                Uuid::new_v4(),
                "integration:test:extension_template:demo",
                "0.1.0",
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        )
        .expect("valid installation")
    }

    async fn fixtures() -> (
        Arc<dyn ProjectExtensionInstallationRepository>,
        Arc<dyn CanvasRepository>,
        Uuid,
    ) {
        let project_id = Uuid::new_v4();
        let install_repo = Arc::new(FixtureInstallationRepo::default());
        install_repo
            .create(&installation(project_id, "demo"))
            .await
            .expect("install extension");
        let canvas_repo = Arc::new(FixtureCanvasRepo::default());
        let canvas = build_canvas(
            project_id,
            Some("cvs-dashboard-a".to_string()),
            "Dashboard A".to_string(),
            "demo canvas".to_string(),
            Default::default(),
        )
        .expect("canvas");
        canvas_repo.create(&canvas).await.expect("create canvas");
        (install_repo, canvas_repo, project_id)
    }

    fn canvas_vfs(mount_ids: impl IntoIterator<Item = impl Into<String>>) -> Vfs {
        Vfs {
            mounts: mount_ids
                .into_iter()
                .map(|mount_id| Mount {
                    id: mount_id.into(),
                    provider: PROVIDER_CANVAS_FS.to_string(),
                    backend_id: String::new(),
                    root_ref: String::new(),
                    capabilities: vec![MountCapability::Read],
                    default_write: false,
                    display_name: String::new(),
                    metadata: serde_json::json!({}),
                })
                .collect(),
            ..Vfs::default()
        }
    }

    fn view(
        workspace_module: WorkspaceModuleDimension,
        runtime_canvas_mount_ids: Vec<String>,
    ) -> AgentRunEffectiveCapabilityView {
        let mut state = CapabilityState::from_clusters([ToolCluster::WorkspaceModule]);
        state.workspace_module = workspace_module;
        let runtime_vfs = canvas_vfs(runtime_canvas_mount_ids);
        state.vfs.active = Some(runtime_vfs.clone());
        AgentRunEffectiveCapabilityView {
            target: AgentFrameRuntimeTarget {
                frame_id: Uuid::new_v4(),
                runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new(
                    "session-a",
                )
                .unwrap(),
            },
            visible_capabilities: state.tool.capabilities.clone(),
            vfs_surface: runtime_vfs,
            mcp_surface: Vec::new(),
            capability_state: state,
        }
    }

    #[tokio::test]
    async fn base_all_returns_extensions_and_canvases() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let projection = resolve_workspace_module_visibility(
            &install_repo,
            &canvas_repo,
            project_id,
            WorkspaceModuleVisibilityInput::from(&view(
                WorkspaceModuleDimension::all(),
                Vec::new(),
            )),
        )
        .await
        .expect("resolve visibility");

        let module_ids = projection
            .modules
            .iter()
            .map(|module| module.summary.module_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(module_ids.len(), 2);
        assert!(module_ids.contains(&"ext:demo"));
        assert!(module_ids.contains(&"canvas:cvs-dashboard-a"));
    }

    #[tokio::test]
    async fn mounted_canvas_extends_allowlist_from_agent_run_view() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let base = WorkspaceModuleDimension {
            mode: WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: vec!["ext:demo".to_string()],
        };
        let projection = resolve_workspace_module_visibility(
            &install_repo,
            &canvas_repo,
            project_id,
            WorkspaceModuleVisibilityInput::from(&view(base, vec!["cvs-dashboard-a".to_string()])),
        )
        .await
        .expect("resolve visibility");

        let module_ids = projection
            .modules
            .iter()
            .map(|module| module.summary.module_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(module_ids.len(), 2);
        assert!(module_ids.contains(&"ext:demo"));
        assert!(module_ids.contains(&"canvas:cvs-dashboard-a"));
    }

    #[tokio::test]
    async fn missing_mounted_canvas_reports_diagnostic_without_fabricating_module() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let base = WorkspaceModuleDimension {
            mode: WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: Vec::new(),
        };
        let projection = resolve_workspace_module_visibility(
            &install_repo,
            &canvas_repo,
            project_id,
            WorkspaceModuleVisibilityInput::from(&view(base, vec!["missing".to_string()])),
        )
        .await
        .expect("resolve visibility");

        assert!(projection.modules.is_empty());
        assert_eq!(projection.diagnostics.len(), 1);
        assert_eq!(projection.diagnostics[0].code, "mounted_canvas_not_found");
        assert_eq!(
            projection.diagnostics[0].module_ref.as_deref(),
            Some("canvas:missing")
        );
    }

    #[tokio::test]
    async fn agent_run_projection_only_exposes_canvases_in_canonical_vfs() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let all = WorkspaceModuleDimension::all();
        let runtime_vfs = Vfs::default();
        let modules = resolve_workspace_module_visibility(
            &install_repo,
            &canvas_repo,
            project_id,
            WorkspaceModuleVisibilityInput {
                base_visibility: &all,
                runtime_vfs: &runtime_vfs,
            },
        )
        .await
        .expect("resolve project visibility")
        .modules;
        let projection = project_agent_run_workspace_module_visibility(
            modules,
            WorkspaceModuleVisibilityInput {
                base_visibility: &all,
                runtime_vfs: &runtime_vfs,
            },
        );

        assert!(
            projection
                .modules
                .iter()
                .any(|module| module.summary.module_id == "ext:demo")
        );
        assert!(
            projection
                .modules
                .iter()
                .all(|module| module.summary.module_id != "canvas:cvs-dashboard-a")
        );
    }
}
