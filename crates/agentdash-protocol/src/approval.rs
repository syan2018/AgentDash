use codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 审批请求的薄包裹。
///
/// 保留 `request_id` 用于回传决策结果，payload 直接复用 Codex 类型。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ApprovalRequest {
    CommandExecution {
        request_id: codex::RequestId,
        params: codex::CommandExecutionRequestApprovalParams,
    },
    FileChange {
        request_id: codex::RequestId,
        params: codex::FileChangeRequestApprovalParams,
    },
    ToolUserInput {
        request_id: codex::RequestId,
        params: codex::ToolRequestUserInputParams,
    },
    PermissionsApproval {
        request_id: codex::RequestId,
        params: codex::PermissionsRequestApprovalParams,
    },
}
