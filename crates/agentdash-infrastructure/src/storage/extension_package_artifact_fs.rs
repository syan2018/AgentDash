use std::path::{Component, Path, PathBuf};

use agentdash_platform_spi::extension_package::{
    ExtensionPackageArtifactStorage, ExtensionPackageArtifactStorageError,
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct FilesystemExtensionPackageArtifactStorage {
    root: PathBuf,
}

impl FilesystemExtensionPackageArtifactStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn storage_path(
        &self,
        storage_ref: &str,
    ) -> Result<PathBuf, ExtensionPackageArtifactStorageError> {
        let mut path = self.root.clone();
        let mut has_component = false;
        for component in Path::new(storage_ref).components() {
            match component {
                Component::Normal(part) => {
                    has_component = true;
                    path.push(part);
                }
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(ExtensionPackageArtifactStorageError::InvalidStorageRef(
                        storage_ref.to_string(),
                    ));
                }
            }
        }
        if !has_component {
            return Err(ExtensionPackageArtifactStorageError::InvalidStorageRef(
                storage_ref.to_string(),
            ));
        }
        Ok(path)
    }
}

impl Default for FilesystemExtensionPackageArtifactStorage {
    fn default() -> Self {
        Self::new(
            std::env::var_os("AGENTDASH_EXTENSION_ARTIFACT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::current_dir()
                        .unwrap_or_else(|_| std::env::temp_dir())
                        .join(".agentdash")
                        .join("extension-artifacts")
                }),
        )
    }
}

#[async_trait::async_trait]
impl ExtensionPackageArtifactStorage for FilesystemExtensionPackageArtifactStorage {
    async fn write_archive_object(
        &self,
        storage_ref: &str,
        bytes: &[u8],
    ) -> Result<(), ExtensionPackageArtifactStorageError> {
        let path = self.storage_path(storage_ref)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(ExtensionPackageArtifactStorageError::CreateDir)?;
            let temp_path = parent.join(format!(".{}.tmp", Uuid::new_v4()));
            tokio::fs::write(&temp_path, bytes)
                .await
                .map_err(ExtensionPackageArtifactStorageError::Write)?;
            tokio::fs::rename(&temp_path, &path)
                .await
                .map_err(ExtensionPackageArtifactStorageError::Write)?;
            return Ok(());
        }
        tokio::fs::write(&path, bytes)
            .await
            .map_err(ExtensionPackageArtifactStorageError::Write)
    }

    async fn read_archive_object(
        &self,
        storage_ref: &str,
    ) -> Result<Vec<u8>, ExtensionPackageArtifactStorageError> {
        let path = self.storage_path(storage_ref)?;
        tokio::fs::read(&path)
            .await
            .map_err(ExtensionPackageArtifactStorageError::Read)
    }
}
