# Lifecycle 控制面重构收口复核 Closeout

## 分阶段提交

- `de53d8ce` `test(lifecycle): 修复 dispatch DTO 收口基线`
- `afec675e` `refactor(lifecycle): 收束 runtime session anchor 证据链`
- `653b94ab` `refactor(session): 移除旧 construction 生产暴露`
- `e10dd361` `refactor(frontend): 收束 agent-frame 优先运行态派生`
- `5b0b379a` `refactor(lifecycle): 收束 activity artifact 与 runtime trace 状态`

## 最终验证

- `cargo check --workspace` 通过。
- `cargo test -p agentdash-domain --lib -- --format terse` 通过，100 tests。
- `cargo test -p agentdash-application --lib -- --format terse` 通过，672 tests。
- `pnpm --filter app-web run typecheck` 通过。

## 残留扫描

- `rg "runtime_trace_refs\\[0\\]|primarySessionId" packages/app-web/src` 无命中。
- `rg "HookSessionRuntimeInfo" packages/app-web/src` 无命中。
- `rg "load_port_output_map" crates/agentdash-application/src/session crates/agentdash-application/src/workflow` 无命中。
- `rg "RuntimeContextInspectionPlan|ResolvedSessionOwner" crates/agentdash-application/src --type rust` 仅剩 test-only construction fixture / test adapter / frame assembly 测试支撑命中。

## Smoke

- 默认 embedded PostgreSQL 数据根中已应用 migration 与当前 migration 8 checksum 不一致，`pnpm dev` 的首次 smoke 在 server migration 阶段停止。
- 使用临时 `AGENTDASH_DATA_ROOT` 重新运行 `pnpm dev`，API `/api/health` 返回 200，Web `/` 返回 200，本机 runtime 注册为 online。
- `GET /api/backends/runtime-summary` 返回 online backend，`active_session_count = 0`。
- 创建临时 Project 后，`GET /api/projects/{project_id}/active-agents` 返回空 `runs` / `agents`。
- 真实 Project Agent launch 在空临时库缺 `builtin.freeform_session` 时停止；该 smoke 已覆盖 dev launch、runtime registration、frontend active list read path。
