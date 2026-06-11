# 设计

## Workspace View

`AgentRunWorkspaceView` 以 AgentRun 为主体：

```text
run_ref
agent_ref
project_id
shell
delivery_runtime_ref?
delivery_trace_meta?
control_plane
frame_runtime?
subject_associations
actions
pending_messages
```

`frame_runtime.execution_profile` 继续承载 `AgentConfig` JSON。前端通过 typed mapper 转为 executor selector source。

`shell` 是 AgentRun Workspace 的 public display/status projection，不读取 `SessionMeta` 作为权威工作台事实。RuntimeSession 相关信息只进入 `delivery_trace_meta`。

## Identity

Public workspace identity 使用完整 `AgentRunRefDto { run_id, agent_id }`。`run_id` 定位 LifecycleRun 的账本和拓扑，`agent_id` 定位该 run 内实际可交互的 LifecycleAgent/AgentRun。workspace API 通过二者共同确定 current frame、execution profile、delivery runtime ref、actions 和 pending messages。

短入口 `/agent-runs/{run_id}` 如需存在，只承担 exact-one-agent resolver 职责；解析后返回或跳转到完整 AgentRunRef。

## Endpoint Semantics

`GET /agent-runs/{run_id}/{agent_id}/workspace`：

- 校验 run 存在、agent 属于 run、当前用户有项目权限。
- 读取 `LifecycleAgent.current_frame_id` 或有效 frame。
- 读取 agent delivery runtime ref。
- 读取 pending messages、execution state 和 supported actions。
- 返回 workspace view。
- `shell.delivery_status` 来自 AgentRun delivery projection、active turn inspection 或 command receipt read model；`SessionMeta.last_delivery_status` 只可作为 trace fallback。

`POST /agent-runs/{run_id}/{agent_id}/messages`：

- 校验 `client_command_id`。
- 解析 delivery runtime ref。
- 解析 `executor_config` 为 `AgentConfig`。
- 进入 AgentRunMessageService。

`POST /projects/{project_id}/agents/{project_agent_id}/agent-runs`：

- materialize Project Agent as LifecycleRun/LifecycleAgent/RuntimeSession。
- 使用同一 command contract 提交首条消息。
- 返回 accepted refs 和 workspace view 或可立即 fetch workspace 的 refs。

## Error Mapping

AgentRun delivery port 应保留 connector error category：

- InvalidConfig -> WorkflowApplicationError::BadRequest -> HTTP 400
- ConnectionFailed -> ServiceUnavailable 或明确 delivery unavailable
- Runtime/Io -> Internal

这部分可以通过新增 `From<ConnectorError> for WorkflowApplicationError` 或 delivery port 显式 match 完成。

## Contract Generation

本任务完成后运行 contract generation/check，让 frontend child 不再手写临时 DTO。
