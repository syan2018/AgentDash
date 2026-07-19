# Agent Runtime S5 Hard Cut 最终清单

本清单以
[`final-convergence-closeout.md`](./final-convergence-closeout.md) 为执行依据。S5
只删除本任务已经
正确替代并通过真实 consumer/tracer 证明的旧实现。

## 当前真实状态

- [x] C0：工作树干净，Product behavior oracle 与 capability inventory 已固定。
- [x] Product 控制面 oracle 固定为 `58c537b7`（`c3cc58b9^`）。
- [x] Complete Agent / Managed Runtime / Host / Dash/Core 的已验证基础保留。
- [x] canonical App Server protocol owner、source projector、Runtime carrier 与前端
  reducer/renderer 已恢复。
- [x] Lifecycle canonical history provider 已实现并注册到 VFS kernel。
- [x] VFS surface route/resolver 已接 Product binding 与 AppliedResourceSurface。
- [ ] S4 Product Lane Ready：尚未通过。
- [ ] 正式 S5 deletion manifest：尚未形成。

## C1 — Product Integrity

### Application modules

- [ ] 恢复并挂载 `companion`。
- [ ] 恢复并挂载 `frame_construction`。
- [ ] 恢复并挂载 `routine`。
- [ ] 重新挂载仍在源码树中的 `canvas`、`capability`、`runtime_tools`、
  `gate_wait_policy`、`wait_activity`。
- [ ] 恢复旧 Hook presets 所承载的 Product effects inventory。

### API routes

- [ ] 恢复 Companion gate routes。
- [ ] 恢复 Routine public/secured routes。
- [ ] 恢复 Canvas routes。
- [ ] 恢复 Workspace Module routes。
- [ ] 恢复 Terminal routes。
- [ ] 保持并验证 VFS surface routes。
- [ ] 恢复 AgentRun workspace/runtime trace 读取 routes。

### AppState / production composition

- [ ] 恢复 Companion model preflight。
- [ ] 恢复 collaboration tool contribution。
- [ ] 恢复 Companion coordinator/worker、parent mailbox delivery、gate wake、
  adoption/result。
- [ ] 恢复 Routine executor 与 trigger composition。
- [ ] 恢复 Wait service/provider 与 terminal convergence。
- [ ] 恢复 Workspace Module、Canvas、Terminal control/presentation composition。
- [ ] 恢复 Capability/Runtime Tool catalog contributions。

### Product behavior tests

- [ ] 从 oracle 恢复 Companion、Frame Construction、Routine tests。
- [ ] 从 oracle 恢复 AgentRun project start/delete/fork/message/workspace/mailbox tests。
- [ ] 恢复 API route 与 AppState composition tracer tests。

## C2 — Final Seam Wiring

- [ ] AgentRun create/input/control 只调用 Runtime Contract。
- [ ] Companion Full 只调用 exact Runtime / Complete Agent Fork。
- [ ] Companion fresh 只调用 Create + `InitialAgentContextPackage`，随后独立
  `SubmitInput`。
- [ ] Companion/channel/gate/adoption/result 只写 Product repositories。
- [ ] Dash collaboration tool 经 Tool Broker 调 Product Companion command。
- [ ] Routine / Workflow AgentCall 经 AgentRun Product command 调 Runtime。
- [ ] Capability/Runtime Tools 编译为 Runtime Surface / Tool Broker contributions。
- [ ] Hook Product effects 迁到 typed Product command/callback owner。
- [ ] Workspace/Canvas/VFS grants 只读 AppliedResourceSurface。
- [ ] Lifecycle VFS mount 进入 AgentRun AppliedResourceSurface materialization。
- [ ] Terminal control与展示只读写 Product terminal projection/control owner。
- [ ] AgentRun workspace/runtime trace 读取 canonical Product/Runtime projection。
- [ ] 所有 conversation presentation 只使用 canonical App Server records。
- [ ] Product 代码只依赖 Runtime Contract、Product repositories、AppliedResourceSurface
  与 canonical conversation protocol。

## C3/C4 — Product parity tracer

- [ ] Project Agent / AgentRun create、resume、delete。
- [ ] 普通 input → Complete Agent → canonical Turn/Item/output → UI。
- [ ] Native exact fork 与 Codex native fork。
- [ ] Companion Full exact fork。
- [ ] Companion Compact / WorkflowOnly / ConstraintsOnly fresh create。
- [ ] Companion channel、gate、adoption、result、mailbox。
- [ ] Dash collaboration tool spawn/read/wait/result。
- [ ] Workflow AgentCall。
- [ ] Routine trigger → AgentRun → terminal。
- [ ] Workspace Module read/write/presentation。
- [ ] Canvas read/write/promotion/diagnostics。
- [ ] VFS surface read/list/search。
- [ ] Lifecycle VFS canonical `events.json` 与 derived indexes。
- [ ] Terminal create/input/resize/close/projection。
- [ ] Wait activity 与 gate/terminal convergence。
- [ ] Tool/Hook callback、permission、deadline、effect correlation。
- [ ] Compaction Dash exact / Codex native projection。
- [ ] reconnect cursor tail与gap snapshot reload。
- [ ] restart/unknown outcome/recovery 使用同一 command/effect/child identity。

## C5 — Final Hard Cut

Application/Product 领域不属于 Hard Cut。Companion、Frame、Routine、Workflow、
Workspace、Canvas、Terminal、Wait、Lifecycle 只迁移 Runtime 接入 seam；其业务规则、
route、worker、权限、gate、mailbox 与用户可见行为必须保持。移除 module export、
route mount、AppState composition 或 Product caller 不能证明旧 Runtime 已被替代。

每个候选项必须填写：

```text
Legacy:
Target replacement:
Production callers:
Composition:
Repository/schema:
Projection/consumer:
Behavior tracer:
Negative evidence:
```

候选范围：

- [ ] platform `RuntimeSession*` delivery/live/capability/DTO/event。
- [ ] universal `RuntimeJournalFact` / journal persistence/readers。
- [ ] 已被 Complete Agent Host 替代的 connector/driver/executor。
- [ ] 已被 Tool Broker / AgentHostCallbacks 替代的 Hook execution owner。
- [ ] `agentdash-agent-types` 中已迁到最终 owner 的类型。
- [ ] protocol 中 Backbone platform/product、Runtime internal、journal carrier。
- [ ] Relay Prompt/SessionEvent legacy variants。
- [ ] 无消费者的 SPI Agent delegate/re-export。
- [ ] 旧 schema tables/fields/indexes。

## 最终门禁

- [ ] final migration、repositories 和 production composition 使用同一 schema。
- [ ] canonical Rust/TypeScript protocol roots、schema lock、freshness 与 parity 通过。
- [ ] `cargo metadata` 符合最终 crate DAG。
- [ ] 旧 owner negative search 只剩 migration 删除语句或历史任务文档。
- [ ] Rust affected crates/tests 通过。
- [ ] PostgreSQL behavior、CAS、outbox、recovery tests 通过。
- [ ] frontend typecheck、session tests 与 Product feature tests 通过。
- [ ] 一条真实 production tracer 覆盖：

```text
Product command
  -> Managed Runtime operation/change
  -> Host placement/effect
  -> Complete Agent
  -> Agent-owned history
  -> canonical conversation
  -> Product API/UI/VFS consumer
```
