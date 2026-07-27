# 后端三路审计交叉收敛

## 目的

本文件把业务资产、控制编排、执行与系统装配三份独立研究合并为一个 planning 级事实清单，主要解决：

- 同一根因被三份报告重复定级，后续却拆成三个 facade/task；
- 一个业务入口在某一研究面被当作“已转交”，最终无人负责；
- 为修执行链局部症状而新建第二个 Project/Workflow/Runtime owner。

它不替代批准后生成的 `business-coupling-matrix.md` 和 `convergence-plan.md`。

## 覆盖结论

| 覆盖族 | 交叉核验结果 | 主要研究 |
| --- | --- | --- |
| HTTP | `routes/` 38 个含 route 的模块均已按 capability 分类；2 个 helper 模块单列 | 三路共同 |
| MCP | 4 条 transport route、relay/story/workflow 共 19 个工具全部核验 | 控制编排 + 执行装配，业务资产复核 mutation |
| Runtime tools | 16 个 platform descriptor、19 个 platform MCP descriptor及 dynamic MCP 路径均已核验 | 控制编排 + 执行装配 |
| Local Relay | 29 类 production command 全部分类；另发现 2 个 protocol variant 没有 producer/handler | 执行装配 |
| Tauri | `invoke_handler!` 的 25 个 command 全部分类 | 执行装配 + 前端跨层 |
| Background/startup | auth cleanup、Workflow recovery、Routine cron、Interaction/Gate缺失worker、Desktop runner/sidecar均已核验 | 控制编排 + 执行装配 |
| Stream/WS | Project NDJSON、Agent live、Backend WS、terminal notification均已核验 | 执行装配 + 前端跨层 |
| Persistence | 57 张表已按六个 fact family 分类；高风险写链追到具体 transaction/CAS/FK/object owner | 三路共同 |
| Durable object / Local resource | Extension archive、Local cache/Host、PTY/shell、dynamic MCP client 的生命周期均已核验 | 业务资产 + 执行装配 |

当前没有未分类的 production capability。仍存在的是明确的 evidence/product-decision gap：

- Identity Directory 对普通登录用户的企业目录可见性；
- Permission 最终由 Product durable facade 还是 Runtime approval 持有；
- Project retirement 后 concrete Agent source/effect 的 hard-delete 或 retention 合同；
- `/api/health` 是否被仓库外部署当作完整 readiness；
- AgentRun partial launch 是否所有入口都保证 caller retry。

这些缺口不会阻止其它收敛工作包，但对应节点在作语义选择前不能标记完成。

## 合并后的 P0

### P0-A：公开诊断与无 scope setup 暴露

- public `/api/diagnostics` 返回未经字段级脱敏的 arbitrary tracing fields；
- `/workspaces/detect-git` 虽要求登录，却使用 `user_id=None/project_id=None` 接受裸 backend/root；
- 两者的共同根因是“transport authentication 被当作业务 admission”。

拆成两个实施包是合理的，因为一个属于 operational data，一个属于 workspace capability；但二者共享
`entrypoint → actor/scope/authorization owner` architecture gate。

### P0-B：LifecycleRun 写 authority 与 Task policy 分裂

- executor 正确使用 revision CAS；
- terminal、Task、Hook log 等 writer 使用无 revision broad update；
- HTTP/MCP/Runtime/Companion Task 入口的外围 policy不同，HTTP `Use` 还允许 template-visible actor；
- `TaskLockMap` 没有 production caller，且 process-local/task-local lock无法解决整个 run snapshot竞争。

唯一前置包是 `LifecycleRunCommandStore`：typed mutation + stable command identity + CAS retry。Task
authorization、多入口统一是其后继；不能再造一个 Task repository/facade掩盖 broad update。

### P0-C：Project grant last-owner invariant

route 中 check-then-upsert/delete 可以被两个 owner 并发击穿。唯一修复边界是数据库事务内的
`ProjectGrantCommandPort`，而不是 route lock 或更强查询。

### P0-D：Project/Story/Asset retirement 与 object/resource lifecycle

以下不是四个互不相关的 delete bug：

- Project 跨 repository 分步删除；
- Story 先删 record 后 append event，并遗漏 inline；
- ProjectAgent/VFS Mount 先删 inline 再删 owner；
- Project 删除会级联 Product binding/terminal projection，却不先关闭 Host route、dynamic tool lease、
  concrete Agent source/effect；
