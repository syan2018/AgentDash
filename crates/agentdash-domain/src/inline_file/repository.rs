use uuid::Uuid;

use super::entity::{InlineFile, InlineFileOwnerKind};
use crate::common::error::DomainError;

/// 内联文件仓储接口
///
/// 提供 inline_fs 文件的独立 CRUD，替代原来嵌套在
/// Project/Story/LifecycleRun 实体中的 read-modify-write 模式。
#[async_trait::async_trait]
pub trait InlineFileRepository: Send + Sync {
    /// 读取单个文件
    async fn get_file(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<Option<InlineFile>, DomainError>;

    /// 列出 container 下所有文件
    async fn list_files(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<Vec<InlineFile>, DomainError>;

    /// 列出 owner 下所有 container 的所有文件
    async fn list_files_by_owner(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
    ) -> Result<Vec<InlineFile>, DomainError>;

    /// 写入或更新文件（UPSERT on unique key）
    async fn upsert_file(&self, file: &InlineFile) -> Result<(), DomainError>;

    /// 批量写入文件（用于初始导入）
    async fn upsert_files(&self, files: &[InlineFile]) -> Result<(), DomainError>;

    /// 删除单个文件
    async fn delete_file(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
        path: &str,
    ) -> Result<(), DomainError>;

    /// 删除 container 下所有文件
    async fn delete_by_container(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<(), DomainError>;

    /// 删除 owner 下所有文件（owner 被删除时调用）
    async fn delete_by_owner(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
    ) -> Result<(), DomainError>;

    /// 统计 container 下文件数
    async fn count_files(
        &self,
        owner_kind: InlineFileOwnerKind,
        owner_id: Uuid,
        container_id: &str,
    ) -> Result<i64, DomainError>;
}
