use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct DiscoveredOptionsQuery {
    pub executor: String,
    pub working_dir: Option<String>,
}
