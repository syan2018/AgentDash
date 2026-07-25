use agentdash_diagnostics::{Subsystem, diag};
use std::sync::Arc;

use agentdash_application::context::{VfsDiscoveryRegistry, builtin_vfs_registry};
use agentdash_application::repository_set::RepositorySet;
use agentdash_application_lifecycle::lifecycle::{
    DeferredLifecycleHistoryQuery, LifecycleMountProvider,
};
use agentdash_application_vfs::{
    InlineFsMountProvider, MountProviderRegistry, MountProviderRegistryBuilder,
    RoutineMountProvider, SkillAssetFsMountProvider, VfsMaterializationService,
    VfsMutationDispatcher, VfsService,
};
use agentdash_platform_spi::VfsDiscoveryProvider;
use agentdash_platform_spi::platform::mount::MountProvider;

use crate::mount_providers::RelayFsMountProvider;
use crate::relay::registry::BackendRegistry;

pub(crate) struct VfsBootstrapOutput {
    pub mount_provider_registry: Arc<MountProviderRegistry>,
    pub vfs_service: Arc<VfsService>,
    pub vfs_mutation_dispatcher: Arc<VfsMutationDispatcher>,
    pub vfs_materialization_service: Arc<VfsMaterializationService>,
    pub lifecycle_history_query: DeferredLifecycleHistoryQuery,
}

pub(crate) fn build_vfs_kernel(
    repos: RepositorySet,
    backend_registry: Arc<BackendRegistry>,
    integration_mount_providers: Vec<Arc<dyn MountProvider>>,
) -> VfsBootstrapOutput {
    let lifecycle_history_query = DeferredLifecycleHistoryQuery::default();
    let mut mount_registry_builder = MountProviderRegistryBuilder::new()
        .register(Arc::new(InlineFsMountProvider::new(
            repos.inline_file_repo.clone(),
        )))
        .register(Arc::new(RoutineMountProvider::new(
            repos.routine_execution_repo.clone(),
            repos.inline_file_repo.clone(),
        )))
        .register(Arc::new(SkillAssetFsMountProvider::new(
            repos.skill_asset_repo.clone(),
        )))
        .register(Arc::new(LifecycleMountProvider::new(
            repos.lifecycle_run_repo.clone(),
            repos.lifecycle_agent_repo.clone(),
            repos.inline_file_repo.clone(),
            repos.skill_asset_repo.clone(),
            Arc::new(lifecycle_history_query.clone()),
        )))
        .register(Arc::new(RelayFsMountProvider::new(
            backend_registry.clone(),
        )));

    for provider in integration_mount_providers {
        diag!(
            Info,
            Subsystem::Vfs,
            "注册 Host Integration MountProvider: {}",
            provider.provider_id()
        );
        mount_registry_builder = mount_registry_builder.register(provider);
    }

    let mount_provider_registry = Arc::new(mount_registry_builder.build());
    let vfs_service = Arc::new(VfsService::new(mount_provider_registry.clone()));
    let vfs_mutation_dispatcher = Arc::new(VfsMutationDispatcher::new(
        vfs_service.clone(),
        repos.inline_file_repo.clone(),
        mount_provider_registry.clone(),
    ));

    let materialization_transport = Arc::new(
        crate::vfs_materialization::RelayVfsMaterializationTransport::new(backend_registry.clone()),
    );
    let materialization_service =
        Arc::new(agentdash_application_vfs::VfsMaterializationService::new(
            vfs_service.clone(),
            materialization_transport,
        ));
    VfsBootstrapOutput {
        mount_provider_registry,
        vfs_service,
        vfs_mutation_dispatcher,
        vfs_materialization_service: materialization_service,
        lifecycle_history_query,
    }
}

pub(crate) fn build_vfs_discovery_registry(
    integration_vfs_providers: Vec<Box<dyn VfsDiscoveryProvider>>,
) -> VfsDiscoveryRegistry {
    let mut vfs_registry = builtin_vfs_registry();
    for provider in integration_vfs_providers {
        vfs_registry.register(provider);
    }
    vfs_registry
}
