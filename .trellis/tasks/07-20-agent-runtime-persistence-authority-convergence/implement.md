# Agent Runtime 持久化职责与事实边界清理实施计划

> Product 已确认不承诺离线异步可靠输入投递，任务已进入实施阶段。各阶段以 production
> tracer bullet 和唯一事实 owner 为门禁，不保留兼容路径、回退路径或双写过渡。

## Phase 0 — 决策与基线冻结

- [x] 确认 Product 输入合同：只支持同步 Agent handoff，不保存离线 pending input。
- [ ] 用 owner/事实/恢复方式矩阵复核所有 Runtime、Host、Callback、Product、Dash 表。
- [ ] 记录当前 production command/read/stream composition 与表引用负向搜索基线。
- [ ] 固定 `target + client_command_id + request digest` 到 stable command/effect identity 合同。
- [ ] 固定 Complete Agent `inspect` 对 Create/Command/Fork/SurfaceApply 的恢复语义。

## Phase 1 — 建立无 durable Runtime projection 的 tracer bullet

- [ ] 将 Dash execution production callbacks 从 Noop 替换为真实 Agent live change sink。
- [x] 保留真实 `DashCoreError` code/message/retryability，写入 Agent terminal history，并经
  Complete Agent authoritative read 暴露同一 terminal evidence。
- [ ] 建立 Complete Agent `read` → in-memory normalize → AgentRun conversation snapshot。
- [ ] 建立 Complete Agent live callback/change → in-memory broadcast → frontend stream。
- [ ] 重连时从 Complete Agent authoritative snapshot/history 恢复，不读取 Runtime state document。
- [ ] 添加 production composition test；漏装 callback/read mapper/broadcaster 时测试必须失败。

## Phase 2 — Command handoff 与 Agent inspect recovery

- [ ] 普通 Submit/Steer/Interrupt/Compaction/Interaction/Close 使用 deterministic effect identity。
- [ ] dispatch 前先 inspect：
  - [ ] Applied/Accepted 返回既有 receipt；
  - [ ] NotApplied 才允许 execute；
  - [ ] Unknown 返回 typed pending/unavailable，不重派。
- [ ] Create 回包丢失后从 inspection 恢复 source coordinate。
- [ ] Fork 回包丢失后从 inspection 恢复 child source/history digest。
- [ ] SurfaceApply 回包丢失后从 inspection 恢复 applied receipt。
- [ ] 删除 Runtime operation/pending/effect settlement 对上述恢复的依赖。

## Phase 3 — Agent Runtime 纯内存切换

- [ ] 将 production `ManagedAgentRuntimeGateway` 改为 Product/Agent 两端之间的内存协调器。
- [ ] `read`/`changes` 改为即时 Complete Agent read/changes 映射。
- [ ] 删除 `CompleteAgentStateReconciler`、durable normalized projection 与 source identity map。
- [ ] 删除 Runtime repository port、PostgreSQL adapter、coordinator durable accept/settle。
- [ ] 删除 Runtime change/outbox、Product change delivery worker 与 per-consumer delivery state。
- [ ] 删除 command availability revision 与 projection stale gate；availability 由当前 Agent/profile
  推导。
- [ ] 保留 command-specific coordinates，不保留 generic expected projection revision。

## Phase 4 — Complete Agent Host 与 Callback 纯内存切换

- [ ] live catalog 只保存本次进程 attachment、service handle、descriptor/offer 与 diagnostic。
- [ ] Host binding、route、lease、generation、surface bound state 改为 process-local。
- [ ] 删除 Host target provisioning/recovery/effect ledger 与 revision repository。
- [ ] 删除 Callback reservation/outcome repository。
- [ ] Tool/Hook callback 改为调用真实 handler owner 的 idempotent receipt。
- [ ] Host restart 后未知旧 route/incarnation 默认拒绝，并通过重新 attach/apply surface 建立新 route。
- [ ] optional Agent materialization 失败只影响该 selection，不终止 API/server。

## Phase 5 — Product owner document 清理

- [ ] AgentRun list/workspace/delete 使用 Product shell + optional Agent presentation。
- [ ] 删除 Runtime projection/source/surface currentness 对 Product query 和 command 的 gate。
- [ ] 将 Complete Agent source association 收回 LifecycleAgent/AgentRun owner document。
- [ ] 将 AgentFrame history/surface 收回 LifecycleAgent owner-local JSONB，并先收口 repository
  为 agent-scoped exact/latest/history access。
- [ ] 删除 Product binding 中 Runtime/Host/Agent evidence 副本。
- [ ] 删除 Product runtime command claim。
- [ ] 删除 pending input、mailbox/background delivery。
- [ ] 删除只为同一 Agent command recovery 建立的 saga/table。
- [ ] Agent 不可用时直接返回 typed unavailable；同一 client id 重试保持 effect 幂等。

## Phase 6 — Dash / concrete Agent store 收敛

- [ ] 确认 `DashAgentRepositoryState` 是单个 Dash source 的 canonical document。
- [ ] 删除 `dash_agent_branch/history/command/effect/change` 机械镜像与 drift verification。
- [ ] 评估并保留 Create 前必要的 Agent-owned `effect_id` lookup ledger。
- [ ] 保证 Dash fork/compaction/restart 从 canonical source document 恢复。
- [ ] external Agent adapter 只保存 source association，不复制 native history。
- [ ] Agent 删除/关闭按 concrete Agent 自己的生命周期处理，不依赖 Product 跨表级联。

