use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use agentdash_domain::canvas::{Canvas, CanvasImportMap};
use agentdash_spi::AddressSpace;

use crate::address_space::{RelayAddressSpaceService, parse_mount_uri};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasRuntimeSnapshot {
    pub canvas_id: uuid::Uuid,
    pub session_id: Option<String>,
    pub entry: String,
    pub files: Vec<CanvasRuntimeFile>,
    pub bindings: Vec<CanvasRuntimeBinding>,
    pub import_map: CanvasImportMap,
    pub libraries: Vec<String>,
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

    CanvasRuntimeSnapshot {
        canvas_id: canvas.id,
        session_id,
        entry: canvas.entry_file.clone(),
        files,
        bindings,
        import_map: canvas.sandbox_config.import_map.clone(),
        libraries: canvas.sandbox_config.libraries.clone(),
    }
}

pub async fn build_runtime_snapshot_with_bindings(
    canvas: &Canvas,
    session_id: Option<String>,
    address_space: Option<&AddressSpace>,
    address_space_service: &RelayAddressSpaceService,
) -> CanvasRuntimeSnapshot {
    let mut snapshot = build_runtime_snapshot(canvas, session_id);
    let Some(address_space) = address_space else {
        return snapshot;
    };

    for binding in &mut snapshot.bindings {
        let Ok(resource_ref) = parse_mount_uri(&binding.source_uri, address_space) else {
            continue;
        };
        let Ok(result) = address_space_service
            .read_text(address_space, &resource_ref, None, None)
            .await
        else {
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
    use uuid::Uuid;

    use agentdash_domain::canvas::{CanvasDataBinding, CanvasFile};

    use super::*;

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
}
