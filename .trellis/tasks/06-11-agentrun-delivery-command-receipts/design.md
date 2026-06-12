# 设计

## Receipt Scope

Project Agent start scope：

```text
project_agent_start:{project_id}:{project_agent_id}:{subject_ref}:{client_command_id}
```

AgentRun message scope：

```text
agent_run_message:{run_id}:{agent_id}:{client_command_id}
```

## Receipt Record

```text
id
scope_kind
scope_key
client_command_id
request_digest
status
runtime_session_id?
run_id?
agent_id?
frame_id?
turn_id?
error_message?
created_at
updated_at
accepted_at?
failed_at?
```

`session_runtime_commands` 保持 runtime context/frame transition 队列职责；本任务新增或扩展独立 delivery command receipt，避免把用户投递幂等和 frame transition 队列混在一起。

## State Semantics

- pending: command 已被服务端接收，accepted refs 尚未提交。
- accepted: connector accepted boundary 已通过，refs 可复用。
- terminal_failed: command 已结束为失败，重试返回同一失败。

Request digest 使用 canonical JSON：输入、executor_config、subject_ref、target refs。相同 command id 携带不同 digest 是客户端状态错误。

## Recovery

前端遇到 transport failure 后使用原 `client_command_id` 重试。服务端如果已 accepted，直接返回 accepted refs；如果仍 pending，返回 command state，前端刷新 workspace 并保持输入不重复提交。
