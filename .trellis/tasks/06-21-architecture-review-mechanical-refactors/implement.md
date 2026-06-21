# 机械化重构执行计划

## Phase 1: Contract Surface

- [ ] M01 Project event NDJSON contract 化。
- [ ] M02 ProjectBackendAccess / BackendWorkspaceInventory contract 化。
- [ ] M03 Canvas CRUD contract 化。
- [ ] M04 SkillAsset HTTP DTO contract 化。
- [ ] M05 ExtensionManagement service 回到 generated DTO。
- [ ] M06 `workspace_module_presented` stream payload contract 化。
- [ ] M07 Auth/current-user/identity-directory DTO contract 化或明确 route-local wrapper。

建议验证：

```bash
pnpm run contracts:check
pnpm run frontend:check
```

## Phase 2: Residual Surface Cleanup

- [ ] M08 拆分 `types/index.ts`。
- [ ] M09 确认 SessionExecutionState 消费面。
- [ ] M10 移除或封装 `AgentRunSteeringService`。
- [ ] M11 清理 AppState 中未公开消费的 `StoryActivityActivationService`。
- [ ] M12 raw anchor repository API 与 application selection API 分层命名。
- [ ] M13 RuntimeGateway `surface_for` debug 入口守卫。

建议验证：

```bash
cargo check
pnpm run frontend:check
rg "AgentRunSteeringService|StoryActivityActivationService|latest_for_agent|surface_for"
```

## Phase 3: Tests / Diagnostics / UI Semantics

- [ ] M14 固化 runtime status aggregation owner tests。
- [ ] M15 top-level `AgentRunWorkspaceView.control_plane` 只作 display status 的测试/注释。
- [ ] M16 WorkspaceModule runtime deps 缺失改为可观测诊断。
- [ ] M17 前端 workspace routing 文案区分 binding availability 与 execution allocatable。
- [ ] M18 Profile UI 把 machine id 表达为只读 runtime fact。
- [ ] M19 extension relay payload 不携带 backend_id 的 regression test。

建议验证：

```bash
cargo test -p agentdash-domain workflow
cargo test -p agentdash-application
pnpm run frontend:check
```

## Dispatch Notes

- 可以用 `trellis-implement` subagent 每次领取 1-3 个同组 item。
- 每个实现 prompt 必须引用对应 `work-items/*.md` 文件和父任务 research 来源。
- 如果执行时发现 item 需要新增事实源或改变控制面语义，停止实现，把该 item 移回父任务 `design-coupling-tracker.md`。

