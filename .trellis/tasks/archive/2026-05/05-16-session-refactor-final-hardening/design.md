# Session 重构最终收尾清洁设计

## Architecture Direction

保持当前主线不变：

```text
LaunchCommand
  -> SessionConstructionProvider
  -> SessionConstructionPlan
  -> SessionLaunchPlanner
  -> LaunchExecution
  -> SessionLaunchExecutor / prompt_pipeline
  -> TurnProcessor
  -> SessionEvent / TerminalEffectOutbox
```

本任务的设计重点不是再加一层抽象，而是让已有层的边界更硬：

- Construction 负责最终 session 事实：owner、workspace、working directory、VFS、MCP、capability、context、executor。
- Launch planning 只负责 runtime-only 决策：prompt payload、lifecycle、restore、hook、follow-up、runtime command apply plan、terminal effect plan。
- Execution 只执行计划：claim/activate turn、写事件、调用 connector、提交 accepted 后副作用、启动 processor/adapter。
- Terminal 先落 terminal fact，再写 durable outbox，再执行/重放副作用。

## Issue 1: Terminal Persist Failure Cleanup

当前风险：`SessionTurnProcessor::run` 在 terminal notification 持久化失败时直接 return，跳过 `clear_active_turn`。

设计：

- 将 active turn cleanup 移到 terminal handling 的必达路径。
- terminal persist 失败时不派发 terminal effect outbox，但必须释放 runtime turn。
- cleanup 失败或重复 cleanup 只记录日志，不阻断 processor 退出。

推荐实现形态：

```text
terminal_event_result = persist terminal
clear_active_turn(session_id)
if terminal_event_result failed:
  log and return
dispatch_terminal_effects(...)
```

## Issue 2: Runtime Command Apply Once

当前风险：connector accepted 后 `mark_runtime_commands_applied` 失败只 warning，requested command 可能下一轮重复应用。

设计：

- connector accepted 是 runtime command “已被本轮采纳”的边界。
- 如果 `mark_runtime_commands_applied` 失败，不能静默成功返回。
- 最小方案：尝试 `mark_runtime_commands_failed(command_ids, error)`，并返回 `ConnectorError::Runtime`，同时确保 active turn 清理或 terminal 状态不会卡死。
- 更稳方案：引入 `RuntimeCommandApplyCommitError`，在 accepted 后把 turn 标记为 failed terminal 并清理 runtime；后续可人工/重试。

本任务推荐最小稳妥方案：accepted 后 applied 标记失败时，写 failed 状态并让 launch 返回错误；不得留下 requested。

## Issue 3: Context Query 与 Launch Projection 一致

当前风险：

- context 查询使用 `SessionConstructionPlanner::plan_*_context_query`。
- launch 使用 `build_session_construction_for_launch`，并在 `finalize_session_construction_for_launch` 中处理 runtime command overlay、cached capability、source MCP、executor fallback、skill/guideline discovery。

设计：

- 抽出共享 finalization use case，例如：

```rust
enum SessionConstructionProjectionMode {
    Launch,
    Inspect,
}

finalize_session_construction_projection(plan, facts, mode) -> SessionConstructionPlan
```

- `Launch` 模式要求满足 `validate_for_launch()`。
- `Inspect` 模式不要求 prompt payload，但应复用 VFS/MCP/capability/runtime command overlay 的同一裁决逻辑；无法补齐 executor 时允许返回 context projection，但需要 trace 表明 `executor_source=unresolved.inspect`。
- API route 继续只负责 auth/DTO，不直接拼装 VFS/capability。

## Issue 4: Tab Layout 静默兼容清理

当前前端存在静默吞错路径，后端只支持 title patch。

决策：正式支持 `tab_layout`。

- 给 `SessionMeta` 增加 `tab_layout: Option<serde_json::Value>`。
- 新增 Postgres/SQLite migration。
- `PATCH /sessions/{id}/meta` 支持可选 `title` 与 `tab_layout`，且至少一个字段存在。
- `GET /sessions/{id}/meta` 或现有 meta 读取路径必须能返回 `tab_layout`。
- 前端移除静默 catch，保存失败要暴露真实错误。

保留该功能，因为调用方已经存在，删除会影响 workspace panel 使用体验。

## Issue 5: 更薄的层

用户已确认可以接受较高风险的边界清理。本任务可以从“低风险薄化”上调到“中等风险收口”，但仍避免无关重写：

- API route：保持 prompt handler 仅 auth、DTO、launch service、错误映射。
- API bootstrap：把 `finalize_session_construction_for_launch` 的纯逻辑抽成共享 projection/finalization use case，让 launch 和 context query 都走同一条裁决路径。
- `assembler.rs`：不在本任务内大拆，但清理 stale 注释与对旧 `SessionRequestAssembler` 语义的外部暴露；必要时新增后续拆分 TODO 到 spec/task。
- `prompt_pipeline.rs`：只修复副作用边界，不做大切块；避免同时改变过多行为。

## Data And Migration Notes

- `tab_layout` 需要 PostgreSQL 与 SQLite 同步迁移。
- 本项目预研期不需要兼容旧字段，但 migrate 必须能在当前开发数据库上前进。
- `SessionMeta` 当前 serde 使用 camelCase，前端 service 对 session meta 也使用 camelCase；新增字段需明确 JSON 名称，避免 `tab_layout` / `tabLayout` 混用。

## Rollback Considerations

- terminal cleanup 修复可独立回滚。
- runtime command apply failure 行为从 warning 变成错误可能暴露已有存储问题，但这是更正确的失败模式。
- tab_layout 回滚需同时回滚 migration 和前端调用。
