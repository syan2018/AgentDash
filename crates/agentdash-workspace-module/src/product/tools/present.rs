use agentdash_application_ports::product_runtime_tool::{
    ProductRuntimeToolOutcome, ProductRuntimeToolRequest,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::product::tool_service::{
    ApplicationWorkspaceModuleRuntimeToolService, completed, rejected,
};
use crate::product::{WorkspaceModulePresentationRequest, build_workspace_module_presentation};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentArguments {
    module_id: String,
    #[serde(default = "default_view_key")]
    view_key: String,
    #[serde(default)]
    payload: Option<Value>,
}

fn default_view_key() -> String {
    "preview".to_owned()
}

impl ApplicationWorkspaceModuleRuntimeToolService {
    pub(crate) async fn execute_present(
        &self,
        request: ProductRuntimeToolRequest,
    ) -> ProductRuntimeToolOutcome {
        let arguments: PresentArguments = match serde_json::from_value(request.arguments.clone()) {
            Ok(arguments) => arguments,
            Err(error) => {
                return rejected(
                    "workspace_module_invalid_arguments",
                    format!("invalid workspace_module_present arguments: {error}"),
                );
            }
        };
        let surface = match self.resolve_surface(&request).await {
            Ok(surface) => surface,
            Err(outcome) => return outcome,
        };
        let Some(module) = surface
            .modules
            .iter()
            .find(|module| module.summary.module_id == arguments.module_id)
        else {
            return rejected(
                "workspace_module_not_found",
                format!("workspace module is not visible: {}", arguments.module_id),
            );
        };
        let preparation = match self
            .deps
            .providers
            .prepare_presentation(WorkspaceModulePresentationRequest {
                context: &surface.provider_context,
                module,
                view_key: arguments.view_key.trim(),
                payload: arguments.payload.clone(),
            })
            .await
        {
            Ok(preparation) => preparation,
            Err(outcome) => return outcome,
        };
        let mut presentation = match build_workspace_module_presentation(
            module,
            arguments.view_key.trim(),
            arguments.payload,
            preparation.diagnostics,
        ) {
            Ok(presentation) => presentation,
            Err(error) => {
                return rejected("workspace_module_presentation_invalid", error.to_string());
            }
        };
        if let Some(presentation_uri) = preparation.presentation_uri {
            presentation.presentation_uri = presentation_uri;
        }
        completed(json!({
            "content": [{
                "type": "text",
                "text": format!(
                    "Workspace Module `{}` presentation requested",
                    presentation.title
                )
            }],
            "is_error": false,
            "details": {
                "workspace_module_presentation": presentation
            }
        }))
    }
}
