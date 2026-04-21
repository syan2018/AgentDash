# ACP 流内容正确性 E2E + workflow 终止链路排查

## Goal

为 ACP 会话"流式内容渲染"建立一套跨端（Playwright + 真后端）回归测试，锁住
mergeStreamChunk / 多 turn / tool_call 生命周期等目前容易出诡异 bug 的行为。
同时排查 workflow 禁止终止 session 之后出现的相关 bug（具体症状待用户补齐）。

背景：用户感到当前行为诡异，手工测试无法稳定复现，需用 E2E 把"契约"锁死，
回归时一目了然。前置修复（cancel 按钮 + 重复绘制）已落，但没有测试守住。

## What I already know

### 仓库现状
- Playwright 配置齐全：[playwright.config.ts](../../../playwright.config.ts) 已经通过 `scripts/dev-joint.js --skip-build` 串起 webServer + backend，
  `/api` 走 `127.0.0.1:3011`，webServer 由测试自动拉起。
- [tests/e2e/](../../../tests/e2e/) 现有 4 个 spec（`app-smoke`、`story-context-injection`、`task-agent-binding`、
  `task-drawer-return`）；`story-context-injection.spec.ts` 演示了"直接打 API 建 project/workspace/session"的测试风格。
- 前端已有 Vitest 覆盖 `reduceStreamState` 分支：[useAcpStream.test.ts](../../../frontend/src/features/acp-session/model/useAcpStream.test.ts) / [streamTransport.test.ts](../../../frontend/src/features/acp-session/model/streamTransport.test.ts)。

