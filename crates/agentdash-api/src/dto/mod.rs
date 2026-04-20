/// API Response DTO 层
///
/// 隔离 Domain 实体与 HTTP 响应契约。Route handler 通过 DTO 输出，
/// 使 API 序列化结构与领域模型解耦，便于独立演进。
mod canvas;
mod identity_directory;
mod mcp_preset;
mod project;
mod story;
mod task;
mod workflow;
mod workspace;

pub use canvas::*;
pub use identity_directory::*;
pub use mcp_preset::*;
pub use project::*;
pub use story::*;
pub use task::*;
pub use workflow::*;
pub use workspace::*;
