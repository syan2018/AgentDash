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
mod project;
mod project_agent;
mod session;
mod shared_library;
mod skill_asset;
mod story;
mod task_execution;
mod terminal;
mod vfs;
mod workflow;
mod workspace;

pub use agentdash_contracts::project::{
    ProjectAccessSummaryResponse, ProjectDetailResponse, ProjectResponse,
    ProjectSubjectGrantResponse,
};
pub use agentdash_contracts::story::StoryResponse;
pub use agentdash_contracts::task::TaskResponse;
pub use agentdash_contracts::workspace::{WorkspaceBindingResponse, WorkspaceResponse};
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
pub use project::*;
pub use project_agent::*;
pub use session::*;
pub use shared_library::*;
pub use skill_asset::*;
pub use story::*;
pub use task_execution::*;
pub use terminal::*;
pub use vfs::*;
pub use workflow::*;
pub use workspace::*;
