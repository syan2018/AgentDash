use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
};
use serde_json::json;

use crate::product::tool_service::{
    ApplicationWorkspaceModuleRuntimeToolService, compact_diagnostics, completed,
};

impl ApplicationWorkspaceModuleRuntimeToolService {
    pub(crate) async fn execute_list(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> ProductRuntimeToolOutcome {
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        completed(json!({
            "module_count": surface.modules.len(),
            "surface_readiness": if surface.diagnostics.is_empty() { "ready" } else { "degraded" },
            "surface_diagnostics": compact_diagnostics(surface.diagnostics),
            "modules": surface
                .modules
                .into_iter()
                .map(|module| module.summary)
                .collect::<Vec<_>>(),
        }))
    }
}