### 关键路径
- SSE: `GET /api/acp/sessions/:id/stream`，实现在 [routes/acp_sessions.rs:1607](../../../crates/agentdash-api/src/routes/acp_sessions.rs#L1607)
- 历史回放: `GET /api/acp/sessions/:id/events` → [routes/acp_sessions.rs:347](../../../crates/agentdash-api/src/routes/acp_sessions.rs#L347)
- 取消: `POST /api/acp/sessions/:id/cancel` → [routes/acp_sessions.rs:1466](../../../crates/agentdash-api/src/routes/acp_sessions.rs#L1466) → `session_hub.cancel`
- 目前**无** test-only 事件注入端点。
- 前端 chunk 合并契约：[useAcpStream.ts](../../../frontend/src/features/acp-session/model/useAcpStream.ts) `mergeStreamChunk` +
  `applyEventToEntries`（优先 messageId → turnId+entryIndex → tail-merge；若 turnId 不同不合并）。
- 前端 entry 聚合：[useAcpSession.ts](../../../frontend/src/features/acp-session/model/useAcpSession.ts) `aggregateEntries`
  （info_gather / file_edit / thinking 各自折叠规则）。

### Workflow 终止相关
- [stop_gate_checks_pending.rhai](../../../crates/agentdash-application/scripts/hook-presets/stop_gate_checks_pending.rhai)：
  `before_stop` hook，在 completion checks 未满足时返回 `completion.mode = "stop_gate"` + inject 若干"不要结束 session"的 constraint。
- [session_terminal_advance.rhai](../../../crates/agentdash-application/scripts/hook-presets/session_terminal_advance.rhai)：
  `session_terminal_matches` 策略下，返回 `satisfied: false` 等待 runtime 推进。
- **用户症状（2026-04-21）**：agent 在被 stop 之后喜欢**复读**，怀疑会话恢复时注入了重复内容。
- 嫌疑路径：
  - [session/continuation.rs:204](../../../crates/agentdash-application/src/session/continuation.rs#L204)
    `build_restored_session_messages_from_events` —— 根据 DB 事件重建消息，dedup key 基于
    `restored_user_key` / `restored_assistant_key(event, message_id)`；若 message_id 在 stop_gate
    场景下被重分配，可能产生重复条目。
  - [routine/executor.rs:429](../../../crates/agentdash-application/src/routine/executor.rs#L429)
    `resolve_session_prompt_lifecycle` 基于 `has_live_runtime` + `supports_repository_restore`
    决定是否带历史；stop 之后的下一轮是否应带历史、带多少，是关键开关。
  - stop_gate 注入的 `constraint` 如果被当成持久化历史留下，下一轮 prompt 会携带它作为系统消息 → agent 读到就重复响应。

## Assumptions (temporary)

- 注入端点只用于测试：debug_assertions 或 `AGENTDASH_ALLOW_TEST_INJECT=1` 双保险。
- 注入端点只对 local backend（或 e2e backend id）开放；生产 build 彻底编不进去。
- E2E 使用真 backend，executor 用已有的 stub / 不起真实 prompt，而是直接 inject session update 到事件总线。

## Decision (ADR-lite)

**Context**: 用户观察到 ACP 会话流式渲染行为诡异（chunk 合并、多 turn、tool_call 生命周期），
以及 workflow 禁止 session 终止后 Agent 复读（疑似历史恢复重复注入）。需要跨端回归测试
锁死契约，且不能只有 mock 路由测试（那样绕过后端事件总线与 SSE 管道）。

**Decision**:
- 后端新增独立 debug router `POST /api/_debug/sessions/:id/emit`（`#[cfg(debug_assertions)]` +
  `AGENTDASH_ALLOW_TEST_INJECT=1` 双闸门），测试侧推送任意 SessionNotification。
- 本任务**只做** ACP 流内容正确性 E2E（Case 1–6），多 turn 通过注入 `turn_started` /
  `turn_completed` session_info_update 驱动。
- **复读 bug 单独另起任务**（既包含 fail-lock 测试也包含根因修复）：适合 Rust 集成测试
  层（`build_restored_session_messages_from_events`）而非 E2E —— 因为 E2E 没有真实
  executor 可读 prompt。这次不做。

**Consequences**:
- debug router 是新增攻击面，必须双闸门 + CI 验证 release build 不含此路由。
- 后续"复读 bug"任务要能独立：本任务不引入跨模块假设。
- 流正确性 Case 覆盖面决定未来 chunk 合并逻辑的改动安全性。

## Final Scope (确认后的 MVP)

### In
1. 后端：`POST /api/_debug/sessions/:id/emit` debug router，双闸门。
2. E2E spec `tests/e2e/acp-stream-correctness.spec.ts`，覆盖 Case 1–6：
   - Case 1 chunk 累加 → DOM 拼接正确
   - Case 2 重复 / 前缀重叠 / 完全重叠不吞字
   - Case 3 乱序 event_seq 按 seq 应用且不重复
   - Case 4 跨 turn 不合并
   - Case 5 tool_call pending→completed 同批可见开始态（sync flush）
   - Case 6 multi-turn 场景（两个 turn 消息气泡分开 + 状态正确切换）
3. Release build 验证：`cargo build --release` 后跑一条冒烟测试 POST debug endpoint → 404/405。
4. `.trellis/spec/backend/` 记录 debug router 契约。

### Out (移到"workflow-agent-replay-on-stop" 新任务)
- "Agent 被 stop 后复读" 根因定位 + 修复
- `build_restored_session_messages_from_events` 的 fail-lock 测试
- stop_gate 注入内容是否应持久化到历史的策略讨论

## Open Questions

- **Preference（非阻塞）**：Case 6 multi-turn 驱动要不要暴露一个"启动新 turn"的辅助接口，
  还是纯靠注入 turn_started/turn_completed？倾向纯注入，保持端点通用。
- **Preference（非阻塞）**：spec 组织放 `tests/e2e/acp-stream-correctness.spec.ts`
  单文件还是拆 `tests/e2e/stream/` 多文件？倾向单文件，≤6 个 it。

## Requirements (evolving)

### 后端
- 新增 test-only 事件注入端点，支持：
  - 推送任意 `SessionNotification` 到指定 session 的事件总线
  - 推 `turn_started` / `turn_completed` / `turn_interrupted` session_info_update
  - 返回分配到的 `event_seq` 方便测试断言
- 守卫：debug_assertions + env flag 双闸门，release build 返回 404。

### E2E Spec（tests/e2e/acp-stream-correctness.spec.ts）
- **Case 1**: chunk 顺序累加 → DOM 最终文本 = 所有 chunk 拼接
- **Case 2**: 重复 chunk / 部分前缀重叠 / 完全重叠 → 不吞字、不丢片段
- **Case 3**: 乱序 event_seq（小 seq 后到达）→ 按 seq 排序、不重复应用
- **Case 4**: 跨 turn 不合并（turn_A chunk 和 turn_B chunk 不连接成一条消息）
- **Case 5**: tool_call(pending) → tool_call_update(completed) 同批次到达时，DOM 要先看到 pending 态（sync flush）
- **Case 6**: multi-turn 场景（turn_started → chunks → turn_completed → turn_started → chunks → turn_completed），
  两个 turn 的气泡分开、token usage / 状态正确切换

### Workflow 终止排查（需 symptom 后再细化）
- 基于用户补充的症状定位根因（`before_stop` hook 是否错误阻塞 cancel path、state 机是否卡 running 等）
- 补 E2E Case：session 被 stop_gate 阻塞时，用户 cancel 是否仍然生效、UI 是否恢复到 idle

## Acceptance Criteria (evolving)

- [ ] `POST /api/_debug/sessions/:id/emit` 路由在 debug build + `AGENTDASH_ALLOW_TEST_INJECT=1` 下可用；release build 编译不进；debug build 但无 env 时返回 404。
- [ ] 新增 `tests/e2e/acp-stream-correctness.spec.ts`，本地 `pnpm e2e` 跑通 Case 1–6。
- [ ] `.trellis/spec/backend/` 新增 debug router 契约文档；`cross-layer` 加 spec 约定"测试注入端点使用规范"。
- [ ] workflow 复读相关的任务已创建（链接在本 PRD 的 "Out of Scope" 段）。

## Definition of Done

- Tests added, lint/typecheck/clippy 通过
- 注入端点 gate 严格（debug + env），有 release build 验证
- `.trellis/spec/backend/` 记录"测试注入端点"的契约
- 相关 workflow 终止 bug 修复（若定位到）

## Out of Scope (explicit)

- 真实 executor 的端到端（不起真实 prompt），本任务只测前后端事件管道 + 渲染
- React Profiler 级 render-count 断言（属于组件层 Vitest，后续单独开）
- cancel 按钮点击交互（上一轮已修，后续如果也要 E2E 锁再另起）
- **Workflow 禁止终止后 Agent 复读 bug**：挪到单独任务，含 fail-lock Rust 集成测试 +
  根因修复。理由：E2E 层缺真实 executor 无法直接断言"Agent 是否复读"；修复面在
  `session/continuation.rs` + `routine/executor.rs` 的 prompt 重建链路。

## Technical Notes

### 注入端点草案
```
POST /api/_debug/sessions/:id/emit
Body: { notification: SessionNotification, turn_id?, entry_index?, tool_call_id? }
Response: { event_seq: u64 }
挂载:  #[cfg(debug_assertions)] only; release build 无此路由模块
运行时: 额外校验 std::env::var("AGENTDASH_ALLOW_TEST_INJECT") == "1"，否则 404
实现:  直接借用 session_hub / stream_hub 的 publish 入口，确保走与真 agent 完全同一条事件管道
```

### 参考文件
- [routes/acp_sessions.rs](../../../crates/agentdash-api/src/routes/acp_sessions.rs)
- [session/hub.rs](../../../crates/agentdash-application/src/session/hub.rs) — cancel + event bus
- [useAcpStream.ts](../../../frontend/src/features/acp-session/model/useAcpStream.ts)
- [useAcpStream.test.ts](../../../frontend/src/features/acp-session/model/useAcpStream.test.ts) — 现有 seq 构造参考

### Implementation Plan（拆 PR）

- **PR1 (backend)**：debug router 模块 + 双闸门 + Rust 单测（release build 不含 route；
  debug build 无 env → 404；debug build + env → 正确写入事件总线并返回 event_seq）。
- **PR2 (spec + docs)**：`.trellis/spec/backend/` 新增 `debug-inject-endpoint.md`，
  `cross-layer/` 新增测试端点使用规范。
- **PR3 (E2E)**：`tests/e2e/acp-stream-correctness.spec.ts` Case 1–6，配合 helper
  封装"建 session → 注入事件 → 读 DOM"。
