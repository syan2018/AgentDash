# 当前 Agent Runtime 架构与持久化边界审计

## Audit Context

- 日期：2026-07-20
- 基线提交：`098b7010d refactor(agent-runtime): 收敛运行时事实与产品持久化边界`
- 重点回溯提交：`a535ae0165ea8105ebbc9339652d3c6fe9b5980c`
- 相关任务：
  - `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/`
  - `.trellis/tasks/archive/2026-07/07-12-canonical-runtime-session-presentation-convergence/`
  - `.trellis/tasks/archive/2026-07/07-10-agent-runtime-architecture-convergence/`
  - `.trellis/tasks/07-20-database-persistence-boundary-cleanup/`

本文件记录任务级证据和当前实现缺陷，不作为长期 architecture spec。

## 1. 当前 durable state 分布

### Product

- `agent_run_product_runtime_binding`
- `agent_run_product_runtime_command_claim`
- launch/recovery/fork/companion/workflow sagas
- Product mailbox、presentation 与 terminal projections
- LifecycleRun/LifecycleAgent/AgentFrame/workflow/lineage

Product command facade 在 dispatch 前持久化完整 Runtime envelope，并为 lost response 提供
`replay_claimed`：

- `crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:19`
- `crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:149`
- `crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:265`

### Managed Runtime

`ManagedRuntimeFacts` 同一文档包含：

- projection；
- binding；
- source projection / identity map / changes；
- operations / idempotency / pending commands；
- platform changes / outbox。

证据：

- `crates/agentdash-agent-runtime/src/managed_runtime.rs:91`
- `crates/agentdash-agent-runtime/src/gateway.rs:800`

Gateway `read`/`changes` 直接读取 persisted Runtime facts，而不是 Complete Agent：

- `crates/agentdash-agent-runtime/src/gateway.rs:833`
- `crates/agentdash-agent-runtime/src/gateway.rs:847`

### Complete Agent Host / Callback

Host facts 保存 binding/source/effect/lease/generation/runtime target/provisioning/recovery：

- `crates/agentdash-agent-runtime-host/src/complete_agent_repository.rs:110`

Callback facts 保存 reservation/outcome：

- `crates/agentdash-agent-runtime-host/src/complete_agent_callbacks.rs:174`
- `crates/agentdash-agent-runtime-host/src/complete_agent_callbacks.rs:233`

### concrete Agent / Dash

Complete Agent 合同明确 Agent 自己是 history/context/fork/native lifecycle authority：

- `crates/agentdash-agent-service-api/src/service.rs:166`

并提供 effect inspect：

- `crates/agentdash-agent-service-api/src/service.rs:194`
- `crates/agentdash-agent-service-api/src/command.rs:253`

Dash 额外保存：

- `dash_agent_session.repository` 完整 JSONB；
- `dash_agent_branch/history/command/effect/change` 镜像；
- `dash_complete_source`；
- `dash_complete_effect`。

每次 load 都把 repository JSONB 与所有镜像逐项比较：

- `crates/agentdash-infrastructure/src/persistence/postgres/dash_complete_agent_store.rs:412`
- `crates/agentdash-infrastructure/src/persistence/postgres/dash_complete_agent_store.rs:441`

## 2. Triple-ledger diagnosis

同一用户输入当前经历：

```text
Product command claim
  -> Runtime operation + pending + change + outbox
  -> Host effect + lease/generation
  -> Dash/Complete Agent command + effect + history
```

四层都能因自己的 revision/digest/state 发生 conflict。只有 concrete Agent 真正执行 provider/tool
副作用，但中间三层都在建立自己的 “accepted/running/settled/recovery” 账本。

这不是合理的多 aggregate saga：Runtime `execute` 并未只接受后异步返回，而是 durable accept 后
立即 dispatch 并等待 lifecycle/Agent receipt。Product 已能重放稳定 envelope，Agent 已能 inspect
稳定 effect，因此 Runtime/Host durable ledger 没有提供额外不可替代保证。

## 3. Message swallowed evidence

实际开发数据库中，两次用户输入均已进入：

- Product command claim；
- Dash `InputAccepted` history；
- Dash `TurnStarted` history；
- Dash terminal failed history/effect。

但对应 Runtime state：

- `source_projection = null`
- `source_changes = []`
- normalized turns/items/conversation_history 为空
- Product conversation projection 没有可消费内容

### 3.1 Agent → Runtime 未接线

`CompleteAgentStateReconciler::synchronize_source` 存在：

- `crates/agentdash-agent-runtime/src/complete_agent_state.rs:244`

生产代码没有调用者；搜索结果只有模块内测试和：

- `crates/agentdash-agent-runtime-host/tests/complete_agent_target.rs`

该 reconciler 还要求调用者预先提供 `CompleteAgentRuntimeIdentityMap`，生产 composition 没有
allocator/binder，说明不是单纯漏调一个函数，而是组合设计未完成。

### 3.2 Live delta 被 Noop sink 丢弃

Dash Bridge provider/Core 产生 typed delta，但 production dependencies 注入：

- `callbacks: Arc::new(NoopDashExecutionCallbacks)`
- `emit` 永远 `Ok(())`

证据：

- `crates/agentdash-integration-native-agent/src/bridge_execution.rs:183`
- `crates/agentdash-integration-native-agent/src/bridge_execution.rs:205`

### 3.3 真实执行错误被抹去

Dash 捕获 `DashCoreError` 后只保留 retryability/lost，`finish_failed_turn` 写入：

- `retryable execution failure`
- `execution failure`

证据：

- `crates/agentdash-agent/src/dash/service.rs:648`
- `crates/agentdash-agent/src/dash/service.rs:1335`
- `crates/agentdash-agent/src/dash/service.rs:1377`

