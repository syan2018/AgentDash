use super::terminal_boundary::{
    RuntimeTerminalBoundaryDeps, RuntimeTerminalBoundaryDispatcher, RuntimeTerminalBoundaryEvidence,
};

#[derive(Clone)]
pub struct RuntimeTerminalBoundaryService {
    deps: RuntimeTerminalBoundaryDeps,
}

impl RuntimeTerminalBoundaryService {
    pub(crate) fn new(deps: RuntimeTerminalBoundaryDeps) -> Self {
        Self { deps }
    }

    pub(crate) async fn observe_terminal_boundary(&self, input: RuntimeTerminalBoundaryEvidence) {
        RuntimeTerminalBoundaryDispatcher::new(self.deps.clone())
            .observe_terminal_boundary(input)
            .await;
    }
}
