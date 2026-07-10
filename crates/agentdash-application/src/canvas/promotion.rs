use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;
use tar::{Builder, Header};

use agentdash_domain::DomainError;
use agentdash_domain::canvas::Canvas;
use agentdash_domain::extension_package::ExtensionPackageMetadata;
use agentdash_domain::shared_library::{
    ExtensionTemplatePayload, ExtensionWorkspaceTabDefinition,
    ExtensionWorkspaceTabRendererDeclaration,
};

use crate::canvas::build_runtime_snapshot;
use crate::extension_package::validate_extension_package_archive;

pub const CANVAS_EXTENSION_SNAPSHOT_ENTRY: &str = "dist/canvas/runtime-snapshot.json";

#[derive(Debug, Clone, Default)]
pub struct CanvasExtensionPackageInput {
    pub package_version: Option<String>,
    pub asset_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CanvasExtensionPackage {
    pub archive_bytes: Vec<u8>,
    pub archive_digest: String,
    pub manifest_digest: String,
    pub manifest: ExtensionTemplatePayload,
}

pub fn build_canvas_extension_package(
    canvas: &Canvas,
    input: CanvasExtensionPackageInput,
) -> Result<CanvasExtensionPackage, DomainError> {
    let extension_id = canvas_extension_id(canvas);
    let package_version = input
        .package_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0.1.0")
        .to_string();
    let asset_version = input
        .asset_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&package_version)
        .to_string();
    let package_name = format!("@agentdash/{extension_id}");
    let manifest = ExtensionTemplatePayload {
        manifest_version: "2".to_string(),
        extension_id: extension_id.clone(),
        package: ExtensionPackageMetadata {
            name: package_name.clone(),
            version: package_version.clone(),
        },
        asset_version,
        commands: Vec::new(),
        flags: Vec::new(),
        message_renderers: Vec::new(),
        capability_directives: Vec::new(),
        asset_refs: Vec::new(),
        runtime_actions: Vec::new(),
        protocols: Vec::new(),
        extension_dependencies: Vec::new(),
        workspace_tabs: vec![ExtensionWorkspaceTabDefinition {
            type_id: format!("{extension_id}.panel"),
            label: canvas.title.clone(),
            uri_scheme: extension_id.clone(),
            renderer: ExtensionWorkspaceTabRendererDeclaration::CanvasPanel {
                entry: CANVAS_EXTENSION_SNAPSHOT_ENTRY.to_string(),
            },
        }],
        permissions: Vec::new(),
        fetch_routes: Vec::new(),
        operation_catalog: Vec::new(),
        backend_services: Vec::new(),
        bundles: Vec::new(),
    };

    let snapshot = build_runtime_snapshot(canvas, None);
    let package_json = serde_json::json!({
        "name": package_name,
        "version": package_version,
        "private": true,
        "type": "module"
    });
    let archive_bytes = build_archive(vec![
        (
            "agentdash.extension.json",
            serde_json::to_vec_pretty(&manifest).map_err(DomainError::Serialization)?,
        ),
        (
            "package.json",
            serde_json::to_vec_pretty(&package_json).map_err(DomainError::Serialization)?,
        ),
        (
            CANVAS_EXTENSION_SNAPSHOT_ENTRY,
            serde_json::to_vec(&snapshot).map_err(DomainError::Serialization)?,
        ),
    ])?;
    let validated = validate_extension_package_archive(&archive_bytes, None)?;

    Ok(CanvasExtensionPackage {
        archive_bytes,
        archive_digest: validated.archive_digest,
        manifest_digest: validated.manifest_digest,
        manifest: validated.manifest,
    })
}

fn build_archive(files: Vec<(&'static str, Vec<u8>)>) -> Result<Vec<u8>, DomainError> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = Builder::new(encoder);
    for (path, content) in files {
        append_archive_file(&mut builder, path, &content)?;
    }
    builder.finish().map_err(archive_error)?;
    let encoder = builder.into_inner().map_err(archive_error)?;
    encoder.finish().map_err(archive_error)
}

fn append_archive_file<W: Write>(
    builder: &mut Builder<W>,
    path: &str,
    content: &[u8],
) -> Result<(), DomainError> {
    let mut header = Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, path, content)
        .map_err(archive_error)
}

fn archive_error(error: std::io::Error) -> DomainError {
    DomainError::InvalidConfig(format!(
        "Canvas extension package archive 构建失败: {error}"
    ))
}

fn canvas_extension_id(canvas: &Canvas) -> String {
    format!("canvas-{}", normalize_identifier_segment(&canvas.mount_id))
}

fn normalize_identifier_segment(raw: &str) -> String {
    let mut output = String::new();
    let mut last_separator = false;
    for ch in raw.trim().chars() {
        let next = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else if matches!(ch, '-' | '_') {
            ch
        } else {
            '-'
        };
        if matches!(next, '-' | '_') {
            if output.is_empty() || last_separator {
                continue;
            }
            last_separator = true;
            output.push(next);
            continue;
        }
        last_separator = false;
        output.push(next);
    }
    let normalized = output.trim_matches(['-', '_']).to_string();
    if normalized.is_empty() {
        "canvas".to_string()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::extension_package::read_extension_package_archive_file;

    #[test]
    fn builds_canvas_extension_package_with_canvas_panel_manifest() {
        let canvas = Canvas::new(
            Uuid::new_v4(),
            "cvs-demo-canvas".to_string(),
            "Demo Canvas".to_string(),
            "A demo canvas".to_string(),
        );

        let package = build_canvas_extension_package(
            &canvas,
            CanvasExtensionPackageInput {
                package_version: Some("0.2.0".to_string()),
                asset_version: Some("0.2.1".to_string()),
            },
        )
        .expect("package");

        assert!(package.archive_digest.starts_with("sha256:"));
        assert!(package.manifest_digest.starts_with("sha256:"));
        assert_eq!(package.manifest.extension_id, "canvas-cvs-demo-canvas");
        assert_eq!(package.manifest.package.version, "0.2.0");
        assert_eq!(package.manifest.asset_version, "0.2.1");
        assert!(matches!(
            package.manifest.workspace_tabs[0].renderer,
            ExtensionWorkspaceTabRendererDeclaration::CanvasPanel { .. }
        ));
        let snapshot = read_extension_package_archive_file(
            &package.archive_bytes,
            CANVAS_EXTENSION_SNAPSHOT_ENTRY,
        )
        .expect("read snapshot")
        .expect("snapshot exists");
        let value: serde_json::Value = serde_json::from_slice(&snapshot).expect("snapshot json");
        assert_eq!(value["entry"], "src/main.tsx");
        assert_eq!(value["runtime_bridge"]["enabled"], false);
    }
}
