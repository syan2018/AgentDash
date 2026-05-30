//! Permission System application layer.
//!
//! 包含 Policy 评估、Grant 编译（到 RuntimeCapabilityTransition）、
//! Grant 生命周期服务和 Scope Escalation 协调。

mod compiler;
mod escalation;
mod policy;
mod service;

pub use compiler::PermissionGrantCompiler;
pub use escalation::{EscalationResult, ScopeEscalationCoordinator};
pub use policy::PermissionPolicyService;
pub use service::{GrantRequest, GrantRequestResult, PermissionGrantService};
