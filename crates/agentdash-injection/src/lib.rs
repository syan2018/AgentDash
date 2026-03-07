pub mod composer;
pub mod error;
pub mod resolver;

pub use composer::{ContextComposer, ContextFragment, MergeStrategy};
pub use error::InjectionError;
pub use resolver::{ResolveSourcesOutput, ResolveSourcesRequest, resolve_declared_sources};
