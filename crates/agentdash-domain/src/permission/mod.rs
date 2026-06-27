//! Agent Permission System — 权限申请 / 授予 / 生效 / 撤销的领域模型。
//!
//! `PermissionGrant` 是核心聚合根，表达 Agent 对特定 capability scope 的权限事实。
//! 状态机由 domain 层强制校验，policy 评估和 capability runtime 集成在 application 层。

mod entity;
mod repository;
mod value_objects;

pub use entity::PermissionGrant;
pub use repository::{PermissionGrantRepository, PermissionGrantStatusFilter};
pub use value_objects::{
    GrantScope, GrantStatus, PolicyDecision, PolicyOutcome, ScopeEscalationIntent,
};