因此 provider/Core 根因在 Agent 边界被永久丢失。

### 3.4 Runtime → Product worker 依赖已删除字段

`0094_runtime_evidence_single_writer.sql` 删除 Product source evidence，并声明 Runtime sole owner：

- `crates/agentdash-infrastructure/migrations/0094_runtime_evidence_single_writer.sql:1`

但 Product change claim SQL 仍过滤：

```sql
WHERE product_binding.binding
  -> 'source_binding' ->> 'activated_at_revision' IS NOT NULL
```

证据：

- `crates/agentdash-infrastructure/src/managed_runtime_product_change_delivery.rs:117`

结果是 worker 静默 claim 0 条，而不是显式失败。

## 4. State amplification

当前 live database 样本：

| Document/table | Rows | Approx JSON bytes |
| --- | ---: | ---: |
| `agent_run_product_runtime_binding` | 1 | 1,184 |
| `agent_runtime_callback_revision` | 1 | 29 |
| `agent_runtime_host_revision` | 1 | 319,491 |
| `agent_runtime_state_revision` | 6 | 62,324 |
| `dash_agent_history` | 9 | 4,547 |
| `dash_agent_session` | 5 | 50,601 |
| `dash_complete_effect` | 13 | 34,401 |

目标 Runtime thread 只有 4 个 operation，却有：

- 148 changes；
- 148 outbox entries；
- 0 source changes。

Runtime 单测明确证明一次 command acceptance 写 12 changes 与 12 outbox entries：

- `crates/agentdash-agent-runtime/src/managed_runtime.rs:1872`

原因是每次 operation transition 都为全部 command kinds 重新发 availability change，并把完整
change 再复制进同一 facts.outbox。

## 5. Derived projection becomes authority

Product projection gateway 将 Runtime source/applied surface 作为 currentness fence：

- `crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs:201`
- `crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs:309`

Workspace query 将 stale observation 提升为整个页面 conflict：

- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:320`

AgentRun list 近期已改为 weak presentation read 并在失败时忽略 Runtime summary：

- `crates/agentdash-application/src/agent_run_list.rs:219`

这证明 Product shell 与 optional Agent presentation 可以自然分离，List 不需要 projection
currentness。

## 6. 07-17 design conflict

07-17 ownership matrix规定：

- Runtime operation/idempotency owner：
  `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/design.md:195`
- Runtime pending delivery owner：
  `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/design.md:196`
- normalized conversation owner：
  `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/design.md:204`
- platform change tail/outbox owner：
  `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/design.md:205`
- Runtime 始终提供 durable platform tail：
  `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/design.md:384`

同一设计又要求 Complete Agent authoritative snapshot/effect inspect，Product W7 仍建立 restart-safe
sagas。结果是 Product、Runtime、Agent 三个 durable workflow owner 同时存在。

## 7. a535 hard-cut impact

提交：

```text
a535ae016 refactor(agent-runtime): 收口最终Host与Product API组合
```

变更规模：

- 22 files；
- 1,123 insertions；
- 7,161 deletions；
- `lifecycle_agents.rs` 约删除 4,890 行；
- 删除旧 Runtime/session/journal routes 与 workers；
- 新增 Relay terminal projection；
- AppState 改为 CompleteAgent/Product 组合根。

该提交没有创建全部错误概念，但把生产路径硬切到 07-17 的 durable Runtime 设计，并删除旧工作路径；
Agent source reconciler、live callback 和 reconnect read 尚未完成，因此暴露“纸面模块存在、生产纵切
不存在”的问题。

## 8. Failure-window evaluation

| Window | Existing authoritative recovery | Runtime/Host DB necessity |
| --- | --- | --- |
| normal command response lost | stable effect + Agent inspect | 无 |
| Create source response lost | Applied Create inspection returns source | 无 |
| Fork child response lost | Applied Fork inspection returns child | 无 |
| SurfaceApply response lost | Applied surface inspection | 无 |
| remote Agent unreachable | Product intent remains; retry/inspect later | 无 |
| live stream reconnect | Agent authoritative snapshot | 无 |
| old callback after Host restart | unknown route/incarnation reject | 无 |
| Tool/Hook result response lost | handler-owned idempotency receipt | Host DB 无 |

Product 已明确不承诺离线异步投递。API 只有在 Agent 明确接收后才报告成功；Agent 不可用时返回
typed unavailable，调用者使用相同 client id 稳定重试。因此 Product command claim、pending
input、mailbox delivery 与后台补投递均没有保留理由。

## 9. Corrections and non-evidence

- `agent_run_terminal_projection` 是 PTY/Relay terminal resource，不是 conversation history；
  terminal 表为空不能证明 Agent conversation projection 失败。
- `agent_runtime_callback_revision` 是 reverse Tool/Hook callback store，不是 Runtime→Product
  change registry；它为空不是消息丢失的直接证据。
- JSONB 适合作为 owner document 不等于跨 bounded context 合并。Dash source history 不能进入
  Product AgentRun JSONB。
- 表数量不是根因；没有唯一 owner、可重建 projection 参与 gate、相同 effect 多层建账才是根因。

## 10. Audit conclusion

在现有 Complete Agent `read/changes/inspect` 合同下，没有发现 Runtime/Host durable state 中
无法从 Product intent 和 concrete Agent authority 恢复的事实。

推荐目标：

- Product：business documents + source association；输入只存在于同步 API handoff；
- Runtime/Host：pure memory；
- Complete Agent：native source/history/effect authority；
- UI：Product shell + Agent read + live in-memory delta；
- PostgreSQL：按真实 owner 保存 JSONB documents，不保存中间协调器状态或机械关系镜像。
