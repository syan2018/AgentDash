//! Lifecycle dispatch, orchestration activation, reducer, scheduler, and materialization.

pub mod lifecycle;
pub mod platform_config;
pub mod repository_set;

pub use lifecycle::*;
pub use platform_config::{PlatformConfig, SharedPlatformConfig};
pub use repository_set::RepositorySet;