## Phase 7 — Migration hard cut

- [ ] 使用实施时下一个可用 migration 序号，forward 删除 Runtime/Host/Callback durable schema。
- [ ] 删除 Runtime→Product delivery、Product duplicate execution state 和 Dash mirrors。
- [ ] 迁移 Product source association/AgentFrame/binding 到 owner-local JSONB。
- [ ] 明确清理已执行 0094 开发库中的旧 intermediate/pending/projection 文档。
- [ ] 更新 schema readiness、retired table list 与 migration history guard。
- [ ] 验证空库完整 migration。
- [ ] 验证当前开发库顺序升级后可直接启动。

## Phase 8 — Contract、前端与规范收敛

- [ ] 更新 Rust/TypeScript generated contracts，移除 Runtime durable revision/change/outbox 语义。
- [ ] 前端 stream/reconnect 只依赖 Agent snapshot/live delta，不推断 Runtime projection terminal。
- [ ] 更新 Runtime kernel、persistence、Host、AgentRun facade、Native adapter 与 cross-layer stream
  specs。
- [ ] 文档解释最终 ownership 与恢复理由，不记录旧补丁/兼容路径。
- [ ] 更新 07-17 task closure，明确其 durable Runtime 决策已被本任务覆盖。

## Crash Matrix

- [ ] Product handoff 保存前崩溃。
- [ ] Agent dispatch 前请求进程崩溃；重试同一 client id 不产生错误 receipt。
- [ ] Agent Accepted 后、API receipt 前崩溃。
- [ ] Create Applied 后、Product source association 前崩溃。
- [ ] Fork Applied 后、Product child graph commit 前崩溃。
- [ ] SurfaceApply Applied 后、Host response 前崩溃。
- [ ] Tool/Hook handler Applied 后、Agent callback response 前崩溃。
- [ ] live delta 中途断开并重连。
- [ ] Agent SnapshotOnly 且没有 durable change tail。
- [ ] Agent unreachable 后恢复。

## Focused Validation

实际测试名随实现确定；至少覆盖：

```powershell
cargo test -p agentdash-agent-service-api
cargo test -p agentdash-agent
cargo test -p agentdash-integration-native-agent
cargo test -p agentdash-agent-runtime
cargo test -p agentdash-agent-runtime-host
cargo test -p agentdash-application-agentrun
cargo test -p agentdash-infrastructure
cargo test -p agentdash-api
cargo check -p agentdash-agent-runtime -p agentdash-agent-runtime-host -p agentdash-infrastructure -p agentdash-api
pnpm typecheck
node scripts/check-migration-history.js
git diff --check
```

遵循项目“小规模迭代不重复无关测试”的约束：各 Phase 使用定向测试，最终检查再运行受影响包的完整
质量门。Windows Cargo 锁与 embedded PostgreSQL data root 竞争按 AGENTS.md 处理。

## Production Composition Gate

最终必须有一条真实纵切同时证明：

```text
user input
  -> Product authorization/association
  -> stable Agent effect
  -> Complete Agent execution
  -> live frontend delta
  -> Agent-owned terminal history
  -> disconnect
  -> reconnect authoritative snapshot
```

该测试还必须断言：

- Product/Runtime/Host 只各写自己的允许事实；
- Runtime/Host 数据库写入为零；
- Agent failure 保留真实诊断；
- projection/cache 缺失不影响 Product list/workspace/delete；
- 同一 client/effect retry 不重复副作用。

## Negative Search Gate

生产代码中以下类别引用必须为零：

- Runtime/Host/Callback revision repository；
- Runtime operation/pending/change/outbox persistence；
- Runtime Product change delivery worker；
- Product command 对 projection revision/currentness 的 gate；
- Host callback reservation/outcome persistence；
- Dash repository JSONB relational mirror verification；
- `NoopDashExecutionCallbacks` 的 production 注入；
- 将真实 Agent error 替换为通用 `execution failure`。

测试 fixture 或 migration 历史中的旧字符串可保留，但必须与 production/search allowlist 明确区分。

## Review Gates

1. 每个 durable field 都能指出唯一领域 owner 和不可重建理由。
2. Product 不保存 Agent 执行事实，Agent 不保存 Product workflow/Frame。
3. Runtime/Host 重启不需要恢复自己的数据库。
4. 每个 post-dispatch unknown 都由 stable effect + actual owner inspect 收敛。
5. derived read model 不参与任何业务写入 gate。
6. live stream 可丢、snapshot 可恢复；两者不伪装成同一 durable tail。
7. migration 从空库和当前开发库收敛到相同 schema。
8. 最终 production composition 只有一条 command/read/stream 路径，无 fallback/dual write。

## Rollback Policy

项目未上线，不建立 runtime rollback 或兼容路径。某个 Slice 未通过时修正该 Slice 和 forward
migration，在隔离数据库重跑；不能恢复 Runtime/Host durable authority 作为临时兜底。
