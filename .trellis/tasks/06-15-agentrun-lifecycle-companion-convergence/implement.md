# Implement Plan

## Phase 0: Pre-implementation Review

- [ ] 用户确认本任务的 PRD 与设计方向。
- [ ] 决定是否把实现拆为子任务：
  - companion payload contract
  - AgentRun lifecycle VFS helper
  - embedded skill projection
  - frontend resource/capability projection
- [ ] 进入实现前运行 `trellis-before-dev`，读取 backend / cross-layer / frontend 相关 spec index。

## Phase 1: Companion Payload Contract

- [ ] 修改 `CompanionRequestTool` 执行逻辑：`sub` / `parent` / `human` 都读取 `payload.message`。
- [ ] 修改 `PayloadTypeRegistry`：`task`、`review`、`approval`、`notification` 的 request required field 改为 `message`。
- [ ] 更新 error message，统一提示 `payload.message 不能为空`。
- [ ] 更新 companion request tool schema，展开 registered payload types。
- [ ] 更新 companion-system skill docs：
  - `SKILL.md`
  - `references/payload-envelope.md`
  - `references/human-interaction.md`
  - `references/response-adoption.md`
  - `references/capability-grant-request.md`
- [ ] 更新相关测试与示例中的 `prompt` 字段。

## Phase 2: AgentRun Lifecycle VFS Helper

- [ ] 设计并实现统一 helper，用 anchor + project_id + skill keys 构造 AgentRun lifecycle VFS。
- [ ] helper 合并 explicit agent skill keys 与 builtin skill keys。
- [ ] helper 保证 `skill_asset_project_id` / `skill_asset_keys` 不因 mount 替换丢失。
- [ ] lifecycle provider 支持 AgentRun session-scope mount 下的 `skills/` 与 node subtree。
- [ ] 若当前 node artifact/record 仍依赖 `node_runtime` scope，迁移到 AgentRun session-scope subtree 或明确桥接。

## Phase 3: Frame Construction Integration

- [ ] ProjectAgent owner bootstrap：无 active workflow 时仍安装 AgentRun lifecycle mount。
- [ ] ProjectAgent owner bootstrap：有 collaboration/companion capability 时投影 `companion-system`。
- [ ] ProjectAgent owner bootstrap：有 workspace_module capability 时投影 `workspace-module-system`。
- [ ] Lifecycle node composer：使用统一 AgentRun lifecycle helper，保留 node artifact/record 能力。
- [ ] Plain companion child composer：在 parent VFS slice 基础上安装 child AgentRun lifecycle mount，并投影 `companion-system`。
- [ ] Companion + workflow child composer：与 plain companion 使用同一 lifecycle helper，再叠加 workflow node surface。
- [ ] Routine frame construction 如涉及 AgentRun surface，同步投影 `routine-memory`。

## Phase 4: Workspace Query And Frontend Projection

- [ ] `AgentRunWorkspaceQuery` 不再用无 skill metadata 的 fresh lifecycle mount 覆盖 frame VFS。
- [ ] workspace/resource surface 与 connector-visible VFS 使用同一闭包后的 runtime surface。
- [ ] 前端 capability/resource surface 测试覆盖 projected builtin skill 可见性。
- [ ] 检查 Session/Runtime detail 入口仍能通过 RuntimeSession trace ref 下钻，但不复制 AgentRun command facts。

## Phase 5: Specs And Tests

- [ ] 更新 `.trellis/spec/backend/embedded-skill-bundles.md`。
- [ ] 更新 `.trellis/spec/backend/session/runtime-execution-state.md`。
- [ ] 更新 `.trellis/spec/cross-layer/frontend-backend-contracts.md`。
- [ ] 补 backend 单元测试：
  - graphless ProjectAgent frame VFS 包含 AgentRun lifecycle mount + companion-system。
  - workspace_module capability 追加 workspace-module-system。
  - plain companion child 包含 child AgentRun lifecycle mount + companion-system。
  - workspace query 不丢 skill projection metadata。
  - companion payload registry 要求 message。
- [ ] 补 frontend 测试：
  - capability card 展示 projected builtin skill。
  - resource surface 可浏览 lifecycle skills。

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
