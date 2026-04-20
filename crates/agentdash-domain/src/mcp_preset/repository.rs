use uuid::Uuid;

use super::entity::McpPreset;
use crate::common::error::DomainError;

/// MCP Preset 仓储端口——单聚合持久化接口。
///
/// 约定：
/// - `list_by_project` 按 `project_id` 列出所有 Preset（含 builtin 和 user）
/// - `get` 按主键精确获取
/// - `get_by_project_and_name` 提供 name 唯一性校验入口
/// - `upsert_builtin` 用于 builtin seed 幂等装载：
///   - 如果 project 内已存在同 builtin_key 的 Preset → 以其 id 更新
///   - 否则按给定 Preset 整体插入
#[async_trait::async_trait]
pub trait McpPresetRepository: Send + Sync {
    async fn create(&self, preset: &McpPreset) -> Result<(), DomainError>;
    async fn get(&self, id: Uuid) -> Result<Option<McpPreset>, DomainError>;
    async fn get_by_project_and_name(
        &self,
        project_id: Uuid,
        name: &str,
    ) -> Result<Option<McpPreset>, DomainError>;
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<McpPreset>, DomainError>;
    async fn update(&self, preset: &McpPreset) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
    /// builtin seed 幂等装载——存在则更新 server_decl/name/description，不存在则插入。
    async fn upsert_builtin(&self, preset: &McpPreset) -> Result<McpPreset, DomainError>;
}
