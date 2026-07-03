use agentdash_spi::platform::tool_capability::{
    CAP_COLLABORATION, CAP_FILE_READ, CAP_FILE_WRITE, CAP_RELAY_MANAGEMENT, CAP_SHELL_EXECUTE,
    CAP_STORY_MANAGEMENT, CAP_TASK, CAP_WORKFLOW, CAP_WORKFLOW_MANAGEMENT, CAP_WORKSPACE_MODULE,
    ToolCapability,
};

pub(crate) fn capability_description(key: &str) -> &'static str {
    match key {
        CAP_FILE_READ => "文件读取（mounts_list / fs_read / fs_glob / fs_grep）",
        CAP_FILE_WRITE => "文件写入（fs_apply_patch）",
        CAP_SHELL_EXECUTE => "Shell 命令执行",
        CAP_WORKSPACE_MODULE => "Workspace Module 创建、调用与展示（含 Canvas）",
        CAP_WORKFLOW => "Lifecycle 推进与产物上报",
        CAP_COLLABORATION => "结构化协作请求、回应与活动回传",
        CAP_TASK => "Task 读取与维护（task_read / task_write）",
        CAP_STORY_MANAGEMENT => "Story 上下文管理、Task 创建与批量拆解、状态推进",
        CAP_RELAY_MANAGEMENT => "项目管理、Story 创建与状态变更",
        CAP_WORKFLOW_MANAGEMENT => "Workflow / Lifecycle 定义的查看、创建与编辑",
        _ => {
            if ToolCapability::new(key).is_custom_mcp() {
                "外部自定义 MCP 工具集"
            } else {
                ""
            }
        }
    }
}
