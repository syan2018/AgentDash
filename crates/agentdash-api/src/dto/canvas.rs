use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ListProjectCanvasesPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PromoteCanvasToExtensionRequest {
    pub extension_key: Option<String>,
    pub display_name: Option<String>,
    pub package_version: Option<String>,
    pub asset_version: Option<String>,
    #[serde(default = "default_promote_overwrite")]
    pub overwrite: bool,
}

fn default_promote_overwrite() -> bool {
    true
}
