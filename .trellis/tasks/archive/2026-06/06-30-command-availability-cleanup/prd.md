# Command availability cleanup 收束

## Goal

实现 design backlog Slice 5 / D5：删除或收窄 runtime_command_state 平行控制状态，使 ConversationCommandAvailabilityResolver 成为唯一 command availability owner；前端命令 handler 只依赖 backend command.enabled/stale_guard 作为语义准入，workspaceStatus 仅保留为展示/加载 UX。

## Requirements

- `ConversationCommandAvailabilityResolver` 必须是 AgentRun server-side command availability 的唯一推导 owner。
- Command policy 继续复用 resolver 输出，并只校验 durable precondition、stale guard、command kind/id 与 enabled 状态；不得引入第二套 allow/deny 推导。
- 后端 `AgentRunWorkspaceProjection::runtime_command_state` 及其 model/type 若未进入 public contract，应删除；若发现仍被展示消费，只能改名/收窄为 display-only shell status，不得表达 command authority。
- `AgentRunWorkspaceShellModel.workspace_status` / `delivery_status` 只表达 chrome/list/status display，不参与 mutating command semantic admission。
- 前端 command handlers 的语义准入只能来自 backend `ConversationCommandView.enabled`、`unavailable_reason` 和提交时的 `commandPrecondition(command)`；`workspaceStatus` 可用于 loading UX，但不能因为非 `"ready"` 阻断一个 backend-enabled command。
- Stale command refresh 行为保留：后端返回 `stale_command` 时刷新 workspace projection。
- 清理旧问题优先于添加 feature：不要新增 `CommandAvailabilityService`、前端本地 availability mirror 或平行状态判断。
- Subagent 执行约束：实现 worker 不跑大规模 Rust 编译或 broad suites；允许 scoped `rg`、format、小型定向 Rust/TS tests/typecheck。

## Acceptance Criteria

- [x] `runtime_command_state` 不再出现在用户命令路径；若彻底删除可行，则代码库只允许历史文档/设计 review 中出现该旧名。
- [x] 后端 workspace projection tests 不再断言 `runtime_command_state`，而继续覆盖 `state_code`、`delivery_status`、turn ids 的 display projection。
- [x] Command policy tests 仍覆盖 stale guard mismatch、disabled command、terminal/invalid command rejection，并使用 resolver 输出。
- [x] Frontend `useAgentRunWorkspaceCommands` 不再以 `workspaceStatus !== "ready"` 作为 command submission/cancel/mailbox semantic gate。
- [x] Frontend command requests 仍提交 `commandPrecondition(command)`，并对 `stale_command` refresh。
- [x] Specs 更新记录 command availability owner 与 shell display status 的分工。

## Notes

- Source: `.trellis/tasks/06-30-design-backlog-review/design-review.md#d5-command-availability-resolver--policy`.
- This is Slice 5 from `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md`.
