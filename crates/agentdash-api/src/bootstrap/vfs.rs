use std::sync::Arc;

use agentdash_application::context::{VfsDiscoveryRegistry, builtin_vfs_registry};
use agentdash_application::platform_config::SharedPlatformConfig;
use agentdash_application::repository_set::RepositorySet;
use agentdash_application::session::SessionPersistence;
use agentdash_application::vfs::tools::provider::{
    RelayRuntimeToolProvider, SharedSessionToolServicesHandle,
};
use agentdash_application::vfs::{MountProviderRegistry, MountProviderRegistryBuilder};
use agentdash_application::vfs::{VfsMutationDispatcher, VfsService};
use agentdash_spi::VfsDiscoveryProvider;
use agentdash_spi::platform::mount::MountProvider;

use crate::mount_providers::RelayFsMountProvider;
use crate::relay::registry::BackendRegistry;

pub(crate) struct VfsBootstrapOutput {
    pub mount_provider_registry: Arc<MountProviderRegistry>,
    pub vfs_service: Arc<VfsService>,
    pub vfs_mutation_dispatcher: Arc<VfsMutationDispatcher>,
    pub session_services_handle: SharedSessionToolServicesHandle,
    pub runtime_tool_provider: Arc<dyn agentdash_spi::connector::RuntimeToolProvider>,
    pub mcp_relay_provider: Arc<dyn agentdash_spi::McpRelayProvider>,
}

pub(crate) fn build_vfs_kernel(
    repos: RepositorySet,
    session_persistence: Arc<dyn SessionPersistence>,
    backend_registry: Arc<BackendRegistry>,
    shell_output_registry: Arc<agentdash_relay::ShellOutputRegistry>,
    platform_config: SharedPlatformConfig,
    function_runner: Arc<dyn agentdash_spi::FunctionRunner>,
    integration_mount_providers: Vec<Arc<dyn MountProvider>>,
) -> VfsBootstrapOutput {
    let mut mount_registry_builder = MountProviderRegistryBuilder::new()
        .with_builtins(
            repos.lifecycle_run_repo.clone(),
            repos.canvas_repo.clone(),
            repos.inline_file_repo.clone(),
            repos.routine_execution_repo.clone(),
            repos.skill_asset_repo.clone(),
            session_persistence,
        )
        .register(Arc::new(RelayFsMountProvider::new(
            backend_registry.clone(),
        )));

    for provider in integration_mount_providers {
        tracing::info!(
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
    let session_services_handle = SharedSessionToolServicesHandle::default();

    let inline_persister: Arc<
        dyn agentdash_application::vfs::inline_persistence::InlineContentPersister,
    > = Arc::new(
        agentdash_application::vfs::inline_persistence::DbInlineContentPersister::new(
            repos.inline_file_repo.clone(),
        ),
    );

    let materialization_transport = Arc::new(
        crate::vfs_materialization::RelayVfsMaterializationTransport::new(backend_registry.clone()),
    );
    let materialization_service =
        Arc::new(agentdash_application::vfs::VfsMaterializationService::new(
            vfs_service.clone(),
            materialization_transport,
        ));

    let runtime_tool_provider: Arc<dyn agentdash_spi::connector::RuntimeToolProvider> = Arc::new(
        RelayRuntimeToolProvider::new(
            vfs_service.clone(),
            repos,
            session_services_handle.clone(),
            Some(inline_persister),
            platform_config,
            function_runner,
        )
        .with_materialization_service(materialization_service.clone())
        .with_shell_output_registry(shell_output_registry),
    );
    let mcp_relay_provider: Arc<dyn agentdash_spi::McpRelayProvider> = Arc::new(
        crate::vfs_materialization::MaterializingMcpRelayProvider::new(
            backend_registry,
            materialization_service,
        ),
    );

    VfsBootstrapOutput {
        mount_provider_registry,
        vfs_service,
        vfs_mutation_dispatcher,
        session_services_handle,
        runtime_tool_provider,
        mcp_relay_provider,
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