- Extension metadata、durable archive、Local cache、Host activation没有共同 revoke coordinate。

父包必须是 Project/aggregate semantic retirement。数据库 owner、durable object、AgentRun retirement、
Local derived lifecycle分别实现窄 adapter，但不能各自启动第二套 Project delete orchestration。

### P0-E：资产 install/publish/upload 半提交

Agent+MCP dependency、Mount+inline、Extension publish/upload/install 都缺少 semantic operation
transaction/receipt。Canvas promotion消费同一 artifact operation port；它不是独立 storage saga。

### P0-F：Lifecycle dispatch 非原子

Run、Agent、Frame、association、lineage/gate、node claim 跨多个 repository按顺序提交。目标是一个
`LifecycleDispatchCommandPort` 原子提交 Product facts与outbox；Runtime launch/input只消费 committed
intent。

### P0-G：Routine occurrence 无 durable identity

每个 API 实例都有 cron loop，同一计划时刻可创建多个随机 execution。先建立
`RoutineTriggerOccurrence` unique identity + distributed claim，再修 phase/recovery/admission；在旧
scheduler 上只补 executor retry不成立。

### P0-H：已有 durable effect/convergence 实现未进入 production

- Interaction command transaction已原子写 effect intent，但 dispatcher没有 production worker；
- Gate producer terminal convergence已有service，却没有 terminal projector/replay worker；
- Companion HTTP respond用未注入 delivery 的service，先resolve gate后返回delivery失败。

三项可并行修，但都必须进入统一 production reachability/composition manifest；“代码与单测存在”不能
再算实现完成。

### P0-I：Dynamic Runtime tool executor/credential lease 失真

同一 tool definition 重绑时 Broker 丢弃新 executor，旧 MCP header/endpoint/backend anchor继续有效；
definitions变化又直接拒绝 rebind，且没有 unbind/revoke。目标 identity 必须包含
runtime thread + binding generation + applied surface digest，surface apply、catalog activate和旧lease
revoke形成同一 prepared operation。

## 合并后的 P1 工作面

| 工作面 | 合并内容 | 前置 |
| --- | --- | --- |
| Use-case seam | Project/BackendAccess/Workspace/ProjectAgent/VFS route写；Story/Workflow MCP direct write；聚合 `RepositorySet` | 对应 P0 command/transaction port |
| Durable operation | Codex OAuth flow、Runner enrollment、Lifecycle create idempotency | owner/receipt schema |
| Routine/Channel/Wait | Routine typed phase、Channel semantic identity/outbox、exec activity durable projection | Routine occurrence；Lifecycle command store |
| Execution composition | frozen subsystem builder、execution profile catalog、exact Hook outcome合同 | generation-scoped tool catalog的 builder 落点 |
| Local/Relay | Terminal semantic launch、bounded Local command lanes、WS header auth、lossless MCP result | typed operation/wire contract |
| Dependency direction | Platform SPI hard cut、persistence/composition拆分、VFS core/Relay adapter拆分、application aggregate retirement | semantic ports已经稳定 |
| Frontend owner | live delta coordinate、Workspace presentation port、Project event dispatcher、Terminal port | backend/wire owner合同 |
| Generated contract | JSON integer policy、Agent live decoder、route DTO、Tauri IPC、generator DAG | 唯一wire owner与runtime fixture |

## 应保留的稳定边界

整改计划必须显式保护以下正例，防止“全仓收敛”退化为机械拆分：

- Workflow reducer + CAS executor + durable Function/HumanGate effect/replay；
- Interaction state/event/receipt/effect intent 单事务提交；
- Canvas definition revision/lineage/CAS application service；
- Workspace placement 的 detect fact + binding + inventory command owner；
- BackendAuthorization 的跨聚合 query policy；
- Workspace Module 对 Extension/Canvas/Operation 的纯 projection builder；
- Product AgentRun facade、concrete Agent source/effect、Host process route三者分权；
- Runtime tool的 committed binding + applied surface双证据授权；
- Relay Runtime Wire 对 route/sequence/ack/lost 的 transport ownership；
- Local物理资源校验作为Cloud authorization之后的第二层执行保护；
- terminal Local PTY truth 与 Cloud Product projection分权；
- Tauri作为本机平台composition shell，而不是业务事实owner。

## Planning work package graph

以下是跨报告去重后的 planning 节点；批准后每项再补齐文件所有权、验证命令和 child task metadata。

