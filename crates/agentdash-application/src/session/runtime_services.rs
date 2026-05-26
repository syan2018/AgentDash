use std::sync::Arc;

use agentdash_spi::AgentConnector;
use agentdash_spi::hooks::ExecutionHookProvider;

use super::branching::SessionBranchingService;
use super::core::SessionCoreService;
use super::eventing::SessionEventingService;
use super::hub::SessionRuntimeInner;
use super::launch::SessionLaunchService;
use super::persistence::SessionPersistence;
use super::runtime_control::SessionRuntimeService;

#[derive(Clone)]
pub struct SessionRuntimeServices {
    pub core: SessionCoreService,
    pub branching: SessionBranchingService,
    pub eventing: SessionEventingService,
    pub runtime: SessionRuntimeService,
    pub launch: SessionLaunchService,
}

impl SessionRuntimeServices {
    pub fn new_with_hooks_and_persistence(
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
        persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        let inner = SessionRuntimeInner::new_with_hooks_and_persistence(
            connector,
            hook_provider,
            persistence,
        );
        Self::from_inner(&inner)
    }

    pub(crate) fn from_inner(inner: &SessionRuntimeInner) -> Self {
        Self {
            core: inner.core_service(),
            branching: inner.branching_service(),
            eventing: inner.eventing_service(),
            runtime: inner.runtime_service(),
            launch: inner.launch_service(),
        }
    }
}
