use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use agentdash_domain::canvas::{Canvas, CanvasImportMap};
use agentdash_spi::Vfs;

use crate::runtime_gateway::RuntimeSurface;
use crate::vfs::{VfsService, ResolvedVfsSurfaceSource, parse_mount_uri};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasRuntimeSnapshot {
    pub canvas_id: uuid::Uuid,
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_surface_ref: Option<String>,
    pub entry: String,
    pub files: Vec<CanvasRuntimeFile>,
    pub bindings: Vec<CanvasRuntimeBinding>,
    pub import_map: CanvasImportMap,
    pub libraries: Vec<String>,
    pub runtime_bridge: CanvasRuntimeBridgeSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasRuntimeFile {
    pub path: String,
    pub content: String,
    pub file_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasRuntimeBinding {
    pub alias: String,
    pub source_uri: String,
    pub data_path: String,
    pub content_type: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasRuntimeBridgeSnapshot {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<RuntimeSurface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

impl CanvasRuntimeBridgeSnapshot {
    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            surface: None,
            disabled_reason: Some(reason.into()),
        }
    }

    pub fn enabled(surface: RuntimeSurface) -> Self {
        Self {
            enabled: true,
            surface: Some(surface),
            disabled_reason: None,
        }
    }
}

pub fn build_runtime_snapshot(
    canvas: &Canvas,
    session_id: Option<String>,
) -> CanvasRuntimeSnapshot {
    let mut files = canvas
        .files
        .iter()
        .map(|file| CanvasRuntimeFile {
            path: file.path.clone(),
            content: file.content.clone(),
            file_type: infer_file_type(&file.path).to_string(),
        })
        .collect::<Vec<_>>();

    let existing_paths = files
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();

    let bindings = canvas
        .bindings
        .iter()
        .map(|binding| CanvasRuntimeBinding {
            alias: binding.alias.clone(),
            source_uri: binding.source_uri.clone(),
            data_path: format!("bindings/{}.json", binding.alias),
            content_type: binding.content_type.clone(),
            resolved: false,
        })
        .collect::<Vec<_>>();

    for binding in &bindings {
        if existing_paths.contains(&binding.data_path) {
            continue;
        }
        files.push(CanvasRuntimeFile {
            path: binding.data_path.clone(),
            content: "null".to_string(),
            file_type: "data".to_string(),
        });
    }

    let runtime_bridge = if session_id.is_some() {
        CanvasRuntimeBridgeSnapshot::disabled("Canvas runtime bridge surface 尚未装配")
    } else {
        CanvasRuntimeBridgeSnapshot::disabled("Canvas runtime snapshot 尚未绑定 Session")
    };

    CanvasRuntimeSnapshot {
        canvas_id: canvas.id,
        session_id,
        resource_surface_ref: None,
        entry: canvas.entry_file.clone(),
        files,
        bindings,
        import_map: canvas.sandbox_config.import_map.clone(),
        libraries: canvas.sandbox_config.libraries.clone(),
        runtime_bridge,
    }
}

pub async fn build_runtime_snapshot_with_bindings(
    canvas: &Canvas,
    session_id: Option<String>,
    vfs: Option<&Vfs>,
    vfs_service: &VfsService,
) -> CanvasRuntimeSnapshot {
    let mut snapshot = build_runtime_snapshot(canvas, session_id);
    let Some(vfs) = vfs else {
        return snapshot;
    };
    if let Some(session_id) = snapshot.session_id.as_deref() {
        snapshot.resource_surface_ref = Some(
            ResolvedVfsSurfaceSource::SessionRuntime {
                session_id: session_id.to_string(),
            }
            .surface_ref(),
        );
    }

    for binding in &mut snapshot.bindings {
        let Ok(resource_ref) = parse_mount_uri(&binding.source_uri, vfs) else {
            continue;
        };
        let Ok(result) = vfs_service.read_text(vfs, &resource_ref, None, None).await else {
            continue;
        };

        if let Some(file) = snapshot
            .files
            .iter_mut()
            .find(|file| file.path == binding.data_path)
        {
            file.content = result.content;
            file.file_type = "data".to_string();
            binding.resolved = true;
        }
    }

    snapshot
}

fn infer_file_type(path: &str) -> &'static str {
    if path.ends_with(".json") {
        "data"
    } else if path.ends_with(".css") {
        "style"
    } else if path.ends_with(".html") {
        "markup"
    } else {
        "code"
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use uuid::Uuid;

    use agentdash_domain::canvas::{CanvasDataBinding, CanvasFile};
    use serde_json::json;

    use super::*;
    use crate::runtime_gateway::{
        RuntimeActionDescriptor, RuntimeActionKey, RuntimeActionKind, RuntimeContext,
        RuntimeSurface,
    };
    use crate::vfs::MountProviderRegistry;

    #[test]
    fn build_runtime_snapshot_marks_binding_unresolved_until_session_wiring_exists() {
        let mut canvas = Canvas::new(
            Uuid::new_v4(),
            "demo".to_string(),
            "Demo".to_string(),
            String::new(),
        );
        canvas.files = vec![CanvasFile::new(
            "src/main.tsx".to_string(),
            "console.log('ok')".to_string(),
        )];
        canvas.bindings = vec![CanvasDataBinding::new(
            "stats".to_string(),
            "lifecycle://active/artifacts/1".to_string(),
        )];

        let snapshot = build_runtime_snapshot(&canvas, Some("session-1".to_string()));

        assert_eq!(snapshot.entry, "src/main.tsx");
        assert!(snapshot.resource_surface_ref.is_none());
        assert!(snapshot.files.iter().any(|file| file.file_type == "code"));
        assert!(
            snapshot
                .files
                .iter()
                .any(|file| file.path == "bindings/stats.json" && file.file_type == "data")
        );
        assert_eq!(snapshot.bindings[0].data_path, "bindings/stats.json");
        assert!(!snapshot.bindings[0].resolved);
    }

    #[test]
    fn build_runtime_snapshot_disables_bridge_without_session_surface() {
        let canvas = Canvas::new(
            Uuid::new_v4(),
            "demo".to_string(),
            "Demo".to_string(),
            String::new(),
        );

        let snapshot = build_runtime_snapshot(&canvas, None);

        assert!(snapshot.resource_surface_ref.is_none());
        assert!(!snapshot.runtime_bridge.enabled);
        assert!(snapshot.runtime_bridge.surface.is_none());
        assert!(
            snapshot
                .runtime_bridge
                .disabled_reason
                .as_deref()
                .unwrap_or_default()
                .contains("Session")
        );
    }

    #[tokio::test]
    async fn build_runtime_snapshot_with_runtime_vfs_exposes_resource_surface_ref() {
        let canvas = Canvas::new(
            Uuid::new_v4(),
            "demo".to_string(),
            "Demo".to_string(),
            String::new(),
        );
        let vfs = Vfs::default();
        let service = VfsService::new(Arc::new(MountProviderRegistry::default()));

        let snapshot = build_runtime_snapshot_with_bindings(
            &canvas,
            Some("session-1".to_string()),
            Some(&vfs),
            &service,
        )
        .await;

        assert_eq!(
            snapshot.resource_surface_ref.as_deref(),
            Some("session-runtime:session-1")
        );
    }

    #[test]
    fn canvas_runtime_bridge_snapshot_can_attach_actor_surface() {
        let surface = RuntimeSurface {
            context: RuntimeContext::Session {
                session_id: "session-1".to_string(),
                project_id: None,
                workspace_id: None,
            },
            actions: vec![RuntimeActionDescriptor {
                action_key: RuntimeActionKey::parse("mcp.list_tools").expect("valid action key"),
                kind: RuntimeActionKind::SessionRuntime,
                description: Some("list tools".to_string()),
                input_schema: Some(json!({ "type": "object" })),
                output_schema: None,
                default_policy: Default::default(),
            }],
        };

        let bridge = CanvasRuntimeBridgeSnapshot::enabled(surface);

        assert!(bridge.enabled);
        assert_eq!(
            bridge.surface.as_ref().unwrap().actions[0]
                .action_key
                .as_str(),
            "mcp.list_tools"
        );
        assert!(bridge.disabled_reason.is_none());
    }
}
