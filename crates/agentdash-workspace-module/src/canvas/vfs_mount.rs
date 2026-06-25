use agentdash_domain::DomainError;
use agentdash_domain::canvas::{Canvas, CanvasAccessProjection, CanvasDataBinding};
use agentdash_domain::common::{Mount, MountCapability, Vfs};

use agentdash_application_vfs::PROVIDER_CANVAS_FS;

use crate::canvas::{
    CanvasResolvedBindingFile, canvas_provider_root_ref, canvas_vfs_mount_id,
    upsert_canvas_binding, validate_canvas_contract,
};

pub const CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY: &str = "runtime_data_bindings";

pub fn build_canvas_mount_id(canvas: &Canvas) -> String {
    canvas_vfs_mount_id(&canvas.mount_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasMountAccess {
    pub runtime_write_allowed: bool,
}

impl CanvasMountAccess {
    pub const fn read_only() -> Self {
        Self {
            runtime_write_allowed: false,
        }
    }

    pub const fn writable() -> Self {
        Self {
            runtime_write_allowed: true,
        }
    }

    pub const fn from_runtime_write_allowed(runtime_write_allowed: bool) -> Self {
        Self {
            runtime_write_allowed,
        }
    }
}

impl From<CanvasAccessProjection> for CanvasMountAccess {
    fn from(access: CanvasAccessProjection) -> Self {
        Self::from_runtime_write_allowed(access.runtime_write_allowed)
    }
}

impl From<&CanvasAccessProjection> for CanvasMountAccess {
    fn from(access: &CanvasAccessProjection) -> Self {
        Self::from_runtime_write_allowed(access.runtime_write_allowed)
    }
}

impl From<bool> for CanvasMountAccess {
    fn from(runtime_write_allowed: bool) -> Self {
        Self::from_runtime_write_allowed(runtime_write_allowed)
    }
}

pub fn build_canvas_mount(canvas: &Canvas, access: impl Into<CanvasMountAccess>) -> Mount {
    let access = access.into();
    let mut capabilities = vec![
        MountCapability::Read,
        MountCapability::List,
        MountCapability::Search,
    ];
    if access.runtime_write_allowed {
        capabilities.insert(1, MountCapability::Write);
    }

    Mount {
        id: build_canvas_mount_id(canvas),
        provider: PROVIDER_CANVAS_FS.to_string(),
        backend_id: String::new(),
        root_ref: canvas_provider_root_ref(canvas.id),
        capabilities,
        default_write: false,
        display_name: if canvas.title.trim().is_empty() {
            format!("Canvas {}", canvas.id)
        } else {
            canvas.title.clone()
        },
        metadata: serde_json::json!({
            "canvas_id": canvas.id.to_string(),
            "canvas_mount_id": canvas.mount_id,
            "vfs_mount_id": canvas_vfs_mount_id(&canvas.mount_id),
            "project_id": canvas.project_id.to_string(),
            "entry_file": canvas.entry_file,
        }),
    }
}

pub fn append_canvas_mount(vfs: &mut Vfs, canvas: &Canvas, access: impl Into<CanvasMountAccess>) {
    let mut mount = build_canvas_mount(canvas, access);
    if let Some(existing) = vfs
        .mounts
        .iter_mut()
        .find(|existing| existing.id == mount.id)
    {
        preserve_canvas_mount_runtime_data_bindings(existing, &mut mount);
        *existing = mount;
    } else {
        vfs.mounts.push(mount);
    }
}

pub fn append_canvas_mounts(
    vfs: &mut Vfs,
    canvases: &[Canvas],
    access: impl Into<CanvasMountAccess> + Copy,
) {
    for canvas in canvases {
        append_canvas_mount(vfs, canvas, access);
    }
}

pub fn refresh_canvas_mount_binding_files(
    vfs: &mut Vfs,
    canvas: &Canvas,
    binding_files: &[CanvasResolvedBindingFile],
) {
    let mount_id = build_canvas_mount_id(canvas);
    let Some(mount) = vfs.mounts.iter_mut().find(|mount| mount.id == mount_id) else {
        return;
    };
    let Some(metadata) = mount.metadata.as_object_mut() else {
        return;
    };
    if binding_files.is_empty() {
        metadata.remove("binding_files");
        return;
    }
    metadata.insert(
        "binding_files".to_string(),
        serde_json::to_value(binding_files).unwrap_or_else(|_| serde_json::json!([])),
    );
}

pub fn canvas_mount_runtime_data_bindings(mount: &Mount) -> Vec<CanvasDataBinding> {
    mount
        .metadata
        .get(CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY)
        .and_then(|value| serde_json::from_value::<Vec<CanvasDataBinding>>(value.clone()).ok())
        .unwrap_or_default()
}

pub fn canvas_with_runtime_data_bindings(canvas: &Canvas, mount: &Mount) -> Canvas {
    let runtime_bindings = canvas_mount_runtime_data_bindings(mount);
    let mut effective = canvas.clone();
    effective.bindings = merge_canvas_runtime_data_bindings(&canvas.bindings, &runtime_bindings);
    effective
}

pub fn upsert_canvas_runtime_data_binding(
    vfs: &mut Vfs,
    canvas: &Canvas,
    binding: CanvasDataBinding,
) -> Result<Vec<CanvasDataBinding>, DomainError> {
    let mount_id = build_canvas_mount_id(canvas);
    let mount = vfs
        .mounts
        .iter_mut()
        .find(|mount| mount.id == mount_id)
        .ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "Canvas runtime mount `{mount_id}` 不存在，无法挂接数据"
            ))
        })?;
    let binding_alias = binding.alias.trim().to_string();
    let mut effective = canvas_with_runtime_data_bindings(canvas, mount);
    upsert_canvas_binding(&mut effective, binding)?;
    let normalized_binding = effective
        .bindings
        .iter()
        .find(|item| item.alias == binding_alias)
        .cloned()
        .ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "Canvas runtime binding `{binding_alias}` 规范化失败"
            ))
        })?;

    let mut runtime_bindings = canvas_mount_runtime_data_bindings(mount);
    if let Some(existing) = runtime_bindings
        .iter_mut()
        .find(|item| item.alias == normalized_binding.alias)
    {
        *existing = normalized_binding;
    } else {
        runtime_bindings.push(normalized_binding);
    }

    let mut validation_canvas = canvas.clone();
    validation_canvas.bindings =
        merge_canvas_runtime_data_bindings(&canvas.bindings, &runtime_bindings);
    validate_canvas_contract(&validation_canvas)?;

    let metadata = ensure_mount_metadata_object(mount);
    metadata.insert(
        CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY.to_string(),
        serde_json::to_value(&runtime_bindings)
            .map_err(|error| DomainError::InvalidConfig(error.to_string()))?,
    );

    Ok(validation_canvas.bindings)
}

