use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    pub pattern: Option<String>,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub rel_path: String,
    pub size: u64,
    pub is_text: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilesResponse {
    pub files: Vec<FileEntry>,
    pub root: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileRequest {
    pub rel_path: String,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileResponse {
    pub rel_path: String,
    pub uri: String,
    pub mime_type: String,
    pub content: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReadFilesRequest {
    pub paths: Vec<String>,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReadFilesResponse {
    pub files: Vec<ReadFileResult>,
    pub total_size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileResult {
    pub rel_path: String,
    pub uri: String,
    pub mime_type: String,
    pub content: Option<String>,
    pub size: u64,
    pub error: Option<String>,
}
