# 架构二三档收敛跟踪 - Implement

## Execution Rules

- 不创建 child task。
- 每次只启动一个 work group 的实现，除非并行 agent 的写入文件范围完全不重叠。
- 启动任何实现前先确认当前工作区已有未提交修改，不能回滚或整理非本任务改动。

## Work Group A - Contract Generation Test Surface

建议顺序：第一组。

可能涉及文件：
- `crates/agentdash-contracts/src/generate_ts.rs`
- 可能新增 `crates/agentdash-contracts/src/contract_generation.rs`
- 仅在 contract 输出有意变化时修改 generated files

清单：
- [ ] Identify pure generation rules currently embedded in the CLI.
- [ ] Extract a pure generation module that returns an in-memory generated file set.
- [ ] Add small fixture tests for dedup/import/header/common type behavior.
- [ ] Keep CLI behavior compatible with current `contracts:check` command shape, but without compatibility branches.
- [ ] Run `cargo test -p agentdash-contracts` and `pnpm run contracts:check`.

## Work Group B - Runtime Snapshot Generated Contracts

建议顺序：第二组；如果与 C 组写入文件不重叠，可以并行。

可能涉及文件：
- `crates/agentdash-contracts/src/backend/*`
- `crates/agentdash-api/src/dto/backend.rs`
- `crates/agentdash-api/src/routes/backends.rs`
- `packages/app-web/src/types/acp.ts`
- `packages/app-web/src/stores/coordinatorStore.ts`
- `packages/core/src/local-runtime/index.ts` only if desktop/local snapshot is explicitly included

清单：
- [ ] Move backend runtime summary response types into generated contract source.
- [ ] Replace frontend hand-written mirror with generated alias or narrow view model.
- [ ] Keep route adapter as the single application projection -> wire DTO mapper.
- [ ] Decide whether desktop local runtime snapshot is included in this iteration; if included, define stable diagnostics DTO before touching Tauri callers.
- [ ] Run `pnpm run contracts:check`, `pnpm --filter app-web typecheck`, and targeted frontend tests if affected.

## Work Group C - Task Tool Local Deepening

建议顺序：第二组；如果与 B 组写入文件不重叠，可以并行。

可能涉及文件：
- `crates/agentdash-application/src/task/tools.rs`
- `crates/agentdash-application/src/task/runtime_tool_provider.rs`
- possible new files under `crates/agentdash-application/src/task/`

清单：
- [ ] Extract `AgentRunTaskScopeResolver` from AgentTool JSON handling.
- [ ] Introduce typed `TaskPlanScope` if current scope type is too coupled to tool input.
- [ ] Extract `TaskPlanWorkspace.read(scope, query)` and/or `TaskPlanWorkspace.apply(scope, changeset)` behind a narrow interface.
- [ ] Keep `task_read` / `task_write` tool schemas stable for the first iteration.
- [ ] Move or add tests so scope/use case behavior can be tested without constructing full AgentTool JSON.
- [ ] Run targeted Rust tests, then `cargo test -p agentdash-application task` if practical.

## Work Group D - NDJSON Validator Exploration

建议顺序：A 组之后，除非出现必须优先处理的 stream bug。

可能涉及文件：
- `crates/agentdash-contracts/src/generate_ts.rs`
- `packages/app-web/src/api/eventStream.ts`
- `packages/app-web/src/features/session/model/streamTransport.ts`
- generated contract files

清单：
- [x] Choose stream pilot scope: Session stream and Project event stream were both included because both generated envelope unions already exist and frontend guard drift was local to current stream consumers.
- [x] Decide validator generation shape: use frontend shared guard helpers plus per-stream generated-union validators; generator-derived metadata/parser is deferred because it is not necessary to remove the current scattered guard paths.
- [x] Add acceptance tests for valid/invalid envelope parsing: Session connected/event/heartbeat/unknown/project-shape rejection; Project Connected/StateChanged/Heartbeat/session-shape rejection/unknown rejection.
- [x] Replace hand-written parser paths in `streamTransport.ts` and `eventStream.ts`; transport files now keep fetch/reader/cursor/lifecycle responsibilities while validator modules own envelope branch validation.

## Work Group E - Quality Gates And Route Shim Follow-up

建议顺序：靠后。

可能涉及文件：
- `package.json`
- `.github/workflows/*.yml`
- `scripts/dev-runtime.js`
- possible new `scripts/lib/quality-gates.js`
- frontend service files only for selected route shim target

清单：
- [ ] Decide whether to start with quality gate manifest or one feature command/query interface.
- [ ] If quality gate: define gate manifest for `full_local`, `pr_quick`, `deployment_contract`, `migration_history`, `desktop_check`.
- [ ] Add small tests for gate membership and command composition.
- [ ] If route shim: choose one feature and move endpoint-string tests down to adapter level.

## Parking Lot

- Desktop local runtime snapshot DTO can be promoted from B into its own work group if it grows beyond backend runtime summary.
- Canvas/Session route shim tests should wait until related feature command/query interface is chosen.
- 第一档 `WorkspaceModuleAgentSurface` 与 `AgentRunWorkspaceControlPlane` 已明确排除在本任务之外。
