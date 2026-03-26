use crate::runtime::RuntimeFileEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    pub mount_id: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    pub path: String,
    pub pattern: Option<String>,
    pub recursive: bool,
}

#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub mount_id: String,
    pub cwd: String,
    pub command: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ReadResult {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ListResult {
    pub entries: Vec<RuntimeFileEntry>,
}

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}
