# 实施计划：后端工具事件事实源收束

## Phase 1 — 任务计划归位

- [x] P1.1 创建独立后端任务，避免继续沿用“前端工具卡片”任务标题承载协议修复。
- [x] P1.2 PRD 改写为 Backbone/Codex 事实源收束目标。
- [x] P1.3 design 明确两条链路：AgentDash ThreadItem 超集主链路与 vibe-kanban legacy adapter。

## Phase 2 — legacy vibe-kanban 边界归位

- [x] P2.1 将 `normalized_to_backbone.rs` 移入/重命名为 vibe-kanban legacy mapper。
- [x] P2.2 将 `NormalizedToBackboneConverter` 改名为
      `VibeKanbanLogToBackboneConverter`。
- [x] P2.3 将 `build_thread_item(ActionType...)` 改名为
      `legacy_action_type_to_thread_item`，表达其边界职责。
- [x] P2.4 更新 `executor_session.rs` 引用路径与命名。
- [x] P2.5 grep 验证 `ActionType` / `NormalizedEntry` 只出现在 legacy mapper 和
      vibe-kanban connector 入口。

## Phase 3 — legacy dynamic fallback 保真修复

- [x] P3.1 抽 `legacy_dynamic_tool_call` helper，统一保留 arguments / content_items / success。
- [x] P3.2 修复 `FileRead` / `WebFetch` / `AskUserQuestion` / `TaskCreate` / `Other`
      fallback 未携带 `entry.content` 的问题。
- [x] P3.3 补单测：dynamic fallback 带非空 fallback content 时产出
      `DynamicToolCallOutputContentItem::InputText`。

## Phase 4 — AgentDash ThreadItem 超集

- [x] P4.1 升级 `codex-app-server-protocol` 到 `rust-v0.133.0`，并更新新增字段调用点。
- [x] P4.2 在 `agentdash-agent-types` 增加 `AgentDashThreadItem`，Codex 原生 item
      通过 `Codex(codex::ThreadItem)` 复用。
- [x] P4.3 增加 `AgentDashNativeThreadItem`，覆盖 `fs_read` / `fs_grep` / `fs_glob`
      三类 Codex 当前没有一等 variant 的 read/search/list 工具事实。
- [x] P4.4 `BackboneEvent::ItemStarted` / `ItemCompleted` 改用 AgentDash item 通知，
      保留 Codex bridge 的 `from_codex` 包装入口。
- [x] P4.5 命令执行直接使用 Codex `CommandExecution` 原生结构，工具私有 metadata
      不参与 item 分类。

## Phase 5 — pi-agent 与应用层接入

- [x] P5.1 `pi_agent::stream_mapper`：`shell_exec` 映射为 Codex `CommandExecution`。
- [x] P5.2 `pi_agent::stream_mapper`：`fs_apply_patch` 映射为 Codex `FileChange`。
- [x] P5.3 `pi_agent::stream_mapper`：`fs_read` 映射为 `AgentDashNativeThreadItem::FsRead`。
- [x] P5.4 `pi_agent::stream_mapper`：`fs_grep` 映射为 `AgentDashNativeThreadItem::FsGrep`。
- [x] P5.5 `pi_agent::stream_mapper`：`fs_glob` 映射为 `AgentDashNativeThreadItem::FsGlob`。
- [x] P5.6 application / persistence 中按 `AgentDashThreadItem` 提取 tool call id、
      artifact patch、continuation tool result 与 journey tool projection。

## Phase 6 — 生成与验证

- [x] P6.1 重新生成 `packages/app-web/src/generated/backbone-protocol.ts`。
- [x] P6.2 `cargo fmt --all --check`。
- [x] P6.3 `cargo test -p agentdash-agent-types --lib`。
- [x] P6.4 `cargo test -p agentdash-agent-protocol --lib`。
- [x] P6.5 `cargo test -p agentdash-executor --lib`。
- [x] P6.6 grep 验证当前协议命名、状态类型与 item union 没有旧方案残留。
- [x] P6.7 `cargo check -p agentdash-application` 与
      `cargo check -p agentdash-infrastructure`。

## Risk Notes

- `AgentDashThreadItem` 使用 untagged union 复用 Codex `ThreadItem` 的 wire shape；
  AgentDash native item 自身仍是 `type` tagged enum。这样前端看到的是一层自然的
  item union，而不是双层 envelope。
- 工具私有 metadata 仍可服务诊断，但 item lifecycle 的协议事实由
  `AgentDashThreadItem` 直接表达。
