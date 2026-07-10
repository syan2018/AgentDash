use std::sync::Arc;

use agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView;
use agentdash_contracts::workspace_module::WorkspaceModuleDescriptor;
use agentdash_domain::canvas::{Canvas, CanvasRepository};
use agentdash_domain::shared_library::ProjectExtensionInstallationRepository;
use agentdash_spi::WorkspaceModuleDimension;
use uuid::Uuid;

use crate::extension_runtime::extension_runtime_projection_from_installations;
use crate::workspace_module::{
    WorkspaceModuleOperationContext, build_workspace_modules_with_operation_context,
};

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
    pub runtime_refs: Vec<String>,
    pub diagnostics: Vec<WorkspaceModuleVisibilityDiagnostic>,
}

pub async fn resolve_workspace_module_visibility(
    installation_repo: &Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    view: &AgentRunEffectiveCapabilityView,
) -> Result<WorkspaceModuleVisibilityProjection, String> {
    resolve_workspace_module_visibility_with_operation_context(
        installation_repo,
        canvas_repo,
        project_id,
        view,
        &WorkspaceModuleOperationContext::default(),
    )
    .await
}

pub async fn resolve_workspace_module_visibility_with_operation_context(
    installation_repo: &Arc<dyn ProjectExtensionInstallationRepository>,
    canvas_repo: &Arc<dyn CanvasRepository>,
    project_id: Uuid,
    view: &AgentRunEffectiveCapabilityView,
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

    let base_visibility = view.capability_state.workspace_module.clone();
    let runtime_refs = view.visible_workspace_module_refs.clone();
    let mut diagnostics = Vec::new();
    let modules =
        build_workspace_modules_with_operation_context(&projection, &canvases, operation_context);
    let visible_modules = modules
        .into_iter()
        .filter(|module| {
            base_visibility.allows(&module.summary.module_id)
                || runtime_refs
                    .iter()
                    .any(|module_ref| module_ref == &module.summary.module_id)
        })
        .collect::<Vec<_>>();

    for module_ref in runtime_refs.iter().filter(|module_ref| {
        !visible_modules
            .iter()
            .any(|module| module.summary.module_id == **module_ref)
    }) {
        diagnostics.push(WorkspaceModuleVisibilityDiagnostic {
            code: "runtime_ref_not_found".to_string(),
            message: format!(
                "runtime workspace module ref `{module_ref}` is not backed by an enabled module"
            ),
            module_ref: Some(module_ref.clone()),
        });
    }

    Ok(WorkspaceModuleVisibilityProjection {
        modules: visible_modules,
        base_visibility,
        runtime_refs,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use agentdash_domain::DomainError;
    use agentdash_domain::canvas::{Canvas, CanvasRepository};
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
    use agentdash_application_ports::agent_run_surface::AgentRunGrantProjection;
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
            protocols: vec![],
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

    fn view(
        workspace_module: WorkspaceModuleDimension,
        runtime_refs: Vec<String>,
    ) -> AgentRunEffectiveCapabilityView {
        let mut state = CapabilityState::from_clusters([ToolCluster::WorkspaceModule]);
        state.workspace_module = workspace_module;
        AgentRunEffectiveCapabilityView {
            target: AgentFrameRuntimeTarget {
                frame_id: Uuid::new_v4(),
                delivery_runtime_session_id: "session-a".to_string(),
            },
            visible_capabilities: state.tool.capabilities.clone(),
            vfs_surface: state.vfs.active.clone().unwrap_or_default(),
            mcp_surface: Vec::new(),
            capability_state: state,
            visible_workspace_module_refs: runtime_refs,
            grant_projection: AgentRunGrantProjection::default(),
        }
    }

    #[tokio::test]
    async fn base_all_returns_extensions_and_canvases() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let projection = resolve_workspace_module_visibility(
            &install_repo,
            &canvas_repo,
            project_id,
            &view(WorkspaceModuleDimension::all(), Vec::new()),
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
    async fn runtime_refs_extend_allowlist_from_agent_run_view() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let base = WorkspaceModuleDimension {
            mode: WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: vec!["ext:demo".to_string()],
        };
        let projection = resolve_workspace_module_visibility(
            &install_repo,
            &canvas_repo,
            project_id,
            &view(base, vec!["canvas:cvs-dashboard-a".to_string()]),
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
        assert_eq!(
            projection.runtime_refs,
            vec!["canvas:cvs-dashboard-a".to_string()]
        );
    }

    #[tokio::test]
    async fn missing_runtime_ref_reports_diagnostic_without_fabricating_module() {
        let (install_repo, canvas_repo, project_id) = fixtures().await;
        let base = WorkspaceModuleDimension {
            mode: WorkspaceModuleVisibilityMode::Allowlist,
            allowed_module_ids: Vec::new(),
        };
        let projection = resolve_workspace_module_visibility(
            &install_repo,
            &canvas_repo,
            project_id,
            &view(base, vec!["canvas:missing".to_string()]),
        )
        .await
        .expect("resolve visibility");

        assert!(projection.modules.is_empty());
        assert_eq!(projection.diagnostics.len(), 1);
        assert_eq!(projection.diagnostics[0].code, "runtime_ref_not_found");
        assert_eq!(
            projection.diagnostics[0].module_ref.as_deref(),
            Some("canvas:missing")
        );
    }
}
