//! Task ↔ Session 桥接。
//!
//! 职责：封装 Task execution child session 的创建流程。
//! Session 归属由 LifecycleRunLink 管理；Permission System 将接管权限事实。

use agentdash_domain::task::Task;

use crate::session::SessionMeta;
use crate::task::execution::TaskExecutionError;

use super::errors::map_internal_error;

pub async fn create_task_session(
    session_core: &crate::session::SessionCoreService,
    task: &Task,
) -> Result<SessionMeta, TaskExecutionError> {
    let title = format!("Task: {}", task.title.trim());
    session_core
        .create_session(title.trim())
        .await
        .map_err(map_internal_error)
}
