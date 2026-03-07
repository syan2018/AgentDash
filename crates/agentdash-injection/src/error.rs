use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum InjectionError {
    #[error("缺少工作区，无法解析来源: {0}")]
    MissingWorkspace(String),
    #[error("来源路径不存在: {0}")]
    PathNotFound(PathBuf),
    #[error("来源文件过大: {path} ({size} bytes)")]
    SourceTooLarge { path: PathBuf, size: u64 },
    #[error("不支持的文件类型: {0}")]
    UnsupportedFileType(PathBuf),
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("YAML 解析失败: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("IO 失败: {0}")]
    Io(#[from] std::io::Error),
}
