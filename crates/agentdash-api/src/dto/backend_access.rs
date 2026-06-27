use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BrowseAccessDirectoryRequest {
    pub path: Option<String>,
}
