use std::sync::Arc;

use agentdash_application_ports::vfs_surface_runtime::{
    ResolvedMountEditCapabilities, VfsSurfaceRuntimeProjection,
};
use agentdash_application_vfs::MountProviderRegistry;
use async_trait::async_trait;

use crate::relay::registry::BackendRegistry;

pub(crate) struct ApiVfsSurfaceRuntimeProjection {
    backend_registry: Arc<BackendRegistry>,
    mount_provider_registry: Arc<MountProviderRegistry>,
}

impl ApiVfsSurfaceRuntimeProjection {
    pub(crate) fn new(
        backend_registry: Arc<BackendRegistry>,
        mount_provider_registry: Arc<MountProviderRegistry>,
    ) -> Self {
        Self {
            backend_registry,
            mount_provider_registry,
        }
    }
}

#[async_trait]
impl VfsSurfaceRuntimeProjection for ApiVfsSurfaceRuntimeProjection {
    async fn is_backend_online(&self, backend_id: &str) -> bool {
        self.backend_registry.is_online(backend_id).await
    }

    fn edit_capabilities(&self, mount: &agentdash_platform_spi::Mount) -> ResolvedMountEditCapabilities {
        self.mount_provider_registry
            .get(&mount.provider)
            .map(|provider| provider.edit_capabilities(mount))
            .map(|capabilities| ResolvedMountEditCapabilities {
                create: capabilities.create,
                delete: capabilities.delete,
                rename: capabilities.rename,
            })
            .unwrap_or_default()
    }
}
