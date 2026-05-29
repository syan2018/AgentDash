/// API Response DTO 层
///
/// 隔离 Domain 实体与 HTTP 响应契约。Route handler 通过 DTO 输出，
/// 使 API 序列化结构与领域模型解耦，便于独立演进。
mod auth;
mod backend;
mod backend_access;
mod canvas;
mod discovered_options;
mod discovery;
mod extension_management;
mod extension_runtime;
mod file_picker;
mod health;
mod identity_directory;
mod llm_provider;
mod shared_library;
mod skill_asset;
mod task_execution;
mod terminal;
mod vfs;
mod workflow;

pub use agentdash_contracts::core::{
    ProjectAccessSummaryResponse, ProjectDetailResponse, ProjectResponse,
    ProjectSubjectGrantResponse, StoryResponse, TaskResponse, WorkspaceBindingResponse,
    WorkspaceResponse,
};
pub use auth::*;
pub use backend::*;
pub use backend_access::*;
pub use canvas::*;
pub use discovered_options::*;
pub use discovery::*;
pub use extension_management::*;
pub use extension_runtime::*;
pub use file_picker::*;
pub use health::*;
pub use identity_directory::*;
pub use llm_provider::*;
pub use shared_library::*;
pub use skill_asset::*;
pub use task_execution::*;
pub use terminal::*;
pub use vfs::*;
pub use workflow::*;
