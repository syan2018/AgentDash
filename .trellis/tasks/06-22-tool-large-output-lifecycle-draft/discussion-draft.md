# PiAgent Tool Result Bounding 讨论纪要

本文记录当前任务的收敛后共识。正式需求见 `prd.md`，技术设计见 `design.md`，派发计划见 `implement.md`。

## 背景

本轮问题来自一次工具调用返回体过大：如果最后一个 ToolCall 原文进入会话记录，后续模型请求会因为上下文过大而无法继续。风险不只在模型上下文，也包括 Postgres `session_events.notification_json`、NDJSON backlog、前端 `rawEvents`、ContextProjector 和 repository rehydrate。

本任务只处理 PiAgent。PiAgent loop 和工具路由在云端统一执行，因此它能访问云端虚拟 mount、本机 relay mount、lifecycle mount、inline/canvas/skill asset 等多种 mount。其它 Agent 的内部工具、transcript 和 mount 能力不作为本任务前提。

## 已确认的项目事实

- `fs_read` 已经有 full-read 防御：不带 `limit` 时受 256 KiB / 5000 行阈值约束，并支持 `offset/limit` 分段读取。
- `fs_read` 是 reader，职责是受控读取已有 VFS 内容。
- 普通 tool、MCP tool、`shell_exec` final result、`ToolExecutionUpdate`、relay shell output、interactive terminal output 是 producer 风险点。
- `RetainedOutputBuffer` / `ToolShellTruncationInfo` 已经存在，但 live relay event 仍可能绕过 retained buffer 的裁切。
- `SessionEvent` / `BackboneEnvelope` 同时影响持久化、NDJSON、前端和后续 projection，因此不能承载原始大 payload。

## 当前收敛方向

第一阶段只做现有链路上的有界化：

- `AgentToolResult.content` 是模型实际看到的有界内容。
- `AgentToolResult.details.truncation` 记录小型裁切 metadata。
- `ToolExecutionEnd`、`ToolExecutionUpdate`、`AgentMessage::ToolResult` 使用同一个有界结果。
- `shell_exec` 与 terminal live event 沿用 `RetainedOutputBuffer` / `ToolShellTruncationInfo` 语义处理输出大小。
- `SessionEventingService` 在 append 前增加兜底大小检查，保护数据库和 stream。
- `lifecycle_vfs` 增加 `session/tool-results` / `session/terminal` 下的只读 metadata/result 路径；实际读取仍通过 `fs_read` 和现有 VFS 防御。
- ContextProjector、continuation、repository rehydrate 和 compaction 只消费持久化的有界内容，不自动读取 lifecycle path。

## 命名约束

规划和后续派发统一使用项目里已有的边界名称：

- `AgentToolResult`
- `ToolExecutionEnd`
- `ToolExecutionUpdate`
- `AgentMessage::ToolResult`
- `BackboneEnvelope`
- `SessionEventingService`
- `ContextProjector`
- `continuation`
- `lifecycle_vfs`
- `fs_read`
- `shell_exec`
- `RetainedOutputBuffer`
- `ToolShellTruncationInfo`
- `CommandExecution.aggregated_output`
- `PlatformEvent::TerminalOutput`
- `details.truncation`
- `lifecycle_path`

这样 subagent 可以直接定位现有模块和函数，不会被讨论期概念误导到新增平台级抽象。

## Refined Scope

第一阶段的目标是防止单条工具/terminal 输出打爆模型上下文、数据库、stream 和前端。它不要求完整输出长期保留，也不要求前端提供完整输出展开体验。

`lifecycle_vfs` 的路径优先提供 metadata。只有当 result/log body 已经由现有 runtime 路径 retained 时，才暴露可读 `result.txt` 或 terminal log；缺失时返回稳定有界状态。

## 参考实践的取舍

Codex 值得借鉴的是：

- 在输出 collector / live delta 边界做 cap。
- durable transcript 不保存高频 delta。
- resume 从有界持久历史重建。

Claude Code 值得借鉴的是：

- 模型看到 preview/path，而不是完整大结果。
- Bash 输出先进入本地文件，通知只携带状态和路径。
- resume 重用稳定 replacement 文本。

AgentDash 的实现需要更贴近自身架构：`SessionEvent` 是跨后端事实流，因此必须保存有界事实；lifecycle path 是 VFS 读取 surface，而不是单独工具集。

## 第一阶段验收重点

- 巨大 `AgentToolResult` 不进入模型上下文、AgentEvent、Backbone、SessionEvent、NDJSON 或 frontend `rawEvents`。
- `ToolExecutionUpdate` 和 terminal live output 不绕过 final-result 有界化。
- `CommandExecution.aggregated_output` 有界，同时保留 exit code、state、cwd、session id、terminal id、`next_seq` 和 truncation 信息。
- ContextProjector / continuation / rehydrate 不自动读取 lifecycle path。
- lifecycle metadata/result 路径能通过 `fs_read` 按现有防御读取或返回有界缺失状态。
