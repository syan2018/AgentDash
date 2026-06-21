# 机械化重构执行计划

## Phase 1: Contract Surface

- [ ] M01 Project event NDJSON contract 化。
- [ ] M02 ProjectBackendAccess / BackendWorkspaceInventory contract 化。
- [x] M03 Canvas CRUD contract 化。
- [ ] M04 SkillAsset HTTP DTO contract 化。
- [x] M05 ExtensionManagement service 回到 generated DTO。
- [x] M06 `workspace_module_presented` stream payload contract 化。
- [ ] M07 Auth/current-user/identity-directory DTO contract 化或明确 route-local wrapper。

建议验证：

```bash
pnpm run contracts:check
pnpm run frontend:check
```

## Phase 2: Residual Surface Cleanup

- [ ] M08 拆分 `types/index.ts`。
- [x] M09 确认 SessionExecutionState 消费面。
- [x] M10 移除或封装 `AgentRunSteeringService`。
- [x] M11 清理 AppState 中未公开消费的 `StoryActivityActivationService`。
- [x] M12 raw anchor repository API 与 application selection API 分层命名。
- [x] M13 RuntimeGateway `surface_for` debug 入口守卫。

建议验证：

```bash
cargo check
pnpm run frontend:check
rg "AgentRunSteeringService|StoryActivityActivationService|latest_for_agent|surface_for"
```

## Phase 3: Tests / Diagnostics / UI Semantics

- [x] M14 固化 runtime status aggregation owner tests。
- [x] M15 top-level `AgentRunWorkspaceView.control_plane` 只作 display status 的测试/注释。
- [x] M16 WorkspaceModule runtime deps 缺失改为可观测诊断。
- [x] M17 前端 workspace routing 文案区分 binding availability 与 execution allocatable。
- [x] M18 Profile UI 把 machine id 表达为只读 runtime fact。
- [x] M19 extension relay payload 不携带 backend_id 的 regression test。

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

## Round 1 Completion Notes

- M05, M10, M11, M13, M14, M15, M16, M17, M18, M19 已由并行 subagents 完成。
- 第一轮刻意避开 `agentdash-contracts` 与 generated files，避免 Contract Surface 条目之间争用生成入口。
- M01-M04, M06, M07 建议后续按 contract 生成入口单 worker 串行推进；M08, M09, M12 建议在下一轮按前端类型入口、session state、anchor repository 命名分别拆分。

## Round 2 Completion Notes

- M03, M06, M09, M12 已由并行 subagents 完成。
- 第二轮仅指定 M03 为 contract/export owner；M06 复用现有 `WorkspaceModulePresentation` generated DTO，M09 明确 `/sessions/{id}/state` 为 route-local diagnostic wrapper，M12 只做 raw repository API 命名收口。
- 剩余 M01, M02, M04, M07 仍涉及 contract export，建议继续按 1 个 export owner + 若干不碰导出的消费面 worker 分轮推进；M08 可独立做类型入口拆分。