fn merge_canvas_runtime_data_bindings(
    source_bindings: &[CanvasDataBinding],
    runtime_bindings: &[CanvasDataBinding],
) -> Vec<CanvasDataBinding> {
    let mut merged = source_bindings.to_vec();
    for runtime_binding in runtime_bindings {
        if let Some(existing) = merged
            .iter_mut()
            .find(|binding| binding.alias == runtime_binding.alias)
        {
            *existing = runtime_binding.clone();
        } else {
            merged.push(runtime_binding.clone());
        }
    }
    merged
}

fn preserve_canvas_mount_runtime_data_bindings(existing: &Mount, next: &mut Mount) {
    let Some(runtime_bindings) = existing
        .metadata
        .get(CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY)
        .cloned()
    else {
        return;
    };
    ensure_mount_metadata_object(next).insert(
        CANVAS_RUNTIME_DATA_BINDINGS_METADATA_KEY.to_string(),
        runtime_bindings,
    );
}

fn ensure_mount_metadata_object(
    mount: &mut Mount,
) -> &mut serde_json::Map<String, serde_json::Value> {
    if !mount.metadata.is_object() {
        mount.metadata = serde_json::json!({});
    }
    mount.metadata.as_object_mut().expect("metadata object")
}

#[cfg(test)]
mod tests {
    use super::*;

    use uuid::Uuid;

    fn canvas(mount_id: &str) -> Canvas {
        Canvas::new(
            Uuid::new_v4(),
            mount_id.to_string(),
            "Dashboard".to_string(),
            String::new(),
        )
    }

    fn relay_mount(id: &str) -> Mount {
        Mount {
            id: id.to_string(),
            provider: "relay_fs".to_string(),
            backend_id: "backend-a".to_string(),
            root_ref: format!("D:/{id}"),
            capabilities: vec![MountCapability::Read],
            default_write: false,
            display_name: id.to_string(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn append_canvas_mounts_replaces_existing_mount_without_reordering() {
        let canvas = canvas("cvs-dashboard-a");
        let mut vfs = Vfs {
            mounts: vec![relay_mount("workspace")],
            default_mount_id: Some("workspace".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        append_canvas_mounts(
            &mut vfs,
            std::slice::from_ref(&canvas),
            CanvasMountAccess::writable(),
        );
        vfs.mounts.push(relay_mount("tail"));
        let before_order = vfs
            .mounts
            .iter()
            .map(|mount| mount.id.clone())
            .collect::<Vec<_>>();

        append_canvas_mounts(
            &mut vfs,
            std::slice::from_ref(&canvas),
            CanvasMountAccess::writable(),
        );

        let after_order = vfs
            .mounts
            .iter()
            .map(|mount| mount.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(after_order, before_order);
    }

    #[test]
    fn build_canvas_mount_caps_write_to_runtime_access() {
        let canvas = canvas("cvs-dashboard-a");

        let writable = build_canvas_mount(&canvas, CanvasMountAccess::writable());
        assert!(writable.supports(MountCapability::Read));
        assert!(writable.supports(MountCapability::Write));
        assert!(writable.supports(MountCapability::List));
        assert!(writable.supports(MountCapability::Search));
        assert!(!writable.default_write);

        let read_only = build_canvas_mount(&canvas, CanvasMountAccess::read_only());
        assert!(read_only.supports(MountCapability::Read));
        assert!(!read_only.supports(MountCapability::Write));
        assert!(read_only.supports(MountCapability::List));
        assert!(read_only.supports(MountCapability::Search));
        assert!(!read_only.default_write);

        let projection_mount = build_canvas_mount(
            &canvas,
            CanvasAccessProjection {
                runtime_write_allowed: false,
                ..CanvasAccessProjection::default()
            },
        );
        assert!(!projection_mount.supports(MountCapability::Write));
    }

    #[test]
    fn refresh_canvas_mount_binding_files_omits_empty_binding_metadata() {
        let canvas = canvas("cvs-dashboard-a");
        let mut vfs = Vfs::default();
        append_canvas_mounts(
            &mut vfs,
            std::slice::from_ref(&canvas),
            CanvasMountAccess::writable(),
        );

        refresh_canvas_mount_binding_files(&mut vfs, &canvas, &[]);

        let mount = vfs
            .mounts
            .iter()
            .find(|mount| mount.id == canvas.mount_id)
            .expect("canvas mount should exist");
        assert!(
            mount
                .metadata
                .get("binding_files")
                .is_none_or(serde_json::Value::is_null),
            "empty Canvas binding projection should not create binding_files metadata"
        );
    }
}
