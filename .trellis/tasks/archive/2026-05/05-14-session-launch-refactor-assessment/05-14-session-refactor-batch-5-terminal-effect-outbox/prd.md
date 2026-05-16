# Session refactor batch 5 terminal effect outbox

## Goal

将 session turn 的终态事实与终态副作用拆开：`turn_terminal` event 必须先持久化并更新 `SessionMeta.last_execution_status`，随后所有业务副作用以 typed terminal effect 写入 durable outbox，再由 dispatcher 执行、记录成功或失败。

## Current Fact

- `SessionTurnProcessor` 当前在同一个后台任务里直接执行四类终态副作用：
  - `SessionTerminal` hook trigger 评估。
  - `PostTurnHandler::execute_effects`。
  - `SessionTerminalCallback::on_session_terminal`。
  - hook `BeforeStop == continue` 后的 auto-resume 请求。
- `turn_terminal` notification 虽然先通过 `persist_notification` 写入，但后续副作用失败只写 warn 或被忽略，没有可查询、可重试、可审计的 outbox 事实。
- `PostTurnHandler` / `SessionTerminalCallback` 是运行期 trait object，当前不能跨进程恢复，因此本批需要先定义可持久化的 effect payload 与 dispatcher 结果，不继续把 trait object 当作事实源。

## Requirements

- 新增 terminal effect outbox 领域模型，至少覆盖：
  - `hook_effects`：由 SessionTerminal hook 产出的 `HookEffect` 列表。
  - `session_terminal_callback`：平台级 terminal callback。
  - `hook_auto_resume`：hook continue 驱动的 auto-resume。
- outbox record 必须包含 `id`、`session_id`、`turn_id`、`terminal_event_seq`、`effect_type`、`payload`、`status`、`attempt_count`、`created_at_ms`、`updated_at_ms`、`last_error`。
- `SessionPersistence` 必须提供创建、claim/list pending、mark succeeded、mark failed 的 outbox 接口；Memory / SQLite / PostgreSQL 实现保持同一语义。
- SQLite / PostgreSQL schema 初始化与 migration 都必须包含 outbox 表；删除 session 时 outbox 记录随 session 删除。
- `SessionTurnProcessor` 不再直接执行终态业务副作用；它只生成 terminal outcome、持久化 terminal event、请求 outbox dispatcher 处理 terminal effect plan。
- dispatcher 必须先写 pending record，再执行副作用；副作用失败不得回滚 terminal event，也不得破坏 active turn cleanup。
- 对于当前仍依赖运行期 trait object 的 effect，dispatcher 可以在进程内立即执行并持久化结果；重启后的 replay API 必须能查出 pending/failed record，但本批不强制实现 AppState 启动时自动 drain。
- 保持 Batch 4 的运行态边界：processor 不直接锁 runtime map，不绕过 `TurnSupervisor`。

## Non-goals

- 不重写 hook runtime 或 Rhai effect DSL。
- 不删除 `PostTurnHandler` / `SessionTerminalCallback` trait；本批先把调用点迁入 dispatcher。
- 不做 pending runtime command 事件化；那是 Batch 6。
- 不删除 `SessionHub` facade；那是 Batch 7。

## Acceptance Criteria

- [ ] `turn_terminal` event 持久化后，即使某个 terminal effect 失败，session meta 仍保持正确 terminal status。
- [ ] outbox pending / succeeded / failed 状态可在 Memory 与 SQLite 测试中验证。
- [ ] PostgreSQL repository 编译通过，migration 包含 terminal effect outbox 表。
- [ ] `SessionTurnProcessor` 不再直接调用 `execute_effects` / `on_session_terminal` / `request_hook_auto_resume`。
- [ ] 现有 `session::hub` cancel、hook auto-resume、terminal callback 相关测试通过或按 outbox 新语义更新。
- [ ] 新增 focused tests 覆盖：effect 失败不破坏 terminal fact、outbox attempt/status 更新、dispatcher 顺序为 event → pending effect → effect execution。

## Notes

- 当前项目未上线，不保留旧 API / 旧 schema fallback；需要同步更新 migrations 与 repository 初始化。
- 本批的核心价值不是“多一层队列”，而是让终态事实、可恢复副作用、业务 handler 三者边界清楚。
