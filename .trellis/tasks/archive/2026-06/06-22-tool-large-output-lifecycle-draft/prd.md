# PiAgent 工具大返回裁切与生命周期映射

## Goal

让 PiAgent 在工具调用、shell/terminal 输出或本机 MCP 返回巨大内容时保持会话可继续：模型上下文、`SessionEvent`、Postgres `session_events`、NDJSON 历史补发和前端 `rawEvents` 都只接收有界内容；需要读取更多内容时，只通过现有 `lifecycle_vfs` + `fs_read` 的受控路径进入。

## Background

当前 PiAgent loop 与工具路由在云端统一执行，以便访问云端虚拟 mount、本机 relay mount、lifecycle mount、inline/canvas/skill asset 等多种 mount。`fs_read` 已经在 reader 层提供 256 KiB / 5000 行的 full-read 防御，并支持 `offset/limit` 分段读取；真正缺口在 producer 层：普通 tool、MCP tool、`shell_exec` final result、shell streaming update、interactive terminal output 仍可能把巨大原文写入 `AgentToolResult`、Backbone event、SessionEvent、前端和后续模型上下文。

本任务聚焦 PiAgent 自身。其它 Agent 是否支持 lifecycle mount、是否能写入相同 ref surface，不作为本方案前提。

## Requirements

- PiAgent 工具结果必须在进入 `ToolExecutionEnd`、`AgentMessage::ToolResult`、Backbone `ItemCompleted`、SessionEvent 持久化和下一轮 provider request 之前完成有界化。
- `ToolExecutionUpdate`、`shell_exec` live output 和 interactive terminal output 必须在各自现有 producer 边界有界化，避免实时 delta 绕过 final-result 裁切。
- `SessionEvent` / ThreadItem 持久化的内容必须表达“模型实际看到的有界内容、裁切策略、原始大小、inline 大小”，而不是原始大 payload。
- lifecycle mount 下只增加项目已有 VFS 模型可解释的只读路径；读取仍受 `fs_read` 的 full-read 防御和 `offset/limit` 机制约束。
- 若某个 result/log body 当前没有实际 retained 来源，lifecycle read 应返回明确的有界缺失状态。
- terminal 需要沿用 `RetainedOutputBuffer` / `ToolShellTruncationInfo` 语义处理：`shell_exec` final result、shell streaming update、interactive terminal durable event 都必须避免把完整输出写入云端 SessionEvent。
- ContextProjector、continuation、repository rehydrate 和 compaction 只能消费持久化的有界内容，不能自动通过 lifecycle path 重新内联原始大内容。
- 前端 session hydrate、NDJSON stream 和 terminal store 应消费有界 event；第一阶段不要求完整输出展开 UI。
- 需要保留现有 tool call / provider call id、item id、entry_index、exit code、terminal id、shell session id、trace lineage 等关联信息。

## Acceptance Criteria

- [ ] 构造一个返回超大文本的 PiAgent dynamic/MCP tool 后，下一轮模型上下文、`ToolExecutionEnd`、`MessageEnd(ToolResult)`、Backbone `ItemCompleted` 都只包含有界内容和裁切 metadata。
- [ ] 对同一大结果查询 Postgres `session_events.notification_json`，长度受策略阈值控制，且不包含用于测试的原文 sentinel。
- [ ] `ToolExecutionUpdate` 和 `shell_exec` live output 的单条 event 与累计历史都保持有界，不再因为一个大 chunk 或大量小 chunk 线性写入原文。
- [ ] `shell_exec` final `CommandExecution.aggregated_output` 只保留有界输出，同时保留 exit code、状态、cwd、shell session/terminal 关联和 `ToolShellTruncationInfo` 等价信息。
- [ ] interactive terminal 仍能实时渲染当前输出，但历史 SessionEvent / NDJSON backlog 不承载完整 PTY 原文。
- [ ] `lifecycle://session/tool-results/...` 或等价路径可通过 `fs_read` 读取 metadata；若 result body 实际可用，full read 过大时沿用 `fs_read` 大文件防御，`offset/limit` 可分段读取。
- [ ] 缺失或不可用的 lifecycle result body 返回稳定、模型可理解的有界状态，不触发 panic、不重新生成大 SessionEvent。
- [ ] ContextProjector、continuation 和 repository rehydrate 在包含大工具结果的会话中只生成持久化有界内容，不把原始大文本放回模型输入。
- [ ] `/sessions/{id}/events` 和 NDJSON backlog 在包含大工具结果的会话中响应体可控，前端 `rawEvents` 不保存原始 sentinel。
- [ ] 生命周期投影中的 `session/events.json`、`session/tools`、`session/terminal` 暴露 preview/ref 语义一致，避免 raw-event 展开重新放大。

## Scope Decisions

本轮把“工具/terminal 大返回防爆”作为第一目标，先保证云端数据库、stream、前端和模型上下文不会被单条工具返回打爆。第一阶段只处理 PiAgent、SessionEvent、lifecycle_vfs 与 shell/terminal 现有链路。

`fs_read` 继续保持 reader 语义：它负责受控读取已有 VFS 内容，不因为读取结果过大而创建新的 artifact。大内容路径只由 PiAgent producer 边界或现有 shell/terminal retained 输出提供。