```text
W0 Boundary characterization + architecture harness self-tests
 ├─ S1 secure operational diagnostics
 ├─ S2 workspace setup admission
 ├─ S3 relay header authentication
 ├─ A1 LifecycleRunCommandStore
 │   ├─ A2 Task authorization + cross-entry command
 │   ├─ A3 Lifecycle dispatch transaction/outbox
 │   └─ A4 Hook log / terminal typed mutations
 ├─ A5 Project grant transaction
 ├─ D1 Project semantic retirement
 │   ├─ D2 Project/Story/asset DB deletion transaction
 │   ├─ D3 durable artifact lifecycle
 │   ├─ D4 AgentRun retirement port + tool lease revoke
 │   └─ D5 Local extension cache/Host revoke lifecycle
 ├─ D6 Asset operation transaction
 │   ├─ D7 Canvas promotion command
 │   └─ D8 contextual document transaction
 ├─ R1 Routine occurrence + distributed claim
 │   └─ R2 Routine phase/recovery/admission
 ├─ C1 Interaction effect worker
 ├─ C2 Companion human response delivery
 ├─ C3 Gate terminal convergence worker
 └─ X1 frozen runtime subsystem composition
     ├─ X2 generation-scoped dynamic tool catalog
     ├─ X3 exact Hook outcome contract
     └─ X4 unified execution profile catalog

After owning command/ports stabilize:
 U1 Story command service (HTTP/MCP + inline transaction)
 U2 Workflow definition command service (HTTP/MCP)
 U3 durable OAuth operation
 U4 runner enrollment transaction
 U5 Wait durable exec activity
 U6 Channel semantic identity/outbox
 U7 lifecycle create idempotency
 U8 remaining route/MCP use-case seams + RepositorySet removal

Independent cross-layer lanes after W0:
 F1 Agent live delta coordinate
 F2 Workspace presentation port
 F3 Project event dispatcher
 F4 honest generated wire + runtime decoders
 F5 generated Tauri IPC + path contract
 F6 Terminal frontend application port
 L1 Local Relay bounded dispatch
 L2 lossless Runtime MCP result
 L3 Terminal semantic launch commit

Physical dependency/composition convergence:
 P1 Platform SPI concrete Agent hard cut
 P2 persistence / runtime composition split
 P3 VFS core / Relay / Agent tool adapter split
 P4 application aggregate retirement + subsystem AppState handles

Final closure:
 G1 entrypoint/admission/reachability blocking gate
 G2 mutation/transaction/data-owner blocking gate
 G3 dependency/public-owner blocking gate
 G4 contract/IPC/frontend-owner blocking gate
 G5 composition/recovery/deployment blocking gate
 G6 hard delete dead protocol, facade, re-export, stale spec/guard roots
```

关键依赖：

- S1/S2/S3、C1/C2/C3 是当前安全或生产断链，不等待物理 crate split；
- A2/A3/A4 依赖 A1，避免在 broad-update 上修局部 policy/retry；
- D4 消费 X2 的 revoke API，Project retirement仍由 D1 唯一拥有；
- U8 必须在各 owner command已经存在后执行，不能先加通用 facade；
- P1-P4 只移动已稳定的 contract/adapter，不承担 P0 语义设计；
- 每条 G gate 在相应违规归零的 package 中启用，不建立永久 baseline allowlist；
- G6 只能删除已无 production consumer的结构，不能成为“顺便修业务”的汇总包。

## Parent/child task建议

父任务保留：

- 全局 owner/invariant 和 work package DAG；
- 交叉 task 的 entrypoint、data owner、generated contract 与 composition验收；
- 每个 child 完成后的 blast-radius 更新；
- 最终 hard-delete和blocking gate闭合。

子任务按上述节点创建，不按当前 crate或大文件创建。若多个节点会同时修改 AppState、migration或
quality-gates manifest，父任务必须给出串行顺序；“并行修各自功能，最后解决冲突”不算可执行所有权。

## 结论

后端的总体形态不是“所有模块都应该重写”。成熟的 reducer、transaction、authorization和transport
边界已经存在，当前事故主要来自三类系统性断点：

1. 入口可以绕过唯一 command/admission owner；
2. 跨事实写入和外部副作用没有 semantic operation identity/receipt；
3. production composition与CI不能证明实现真的被装配、完整且可恢复。

因此收敛计划的主线是：先把正确核心变成唯一可达路径，再移动物理依赖和目录；不是先拆 crate 后期待
边界自然出现。
