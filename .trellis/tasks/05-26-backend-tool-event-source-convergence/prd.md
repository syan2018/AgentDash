# 后端工具事件事实源收束

## Goal

将工具调用事件事实源收束到 AgentDash Backbone/Codex 协议；vibe-kanban ActionType 仅保留在 legacy connector 边界，不作为主链路语义模型。

## Background

近期 `05-26-frontend-toolcall-card-review` 的 P2 把 `executors::logs::ActionType`
映射回 Codex `ThreadItem` variant，解决了所有工具调用退化成 `DynamicToolCall`
的可见问题。但这条修复仍把 vibe-kanban 的 normalized log 形态当成工具语义中间层，
导致 AgentDash 自有执行链路需要跟随外部 `ActionType` 枚举演进。

项目当前预研阶段可以直接调整协议边界。后端工具调用事件应以 AgentDash Backbone /
Codex-compatible event 为主事实源；内嵌工具应在执行层产出结构化工具事实，connector
只做薄投影。vibe-kanban 仍可作为执行器解决方案接入，但其 `NormalizedEntry` /
`ActionType` 语义只属于该 connector 的 legacy 输入。

## Requirements

R1. **主链路以 Backbone/Codex 工具事件为事实源。** AgentDash 自有 agent /
内嵌工具链路直接投影为 `BackboneEnvelope` / Codex `ThreadItem`，公共后端模块不再
以 vibe-kanban `ActionType` 作为工具语义模型。

R2. **vibe-kanban 语义收束到 legacy connector 边界。** `executors::logs::*`
引用保留在 vibe-kanban 执行器接入路径中，模块命名与代码注释体现其 legacy adapter
职责，避免新 connector 复用 `ActionType` 映射作为模板。

R3. **保留现有 P2 的用户可见修复效果。** `CommandRun` / `FileEdit` / `Search`
等 legacy 输入仍能在 vibe-kanban connector 中投影到对应 `ThreadItem`，但该逻辑不再
被表述为平台通用工具语义。

R4. **修复 dynamic fallback 信息保真。** legacy adapter 中无法落到专用
`ThreadItem` variant 的工具调用仍要保留原始入参与可用输出内容，避免 `FileRead` /
`WebFetch` / `TaskCreate` / `Other` 这类分支丢失 `entry.content` 或 result。

R5. **为 AgentDash 自有工具事实铺设结构化出口。** 使用
`AgentDashThreadItem` 作为 Codex `ThreadItem` 的项目超集：Codex 已有语义直接复用
Codex variant，Codex 未覆盖的 read/search/list 工具事实由 AgentDash native variant
表达。

R6. **协议扩展归属 `agentdash-agent-protocol` / `agentdash-agent-types`。** Codex
现有 variant 不足表达 AgentDash 自有语义时，在项目自有 protocol/types 层扩展，
而不是引入或延续 vibe-kanban 中间语义。

R6a. **Agent 类型出口以 Codex 优先、AgentDash 加法扩展为准。** Codex Protocol
已经定义的 `ThreadItem`、状态 enum 和输出片段直接 re-export；Codex 未覆盖的
AgentDash 自有语义集中在 `agentdash-agent-types::AgentDashNativeThreadItem`，避免为
相同语义再包一层同义 enum。

R7. **测试覆盖边界语义。** 后端测试需要覆盖 legacy adapter 的保真投影，以及
`pi_agent` 对 `shell_exec`、`fs_read`、`fs_grep`、`fs_glob` 的
`AgentDashThreadItem` 映射。

## Acceptance Criteria

- [ ] AC1: `executors::logs::ActionType` / `NormalizedEntry` 的生产代码引用集中在
      vibe-kanban legacy connector / adapter 命名空间下，公共 Backbone 或 AgentDash
      自有工具链路不引用它们。
- [ ] AC2: legacy vibe-kanban 输入中 `CommandRun` / `FileEdit` / `Search` 仍分别
      投影为 `CommandExecution` / `FileChange` / `WebSearch`。
- [ ] AC3: legacy vibe-kanban 输入中 `FileRead` / `WebFetch` / `TaskCreate` /
      `Other` 走 dynamic fallback 时保留原始入参以及 `entry.content` 可用输出。
- [ ] AC4: `AgentDashThreadItem` 从 `agentdash-agent-types` 导出，wire shape 为
      Codex `ThreadItem` 与 AgentDash native item 的 union；状态 enum 直接沿用
      Codex Protocol。
- [ ] AC5: `pi_agent` 工具事件中 `shell_exec` 使用 Codex `CommandExecution`，
      `fs_read` / `fs_grep` / `fs_glob` 使用 AgentDash native item，其他工具保持
      Codex `DynamicToolCall`。
- [ ] AC6: `cargo test -p agentdash-agent-types --lib`、
      `cargo test -p agentdash-agent-protocol --lib`、
      `cargo test -p agentdash-executor --lib` 通过。

## Notes

- `agentdash-agent-protocol::backbone::thread_item` builder 继续作为 Codex `ThreadItem`
  构造边界，原因是它集中处理 Codex 内部 path 类型和 serde wire shape。
- 本轮优先完成后端事实源边界与 read/search/list native item；其他 Codex 未覆盖工具
  可按同一 `AgentDashNativeThreadItem` 路线小步扩展。
