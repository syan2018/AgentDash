use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agentdash_local::DesktopRunnerHost;

use crate::desktop_api::{DesktopApiManager, default_desktop_api_snapshot};
use crate::desktop_update::DesktopUpdateGate;

#[derive(Clone)]
pub(crate) struct DesktopState {
    pub(crate) runtime: DesktopRunnerHost,
    pub(crate) api: DesktopApiManager,
    pub(crate) updates: DesktopUpdateGate,
    lifecycle: Arc<DesktopLifecycleState>,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            runtime: DesktopRunnerHost::new(),
            api: DesktopApiManager::from_snapshot(default_desktop_api_snapshot()),
            updates: DesktopUpdateGate::default(),
            lifecycle: Arc::new(DesktopLifecycleState::default()),
        }
    }
}

#[derive(Default)]
struct DesktopLifecycleState {
    explicit_quit: AtomicBool,
}

impl DesktopState {
    pub(crate) fn request_explicit_quit(&self) {
        self.lifecycle.explicit_quit.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_explicit_quit_requested(&self) -> bool {
        self.lifecycle.explicit_quit.load(Ordering::SeqCst)
    }
}
