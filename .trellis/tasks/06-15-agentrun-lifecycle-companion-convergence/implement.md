# Implement Plan

## Phase 0: Pre-implementation Review

- [x] 用户确认本任务的 PRD 与设计方向。
- [x] 决定是否把实现拆为子任务：
  - companion payload contract
  - AgentRun lifecycle VFS helper
  - embedded skill projection
  - frontend resource/capability projection
- [x] 进入实现前运行 `trellis-before-dev`，读取 backend / cross-layer / frontend 相关 spec index。

## Phase 1: Companion Payload Contract

- [x] 修改 `CompanionRequestTool` 执行逻辑：`sub` / `parent` / `human` 都读取 `payload.message`。
- [x] 修改 `PayloadTypeRegistry`：`task`、`review`、`approval`、`notification` 的 request required field 改为 `message`。
- [x] 更新 error message，统一提示 `payload.message 不能为空`。
- [x] 更新 companion request tool schema，展开 registered payload types。
- [x] 更新 companion-system skill docs：
  - `SKILL.md`
  - `references/payload-envelope.md`
  - `references/human-interaction.md`
  - `references/response-adoption.md`
  - `references/capability-grant-request.md`
- [x] 更新相关测试与示例中的 `prompt` 字段。

## Phase 2: AgentRun Lifecycle VFS Helper

- [x] 设计并实现统一 helper，用 anchor + project_id + skill keys 构造 AgentRun lifecycle VFS。
- [x] helper 合并 explicit agent skill keys 与 builtin skill keys。
- [x] helper 保证 `skill_asset_project_id` / `skill_asset_keys` 不因 mount 替换丢失。
- [x] lifecycle provider 支持 AgentRun session-scope mount 下的 `skills/` 与 node subtree。
- [x] 当前 node artifact/record 写入仍使用 `node_runtime` scope；AgentRun session projection 负责展示、anchor node evidence 与 skill metadata carry-over。

## Phase 3: Frame Construction Integration

- [x] ProjectAgent owner bootstrap：无 active workflow 时仍安装 AgentRun lifecycle mount。
- [x] ProjectAgent owner bootstrap：有 collaboration/companion capability 时投影 `companion-system`。
- [x] ProjectAgent owner bootstrap：有 workspace_module capability 时投影 `workspace-module-system`。
- [x] Lifecycle node composer：保留 node artifact/record 写入能力，并让 AgentRun workspace projection 保留 skill metadata。
- [x] Plain companion child composer：在 parent VFS slice 基础上安装 child AgentRun lifecycle mount，并投影 `companion-system`。
- [x] Companion + workflow child composer：叠加 workflow node surface，并投影 `companion-system`。
- [x] Routine frame construction 如涉及 AgentRun surface，同步投影 `routine-memory`。

## Phase 4: Workspace Query And Frontend Projection

- [x] `AgentRunWorkspaceQuery` 不再用无 skill metadata 的 fresh lifecycle mount 覆盖 frame VFS。
- [x] workspace/resource surface 与 connector-visible VFS 使用同一闭包后的 runtime surface。
- [x] 前端 capability/resource surface 继续消费后端 projected runtime surface，本轮无需硬编码 builtin skill。
- [x] 检查 Session/Runtime detail 入口仍能通过 RuntimeSession trace ref 下钻，但不复制 AgentRun command facts。

## Phase 5: Specs And Tests

- [x] 更新 `.trellis/spec/backend/embedded-skill-bundles.md`。
- [x] 更新 `.trellis/spec/backend/session/runtime-execution-state.md`。
- [x] 更新 `.trellis/spec/cross-layer/frontend-backend-contracts.md`。
- [x] 补 backend 单元测试：
  - graphless ProjectAgent frame VFS 包含 AgentRun lifecycle mount + companion-system。
  - workspace_module capability 追加 workspace-module-system。
  - plain companion child 包含 child AgentRun lifecycle mount + companion-system。
  - workspace query 不丢 skill projection metadata。
  - companion payload registry 要求 message。
- [x] 前端测试评估：本轮未新增前端测试；前端继续消费后端 `resource_surface` / capability projection，未引入前端硬编码或新 DTO。

## Validation Commands

按改动范围选择运行：

```powershell
cargo test -p agentdash-application companion::payload_types
cargo test -p agentdash-application workflow::lifecycle::mount
cargo test -p agentdash-application workflow::frame_construction
cargo test -p agentdash-application vfs::provider_lifecycle
pnpm --filter app-web test -- session
```

必要时补充：

```powershell
cargo check -p agentdash-application
cargo check -p agentdash-api
pnpm --filter app-web typecheck
```

## Risk Points

- `lifecycle` mount 同时承担 AgentRun session surface 和 workflow node writable surface，修改时要保证 artifacts/records 写入仍落到正确 run/node/attempt。
- `build_agent_run_lifecycle_vfs` 当前用于 workspace query，修改后要确认展示层和执行层都消费同一 metadata。
- Skill discovery 依赖 VFS provider list/read 行为，metadata 合并错误会导致前端与执行器同时看不到 skill。
- Companion payload 字段切换到 `message` 会影响测试 fixture、skill docs、tool schema、event card 展示。

## Rollback Points

- Companion payload contract 与 lifecycle VFS helper 是两个可独立 review 的变更点。
- 若 AgentRun lifecycle mount subtree 迁移影响较大，可先保证 metadata 合并不丢失，再单独收束 node subtree。
- 若前端展示受影响，优先以后端 frame/runtime surface 为事实源修正 projection，而不是在前端硬编码 builtin skill。
