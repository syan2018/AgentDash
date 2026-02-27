use serde::{Deserialize, Serialize};

/// Workspace 类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceType {
    /// Git worktree（基于现有仓库创建分支工作目录）
    GitWorktree,
    /// 静态目录（指向已有代码库，不做 clone）
    Static,
    /// 临时目录（任务完成后可清理）
    Ephemeral,
}

/// Workspace 状态
/// 生命周期: Pending → Preparing → Ready → Active → Archived
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    /// 待创建
    Pending,
    /// 准备中（clone/setup）
    Preparing,
    /// 就绪，可分配 Task
    Ready,
    /// 有 Task 正在运行
    Active,
    /// 已归档
    Archived,
    /// 错误状态
    Error,
}

/// Git 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    /// 源仓库路径
    pub source_repo: String,
    /// 分支名
    pub branch: String,
    /// 固定 commit（可选）
    pub commit_hash: Option<String>,
}
