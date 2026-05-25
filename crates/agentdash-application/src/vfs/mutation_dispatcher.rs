use std::sync::Arc;

use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
use agentdash_spi::platform::auth::AuthIdentity;
use thiserror::Error;
use uuid::Uuid;

use crate::runtime::{Mount, MountCapability, Vfs};

use super::inline_persistence::{DbInlineContentPersister, InlineContentOverlay};
use super::mount::{PROVIDER_INLINE_FS, parse_inline_mount_owner};
use super::path::{normalize_mount_relative_path, resolve_mount};
use super::provider::MountProviderRegistry;
use super::relay_service::RelayVfsService;
use super::types::{ApplyPatchResult, ResourceRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineStorageKey {
    pub owner_kind: InlineFileOwnerKind,
    pub owner_id: Uuid,
    pub container_id: String,
}

pub fn inline_storage_key_from_mount(mount: &Mount) -> Result<InlineStorageKey, VfsMutationError> {
    let (owner_kind, owner_id, container_id) =
        parse_inline_mount_owner(mount).map_err(VfsMutationError::InvalidMount)?;
    Ok(InlineStorageKey {
        owner_kind,
        owner_id,
        container_id,
    })
}

#[derive(Debug, Clone)]
pub struct TextMutationResult {
    pub path: String,
    pub size: u64,
    pub persisted: bool,
    pub content_kind: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BinaryMutationResult {
    pub path: String,
    pub size: u64,
    pub content_kind: String,
    pub mime_type: String,
}

#[derive(Debug, Error)]
pub enum VfsMutationError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    InvalidMount(String),
    #[error("{0}")]
    Provider(String),
    #[error("{0}")]
    Internal(String),
}

impl From<agentdash_domain::DomainError> for VfsMutationError {
    fn from(error: agentdash_domain::DomainError) -> Self {
        match error {
            agentdash_domain::DomainError::NotFound { .. } => Self::NotFound(error.to_string()),
            agentdash_domain::DomainError::InvalidConfig(_) => Self::BadRequest(error.to_string()),
            _ => Self::Internal(error.to_string()),
        }
    }
}

#[derive(Clone)]
pub struct VfsMutationDispatcher {
    relay_service: Arc<RelayVfsService>,
    inline_file_repo: Arc<dyn InlineFileRepository>,
    mount_provider_registry: Arc<MountProviderRegistry>,
}

impl VfsMutationDispatcher {
    pub fn new(
        relay_service: Arc<RelayVfsService>,
        inline_file_repo: Arc<dyn InlineFileRepository>,
        mount_provider_registry: Arc<MountProviderRegistry>,
    ) -> Self {
        Self {
            relay_service,
            inline_file_repo,
            mount_provider_registry,
        }
    }

    pub async fn create_text(
        &self,
        vfs: &Vfs,
        target: ResourceRef,
        content: &str,
        identity: Option<&AuthIdentity>,
    ) -> Result<TextMutationResult, VfsMutationError> {
        let mount = resolve_mount(vfs, &target.mount_id, MountCapability::Write)
            .map_err(VfsMutationError::BadRequest)?;
        let path = normalize_mount_relative_path(&target.path, false)
            .map_err(VfsMutationError::BadRequest)?;
        self.ensure_edit_capability(mount, "create")?;

        if mount.provider == PROVIDER_INLINE_FS {
            let storage_key = inline_storage_key_from_mount(mount)?;
            if self
                .inline_file_repo
                .get_file(
                    storage_key.owner_kind,
                    storage_key.owner_id,
                    &storage_key.container_id,
                    &path,
                )
                .await?
                .is_some()
            {
                return Err(VfsMutationError::Conflict(format!(
                    "目标文件已存在: {path}"
                )));
            }
            self.upsert_inline_text(&storage_key, &path, content)
                .await?;
            return Ok(text_result(path, content.len() as u64, true));
        }

        self.relay_service
            .create_text(
                vfs,
                &ResourceRef {
                    mount_id: target.mount_id,
                    path: path.clone(),
                },
                content,
                None,
                identity,
            )
            .await
            .map_err(VfsMutationError::Provider)?;
        Ok(text_result(path, content.len() as u64, false))
    }

    pub async fn write_text(
        &self,
        vfs: &Vfs,
        target: ResourceRef,
        content: &str,
        identity: Option<&AuthIdentity>,
    ) -> Result<TextMutationResult, VfsMutationError> {
        let mount = resolve_mount(vfs, &target.mount_id, MountCapability::Write)
            .map_err(VfsMutationError::BadRequest)?;
        let path = normalize_mount_relative_path(&target.path, false)
            .map_err(VfsMutationError::BadRequest)?;

        if mount.provider == PROVIDER_INLINE_FS {
            let storage_key = inline_storage_key_from_mount(mount)?;
            self.upsert_inline_text(&storage_key, &path, content)
                .await?;
            return Ok(text_result(path, content.len() as u64, true));
        }

        self.relay_service
            .write_text(
                vfs,
                &ResourceRef {
                    mount_id: target.mount_id,
                    path: path.clone(),
                },
                content,
                None,
                identity,
            )
            .await
            .map_err(VfsMutationError::Provider)?;
        Ok(text_result(path, content.len() as u64, false))
    }

    pub async fn delete_text(
        &self,
        vfs: &Vfs,
        target: ResourceRef,
        identity: Option<&AuthIdentity>,
    ) -> Result<(), VfsMutationError> {
        let mount = resolve_mount(vfs, &target.mount_id, MountCapability::Write)
            .map_err(VfsMutationError::BadRequest)?;
        let path = normalize_mount_relative_path(&target.path, false)
            .map_err(VfsMutationError::BadRequest)?;
        self.ensure_edit_capability(mount, "delete")?;

        if mount.provider == PROVIDER_INLINE_FS {
            let storage_key = inline_storage_key_from_mount(mount)?;
            self.ensure_inline_file_exists(&storage_key, &path).await?;
            self.inline_file_repo
                .delete_file(
                    storage_key.owner_kind,
                    storage_key.owner_id,
                    &storage_key.container_id,
                    &path,
                )
                .await?;
            return Ok(());
        }

        self.relay_service
            .delete_text(
                vfs,
                &ResourceRef {
                    mount_id: target.mount_id,
                    path,
                },
                None,
                identity,
            )
            .await
            .map_err(VfsMutationError::Provider)
    }

    pub async fn rename_text(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        from_path: &str,
        to_path: &str,
        identity: Option<&AuthIdentity>,
    ) -> Result<(String, String), VfsMutationError> {
        let mount = resolve_mount(vfs, mount_id, MountCapability::Write)
            .map_err(VfsMutationError::BadRequest)?;
        let from_path = normalize_mount_relative_path(from_path, false)
            .map_err(VfsMutationError::BadRequest)?;
        let to_path =
            normalize_mount_relative_path(to_path, false).map_err(VfsMutationError::BadRequest)?;
        if from_path == to_path {
            return Ok((from_path, to_path));
        }
        self.ensure_edit_capability(mount, "rename")?;

        if mount.provider == PROVIDER_INLINE_FS {
            let storage_key = inline_storage_key_from_mount(mount)?;
            let mut file = self
                .ensure_inline_file_exists(&storage_key, &from_path)
                .await?;
            if self
                .inline_file_repo
                .get_file(
                    storage_key.owner_kind,
                    storage_key.owner_id,
                    &storage_key.container_id,
                    &to_path,
                )
                .await?
                .is_some()
            {
                return Err(VfsMutationError::Conflict(format!(
                    "目标文件已存在: {to_path}"
                )));
            }
            file.path = to_path.clone();
            self.inline_file_repo.upsert_file(&file).await?;
            self.inline_file_repo
                .delete_file(
                    storage_key.owner_kind,
                    storage_key.owner_id,
                    &storage_key.container_id,
                    &from_path,
                )
                .await?;
            return Ok((from_path, to_path));
        }

        self.relay_service
            .rename_text(vfs, mount_id, &from_path, &to_path, None, identity)
            .await
            .map_err(VfsMutationError::Provider)?;
        Ok((from_path, to_path))
    }

    pub async fn apply_patch(
        &self,
        vfs: &Vfs,
        mount_id: &str,
        patch: &str,
        identity: Option<&AuthIdentity>,
    ) -> Result<ApplyPatchResult, VfsMutationError> {
        let mount = resolve_mount(vfs, mount_id, MountCapability::Write)
            .map_err(VfsMutationError::BadRequest)?;
        if mount.provider == PROVIDER_INLINE_FS {
            let overlay = self.db_inline_overlay();
            return self
                .relay_service
                .apply_patch(vfs, mount_id, patch, Some(&overlay), identity)
                .await
                .map_err(VfsMutationError::Provider);
        }

        self.relay_service
            .apply_patch(vfs, mount_id, patch, None, identity)
            .await
            .map_err(VfsMutationError::Provider)
    }

    pub async fn upload_inline_binary(
        &self,
        vfs: &Vfs,
        target: ResourceRef,
        bytes: Vec<u8>,
        mime_type: String,
        _identity: Option<&AuthIdentity>,
    ) -> Result<BinaryMutationResult, VfsMutationError> {
        let mount = resolve_mount(vfs, &target.mount_id, MountCapability::Write)
            .map_err(VfsMutationError::BadRequest)?;
        let path = normalize_mount_relative_path(&target.path, false)
            .map_err(VfsMutationError::BadRequest)?;
        self.ensure_edit_capability(mount, "create")?;
        if mount.provider != PROVIDER_INLINE_FS {
            return Err(VfsMutationError::BadRequest(
                "图片上传目前仅支持 inline_fs mount".to_string(),
            ));
        }
        let storage_key = inline_storage_key_from_mount(mount)?;
        let size = bytes.len() as u64;
        let file = InlineFile::new_binary(
            storage_key.owner_kind,
            storage_key.owner_id,
            &storage_key.container_id,
            &path,
            bytes,
            mime_type.clone(),
        );
        self.inline_file_repo.upsert_file(&file).await?;
        Ok(BinaryMutationResult {
            path,
            size,
            content_kind: "binary".to_string(),
            mime_type,
        })
    }

    fn ensure_edit_capability(
        &self,
        mount: &Mount,
        operation: &str,
    ) -> Result<(), VfsMutationError> {
        let supported = if mount.provider == PROVIDER_INLINE_FS {
            mount.supports(MountCapability::Write)
        } else {
            self.mount_provider_registry
                .get(&mount.provider)
                .map(|provider| provider.edit_capabilities(mount))
                .map(|capabilities| match operation {
                    "create" => capabilities.create,
                    "delete" => capabilities.delete,
                    "rename" => capabilities.rename,
                    _ => false,
                })
                .unwrap_or(false)
        };
        if supported {
            Ok(())
        } else {
            Err(VfsMutationError::BadRequest(format!(
                "挂载点 \"{}\" 不支持 {operation} 操作",
                mount.display_name
            )))
        }
    }

    async fn upsert_inline_text(
        &self,
        storage_key: &InlineStorageKey,
        path: &str,
        content: &str,
    ) -> Result<(), VfsMutationError> {
        let file = InlineFile::new(
            storage_key.owner_kind,
            storage_key.owner_id,
            &storage_key.container_id,
            path,
            content,
        );
        self.inline_file_repo.upsert_file(&file).await?;
        Ok(())
    }

    async fn ensure_inline_file_exists(
        &self,
        storage_key: &InlineStorageKey,
        path: &str,
    ) -> Result<InlineFile, VfsMutationError> {
        self.inline_file_repo
            .get_file(
                storage_key.owner_kind,
                storage_key.owner_id,
                &storage_key.container_id,
                path,
            )
            .await?
            .ok_or_else(|| VfsMutationError::NotFound(format!("文件不存在: {path}")))
    }

    fn db_inline_overlay(&self) -> InlineContentOverlay {
        let persister = DbInlineContentPersister::new(self.inline_file_repo.clone());
        InlineContentOverlay::new(Arc::new(persister))
    }
}

fn text_result(path: String, size: u64, persisted: bool) -> TextMutationResult {
    TextMutationResult {
        path,
        size,
        persisted,
        content_kind: "text".to_string(),
        mime_type: Some("text/plain; charset=utf-8".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use agentdash_domain::common::error::DomainError;
    use tokio::sync::Mutex;

    type InlineKey = (String, Uuid, String, String);

    #[derive(Default)]
    struct MemoryInlineFileRepo {
        files: Mutex<HashMap<InlineKey, InlineFile>>,
    }

    #[async_trait::async_trait]
    impl InlineFileRepository for MemoryInlineFileRepo {
        async fn get_file(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
            path: &str,
        ) -> Result<Option<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .await
                .get(&(
                    owner_kind.as_str().to_string(),
                    owner_id,
                    container_id.to_string(),
                    path.to_string(),
                ))
                .cloned())
        }

        async fn list_files(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<Vec<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .await
                .values()
                .filter(|file| {
                    file.owner_kind == owner_kind
                        && file.owner_id == owner_id
                        && file.container_id == container_id
                })
                .cloned()
                .collect())
        }

        async fn list_files_by_owner(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
        ) -> Result<Vec<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .await
                .values()
                .filter(|file| file.owner_kind == owner_kind && file.owner_id == owner_id)
                .cloned()
                .collect())
        }

        async fn upsert_file(&self, file: &InlineFile) -> Result<(), DomainError> {
            self.files.lock().await.insert(
                (
                    file.owner_kind.as_str().to_string(),
                    file.owner_id,
                    file.container_id.clone(),
                    file.path.clone(),
                ),
                file.clone(),
            );
            Ok(())
        }

        async fn upsert_files(&self, files: &[InlineFile]) -> Result<(), DomainError> {
            for file in files {
                self.upsert_file(file).await?;
            }
            Ok(())
        }

        async fn delete_file(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
            path: &str,
        ) -> Result<(), DomainError> {
            self.files.lock().await.remove(&(
                owner_kind.as_str().to_string(),
                owner_id,
                container_id.to_string(),
                path.to_string(),
            ));
            Ok(())
        }

        async fn delete_by_container(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<(), DomainError> {
            self.files.lock().await.retain(|(kind, id, cid, _), _| {
                kind != owner_kind.as_str() || *id != owner_id || cid != container_id
            });
            Ok(())
        }

        async fn delete_by_owner(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
        ) -> Result<(), DomainError> {
            self.files
                .lock()
                .await
                .retain(|(kind, id, _, _), _| kind != owner_kind.as_str() || *id != owner_id);
            Ok(())
        }

        async fn count_files(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<i64, DomainError> {
            Ok(self
                .list_files(owner_kind, owner_id, container_id)
                .await?
                .len() as i64)
        }
    }

    fn inline_mount(owner_kind: &str, owner_id: Uuid, container_id: &str) -> Mount {
        Mount {
            id: "brief".to_string(),
            provider: PROVIDER_INLINE_FS.to_string(),
            backend_id: String::new(),
            root_ref: format!("context://inline/{container_id}"),
            capabilities: vec![
                MountCapability::Read,
                MountCapability::Write,
                MountCapability::List,
                MountCapability::Search,
            ],
            default_write: false,
            display_name: "brief".to_string(),
            metadata: serde_json::json!({
                "container_id": container_id,
                "agentdash_context_owner_kind": owner_kind,
                "agentdash_context_owner_id": owner_id.to_string(),
            }),
        }
    }

    fn dispatcher(repo: Arc<MemoryInlineFileRepo>) -> VfsMutationDispatcher {
        let registry = Arc::new(MountProviderRegistry::new());
        VfsMutationDispatcher::new(
            Arc::new(RelayVfsService::new(registry.clone())),
            repo,
            registry,
        )
    }

    fn vfs_with_mount(mount: Mount) -> Vfs {
        Vfs {
            mounts: vec![mount],
            default_mount_id: Some("brief".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    #[test]
    fn inline_storage_key_supports_all_inline_runtime_owner_kinds() {
        for (owner_kind, expected) in [
            ("project_vfs_mount", InlineFileOwnerKind::ProjectVfsMount),
            ("project", InlineFileOwnerKind::Project),
            ("story", InlineFileOwnerKind::Story),
            ("project_agent", InlineFileOwnerKind::ProjectAgent),
        ] {
            let owner_id = Uuid::new_v4();
            let mount = inline_mount(owner_kind, owner_id, "files");
            let key = inline_storage_key_from_mount(&mount).expect("storage key");
            assert_eq!(key.owner_kind, expected);
            assert_eq!(key.owner_id, owner_id);
            assert_eq!(key.container_id, "files");
        }
    }

    #[tokio::test]
    async fn dispatcher_mutates_inline_files_through_one_storage_key() {
        let owner_id = Uuid::new_v4();
        let repo = Arc::new(MemoryInlineFileRepo::default());
        let dispatcher = dispatcher(repo.clone());
        let vfs = vfs_with_mount(inline_mount("project_vfs_mount", owner_id, "files"));

        dispatcher
            .create_text(
                &vfs,
                ResourceRef {
                    mount_id: "brief".to_string(),
                    path: "note.md".to_string(),
                },
                "v1",
                None,
            )
            .await
            .expect("create inline file");

        dispatcher
            .write_text(
                &vfs,
                ResourceRef {
                    mount_id: "brief".to_string(),
                    path: "note.md".to_string(),
                },
                "v2",
                None,
            )
            .await
            .expect("write inline file");

        dispatcher
            .rename_text(&vfs, "brief", "note.md", "docs/note.md", None)
            .await
            .expect("rename inline file");

        let moved = repo
            .get_file(
                InlineFileOwnerKind::ProjectVfsMount,
                owner_id,
                "files",
                "docs/note.md",
            )
            .await
            .expect("repo read")
            .expect("moved file");
        assert_eq!(moved.text_content(), Some("v2"));
        assert!(
            repo.get_file(
                InlineFileOwnerKind::ProjectVfsMount,
                owner_id,
                "files",
                "note.md",
            )
            .await
            .expect("repo read")
            .is_none()
        );

        dispatcher
            .delete_text(
                &vfs,
                ResourceRef {
                    mount_id: "brief".to_string(),
                    path: "docs/note.md".to_string(),
                },
                None,
            )
            .await
            .expect("delete inline file");

        assert!(
            repo.get_file(
                InlineFileOwnerKind::ProjectVfsMount,
                owner_id,
                "files",
                "docs/note.md",
            )
            .await
            .expect("repo read")
            .is_none()
        );
    }
}
