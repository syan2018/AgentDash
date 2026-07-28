# Agent Runtime View 单一状态链路实施计划

## Scope Estimate

- 预计修改 55–70 个文件。
- 行为关键文件约 20–30 个；其余为生成 contract、fixtures、测试和规范。
- 涉及 Rust contract/service/application/API、TypeScript connection/state/UI、schema 生成与
  Trellis spec。
- 风险等级：高。原因是 live observation、command availability 和 Workspace composer contract
  需要原子切换。
- 不拆并行子任务：各 slice 共享同一生成 contract，拆分会产生大面积冲突和不可编译中间态。

## Ordered Implementation

### Slice 1 — Contract vocabulary and explicit control view

- [ ] 运行 `trellis-before-dev`，加载 backend/frontend/package specs。
- [ ] 将 application-facing `ManagedRuntime*` 词汇硬切为 `AgentRuntime*`。
- [ ] 将 `ManagedRuntimeSnapshot` 改为 `AgentRuntimeView`。
- [ ] 增加显式 `AgentRuntimeExecutionView`，不再要求调用方扫描 conversation。
- [ ] 定义 `AgentRuntimeUpdate`，区分 connection-local lane sequence 与 view revision。
- [ ] 更新 Rust schema/TS generator source；暂不手改生成物。
- [ ] 为 view/update roundtrip、execution 和 availability 增加 contract tests。

### Slice 2 — Runtime observation/update production path

- [ ] 检查 Complete Agent live seam，建立 state + presentation 原子 observation envelope。
- [ ] Codex、Native、Remote adapter 通过同一 seam 输出 observation；不让 API 推断
  `turn_started`。
- [ ] AgentRun Product projection gateway 将 hidden Complete Agent observation normalize 为
  `AgentRuntimeUpdate`。
- [ ] 将 API 从 `/runtime/snapshot`、`/runtime/live` 硬切到 `/runtime/view`、
  `/runtime/updates`。
- [ ] 保持 reconnect/gap → authoritative view reload；删除 presentation-only API contract。
- [ ] 增加 backend tracer，覆盖 idle → running → interrupting/terminal 的 view/update 顺序。

### Slice 3 — Workspace contract owner split

- [ ] 从 Workspace conversation response 删除 execution、active turn、dynamic command enabled
  和 Runtime stale guard。
- [ ] 保留 Product composer support，并将 UI command 显式绑定到
  `AgentRuntimeCommandKind`。
- [ ] Workspace query 不再把 Runtime view还原成 `AgentObservation` 后重复派生 execution。
- [ ] 保持 Product shell、Frame、resource surface、waiting items、model config、ownership、
  keyboard/placement 和 diagnostics。
- [ ] 增加 Workspace contract tests，证明 Agent unavailable/refreshing 时 Product facts仍成立，
  且 response 不复制 Runtime control。

### Slice 4 — Frontend AgentRuntimeConnection

- [ ] 将 `managedRuntimeFeedConnection` 重塑为 `AgentRuntimeConnection`。
- [ ] 将 `useManagedRuntimeFeed` 替换为单一 connection owner/provider。
- [ ] 实现 view baseline、update lane、target fence、reconnect/gap reload、terminal convergence。
- [ ] 提供 conversation/control/interaction/connection selectors。
- [ ] `useSessionStream` 只消费 conversation selector，不自行建立 connection 或合成控制
  `event_seq`。
- [ ] 删除 `SessionChatView.lastLiveEventSeqRef` 对 Runtime control 的副作用派发。
- [ ] Product/Task/title typed invalidation改用稳定 update lane/presentation identity。

### Slice 5 — Composer and command cutover

- [ ] Composer execution、active turn、submit/interrupt availability只读 control selector。
- [ ] Product composer support 与 Runtime availability按声明式 command binding组合。
- [ ] submit/interrupt 统一调用 connection command interface。
- [ ] 删除 `TurnStarted/TurnCompleted -> refreshWorkspaceState` 控制链。
- [ ] 保持 Workspace Module、Task、title 等各自 typed owner invalidation。
- [ ] 增加“history 收缩后下一轮开始仍显示停止按钮”的回归测试。
- [ ] 增加 Workspace refresh/error 不改变运行中停止按钮的测试。

### Slice 6 — Generation, specs and cleanup

- [ ] 重新生成 Rust schema、TypeScript contracts/validators。
- [ ] 删除旧 Managed Runtime/Runtime Session 名称、旧 route 和旧 feed实现。
- [ ] 修正 `CONTEXT.md`、`project-overview.md`、frontend state-management 与 Agent Runtime facade
  specs，记录最终 owner理由。
- [ ] 负向搜索旧 `ManagedRuntimeSnapshot`、`useManagedRuntimeFeed`、
  `lastLiveEventSeqRef` 控制链和 Workspace execution副本。
- [ ] 检查是否实际涉及数据库 schema；无变化则明确记录无需 migration。

## Validation

按风险定向验证，避免无意义重复：

```powershell
pnpm --filter app-web typecheck
pnpm --filter app-web lint
pnpm --filter app-web test -- <AgentRuntimeConnection/SessionChat/Workspace 定向测试>
cargo test -p agentdash-agent-runtime-contract
cargo test -p agentdash-agent-runtime
cargo test -p agentdash-application-agentrun <定向测试名>
cargo test -p agentdash-api <定向 runtime route 测试名>
git diff --check
```

完成代码后使用 `trellis-check` 做 spec、跨层数据流和定向质量检查。真实 Desktop 回归只执行一次
高价值场景：

1. 启动长历史 AgentRun；
2. 连续完成一轮，使 terminal convergence收缩 history；
3. 开始下一轮；
4. 断言 Composer 立即显示停止按钮；
5. 点击停止并断言回合进入中断终态。

## Risk Fences

- 不修改或清理工作区中其他会话的改动。
- 不恢复 Runtime durable repository/change ledger。
- 不保留旧 route/type alias/fallback。
- 不用 presentation event type推断 control。
- 不手改生成 contract；只改 generator source并统一生成。
- Rust命令若等待 Cargo锁，先观察 rust-analyzer/cargo进程，不强制终止。
- 如果 contract hard cut需要数据库字段变化，先增加新的 forward migration，再继续调用方切换。

