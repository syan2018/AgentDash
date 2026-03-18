pub mod address_space;
pub mod composer;
pub mod error;
pub mod resolver;

pub use address_space::{
    AddressSpaceContext, AddressSpaceDescriptor, AddressSpaceProvider, AddressSpaceRegistry,
    builtin_address_space_registry,
};
pub use composer::{ContextComposer, ContextFragment, MergeStrategy};
pub use error::InjectionError;
pub use resolver::{
    ResolveSourcesOutput, ResolveSourcesRequest, SourceResolverRegistry, resolve_declared_sources,
    resolve_declared_sources_with_registry,
};
