#[derive(Debug, thiserror::Error)]
pub enum ExtensionPackageArtifactStorageError {
    #[error("artifact storage_ref 非法: {0}")]
    InvalidStorageRef(String),
    #[error("创建 artifact 存储目录失败: {0}")]
    CreateDir(#[source] std::io::Error),
    #[error("写入 artifact 存储失败: {0}")]
    Write(#[source] std::io::Error),
    #[error("读取 artifact 存储失败: {0}")]
    Read(#[source] std::io::Error),
}

#[async_trait::async_trait]
pub trait ExtensionPackageArtifactStorage: Send + Sync {
    async fn write_archive_object(
        &self,
        storage_ref: &str,
        bytes: &[u8],
    ) -> Result<(), ExtensionPackageArtifactStorageError>;

    async fn read_archive_object(
        &self,
        storage_ref: &str,
    ) -> Result<Vec<u8>, ExtensionPackageArtifactStorageError>;
}
