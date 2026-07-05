//! Lifecycle dispatch, orchestration activation, reducer, scheduler, and materialization.

pub mod lifecycle;
pub mod platform_config;
pub mod workflow_materialization;

pub use lifecycle::*;
pub use platform_config::{PlatformConfig, SharedPlatformConfig};
pub use workflow_materialization::{
    LifecycleWorkflowAgentNodeMaterializationAdapter, LifecycleWorkflowAgentNodeMaterializationDeps,
};
