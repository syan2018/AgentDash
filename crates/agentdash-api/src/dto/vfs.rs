use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct VfssQuery {
    pub workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListEntriesQuery {
    #[serde(default)]
    pub query: Option<String>,
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub recursive: Option<bool>,
}
