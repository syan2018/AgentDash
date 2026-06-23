# PiAgent 大输出 lifecycle 链路收口

## Goal

把 PR #66 的 PiAgent 大输出有界化链路补成完整可合并状态：模型上下文、Backbone、SessionEvent、NDJSON、前端继续只消费 bounded fact；同时 `lifecycle://session/tool-results/.../result.txt` 指向真实共享会话缓存，能够在当前会话缓存可用时通过 lifecycle VFS + `fs_read` 受控读取原始大工具结果。

## Background

当前 PR 已经完成大输出裁切、append guard、projection/resume 不扩写、前端 bounded 展示等主要防爆路径，但 review 发现工具结果 body ref 尚未闭环：

- `AgentToolResult` 裁切 helper 会触发 cache write callback，但默认 callback 只记录 debug 日志，没有写入 `SessionToolResultCache`。
- lifecycle provider 默认创建自己的空 `SessionToolResultCache`，不共享 agent loop 写入源。
- `AgentToolResult.details.lifecycle_path` 以原始 `tool_call_id` 派生；Backbone ThreadItem / lifecycle projection 当前使用 `turn_id:entry_index:tool_call_id` 合成 id，导致事件中的 path 与 VFS 可读路径不一致。

## Requirements

- 工具结果的 lifecycle item id 必须成为跨 AgentLoop、stream mapper、SessionEvent、lifecycle VFS 的单一稳定坐标。
- PiAgent 在裁切 oversized final/update tool result 时，必须把原始 body 写入会话期共享 `SessionToolResultCache`，写入 key 包含 `session_id` 与稳定 item id。
- `AgentToolResult.details.lifecycle_path`、bounded preview 文本、Backbone ThreadItem id、lifecycle VFS `session/tool-results/{item_id}` 必须使用同一个 item id。
- lifecycle provider 必须读取同一个共享 `SessionToolResultCache` 实例；默认空 cache 只能用于测试或显式 isolated 构造，不能出现在生产 bootstrap 主链路。
- cache miss / expired 必须继续返回 bounded status；不得把 persisted bounded preview 当作原始 body，也不得把原始 body 写回 `SessionEvent`。
- projection、continuation、repository rehydrate 和 compaction 仍只恢复 persisted bounded fact；即使 cache body 可用，也不得自动读取 `result.txt` 扩写模型上下文。
- shell/terminal 现有 bounded output 行为保持；terminal full log 可读不作为本任务必须交付，除非能以 bounded retained source 明确闭环。
- 不引入数据库 schema 或 migration；会话期缓存仍是 runtime scoped，可随进程、TTL 或 session 生命周期失效。

## Acceptance Criteria

- [ ] 大 dynamic/MCP/native tool final result 被裁切后，`ToolExecutionEnd`、`AgentMessage::ToolResult`、Backbone `ItemCompleted`、下一轮 provider request 都只包含 bounded preview 与 truncation metadata。
- [ ] 同一大结果的 cache write 真实发生，写入 key 为 `session_id + stable item_id`，原始 sentinel 只存在于 `SessionToolResultCache` body，不进入 persisted event。
- [ ] ThreadItem id、`details.lifecycle_path`、bounded preview 中的 `lifecycle_path`、lifecycle VFS path 使用同一 stable item id。
- [ ] `lifecycle://session/tool-results/{item_id}/metadata.json` 在 cache 可用时显示 available 状态；`result.txt` 能读到 cache body，仍由 `fs_read` full-read / offset / limit 防御控制。
- [ ] cache missing / expired 时，metadata 与 `result.txt` 返回 bounded miss / expired status，不包含原始 sentinel。
- [ ] `session_events.notification_json`、NDJSON backlog、projected transcript、continuation、repository rehydrate 均不包含原始 sentinel。
- [ ] stream mapper tests 不再通过手写不一致 lifecycle path 通过；必须断言 mapped item id 与 lifecycle path id 一致。
- [ ] API / local runtime bootstrap 使用同一共享 cache 传入 PiAgent connector 与 lifecycle provider。
- [ ] 前端 bounded 展示继续可见，不因 item id 收束破坏 tool card / command card 渲染。

## Out Of Scope

- 长期 artifact / object store / 冷热分层。
- 跨 Agent 通用大结果协议。
- 前端完整输出浏览器。
- terminal PTY 完整原文长期留存。
- 数据库 schema 调整。

## Open Questions

无阻塞问题。实现推荐采用 `{turn_id}:{tool_call_id}` 作为 stable tool result item id，原因是该坐标可在 AgentLoop producer 边界提前计算，也能被 stream mapper 和 lifecycle VFS 复用。
