# Agent Runtime 跨层扩散评估

## 结论

当前问题既包含浅interface，也包含一组可以确认的历史遗留双层事实语言：

1. Complete Agent Service API 与 Runtime Contract 重复定义execution/context/interaction/control；
2. context observation 的一致性规则继续泄漏到 Product、HTTP 和 UI；
3. item terminal 被拆散在事件类型、Turn terminal 与 item body；
4. wire encoding 规范没有机械闭合到所有 nested carrier，生成工具也未稳定输出。

Complete Agent callable interface是真实seam，但“seam存在”不要求它拥有独立crate和第二套事实
vocabulary。Service API与Runtime Contract都属于AgentDash-owned Managed Runtime，并且各有62个Rust
consumer、29个文件同时import两者；它们应物理合并并复用canonical nested facts。Wire与canonical
presentation仍是不同变化轴，应继续独立。

## Finding 0 — Service API / Runtime Contract 双层事实语言（P1）

### 证据

- `AgentContextCoordinate`与`AgentRuntimeContextCoordinate`字段一一对应，仅类型名不同。
- `AgentSnapshot`和`AgentRuntimeView`分别定义lifecycle、execution、command availability、
  interaction、context、authority/fidelity与conversation。
- `agent_snapshot_projection.rs`包含逐枚举映射、ID重建和serde JSON transcode。
- `map_initial_context_package`再次逐字段映射两套initial context package，并重新计算同一digest。
- `AgentInputContent`/`AgentRuntimeContentBlock`、`AgentInteractionResponse`/
  `AgentRuntimeInteractionResponse`也在同一进程内重复表达同一输入。
- `agentdash-agent-service-api`与`agentdash-agent-runtime-contract`各被62个Rust文件import，
  29个文件同时依赖两者。
- 两个crate都只依赖`agentdash-agent-protocol`，物理合并不会形成依赖环。
- source-backed view中的turn/item/interaction/effect identity被原值复制到另一组Runtime ID；
  `RuntimeProjectionRevision`也直接从`AgentSnapshotRevision`复制。

### 判断

这是旧Driver/Runtime分层演进后的历史遗留。Complete Agent seam需要保留callable interface与source
wrapper，但不需要另一套AgentDash-owned事实名称。目标是一个contract crate、一套canonical nested
facts、两个最小wrapper，而不是再加第三个shared crate。

“一套事实”不等于“所有状态机共用一个坐标”：

- turn/item/interaction、context recipe、source execution、initial context package等同一业务事实
  应复用同一类型；
- `AgentSourceCoordinate`与`RuntimeThreadId`属于不同identity domain，必须保留；
- source observation revision与Product provisioning/projection revision可能独立前进，必须拆清，
  不能继续用`RuntimeProjectionRevision`同时表达两者；
- Complete Agent的7类source control与Product额外的create/resume/rebind/activate admission不是同一
  policy。Product可以组合source availability，但不能把derived admission写回source observation。

### 方案比较

| 方案 | 结论 | 原因 |
| --- | --- | --- |
| 保留两个crate，再增加shared facts crate | 否决 | 形成第三层依赖与命名，不减少consumer寻找事实owner的成本 |
| 只物理合并crate，保留两套DTO | 否决 | 只减少Cargo边，不消除transcode、枚举映射和变更扩散 |
| Product直接暴露`AgentSnapshot` | 否决 | 泄漏source coordinate、applied surface、initial context等private evidence，也无法表达source出现前的provisioning |
| 一个contract crate + canonical facts + source/Product discriminated wrapper | 建议 | 保留真实状态机边界，同时删除同构事实和无意义转换 |

### 推荐物理边界

- 保留一个`agentdash-agent-runtime-contract`，内部按`facts`、`complete_agent`、`product_runtime`
  分module；删除`agentdash-agent-service-api`。
- 保留`agentdash-agent-runtime`作为协调/验证/投影实现，不把实现并入contract。
- 保留`agentdash-agent-runtime-wire`作为跨进程envelope、placement、revision与ack owner；业务payload
  直接引用统一contract。
- 保留`agentdash-agent-protocol`作为conversation presentation owner。

`AgentRuntimeView`仍有存在价值，但应表达Product aggregate，而不是复制一份Agent snapshot。它需要
显式区分provisioning与attached/source-backed状态；attached分支嵌入canonical observation，
provisioning分支只携带Product operation/binding/admission。这样projection seam只做identity隐藏、
Product状态组合与安全证据派生，不再做同构业务事实翻译。

## 事故样本

基线提交：`0187fb58d fix(compaction): 收口终态展示与上下文版本门禁`

| 现象 | 直接修复 | 扩散结果 |
| --- | --- | --- |
| compaction failed/lost/cancelled 永久显示进行中 | typed compaction status + terminal item update + reducer/card 识别 | protocol、native projector、frontend reducer、registry、tests 同时理解 terminal |
| context inspector 可能提交旧 snapshot | Agent/Runtime context coordinate + required revision + frontend fence | Agent API、Runtime contract/projector、两个 adapter、Product/API、generated TS、UI 同时理解 coordinate |
| contract revision test 漂移 | 同步 test expectation | protocol revision 仍依赖人工维护 |

提交共修改 47 个文件，增加 3,390 行、删除 2,243 行。3 个 schema 文件贡献 4,372 行 diff，
大部分是定义重排而非等量业务变化。文件数本身不是架构判据，但该分布说明 semantic change 与
mechanical propagation 尚未分离。

## Finding 1 — Source context contract 穿透 Runtime seam（P1）

### 证据

- `agentdash-agent-service-api::AgentContextSnapshot` 是 Complete Agent source contract。
- `AgentRunProductProjectionGateway::context_snapshot` 直接构造 `AgentContextQuery` 并返回
  `AgentContextSnapshot`。
- HTTP route 直接把 `required_revision` 映射为 `AgentSnapshotRevision`。
- frontend service 与 Session UI 直接消费 generated `agent-service-api.ts`。
- `contextSnapshotFence.ts` 在 UI 解释 snapshot/context revision、digest 和 commit order。

### 判断

Complete Agent seam 与 Runtime normalized seam 都是真实边界，但 Runtime context read 尚未形成深模块。
Product、API 和 UI 被迫学习 source coordinate 的正确用法，导致 source contract 一扩展就全链修改。

### 删除测试

若删除 Product gateway 当前的 `context_snapshot` pass-through，实现几乎原样出现在 route 或 UI；
它没有隐藏一致性行为，属于浅 module。若删除 Complete Agent service seam，Native/Codex/Remote
差异会重新扩散到 Runtime，说明该 seam 必须保留。

## Finding 2 — Canonical item lifecycle 没有唯一 terminal 事实（P1）

### 证据

- Backbone 提供 `ItemStarted/ItemUpdated/ItemCompleted` 三种 notification，但 completed 没有统一 outcome。
- `SessionEntry` 把 event type 映射为 `"started" | "updated" | "completed"`。
- card registry 又结合 item-specific status 解释 display state。
- Native compaction 失败路径曾只发布 Turn terminal；当前修复通过 `ItemUpdated` 携带 terminal status。
- `AgentDashThreadItem` 是 untagged enum，Codex 与 AgentDash 分支均可序列化为
  `type=contextCompaction`。

### 判断

生命周期“何时结束”和结果“如何结束”分别散落在 event、Turn 和 item body。当前补丁修复了产品
行为，但不是通用 invariant；下一个 maintenance item 仍可能重复该错误。

### 目标

canonical projector 必须保证每个 started item exactly-once terminal，terminal 自带 outcome。
frontend feed 在 reducer seam 折叠该事实，card 只负责展示。

## Finding 3 — 传输规范化没有闭合到 nested carrier（P1）

### 证据

- `agent_runtime_validators.ts` 手工 decode/encode `view.context.snapshot_revision`、
  `update.context.snapshot_revision` 等 nested scalar。
- 同一文件既做 object runtime validation，又以 spread/cast 重建 generated wire。
- 新增 nested coordinate 后必须同时修改 Rust contract、源 validator 与 generated validator。

### 判断

`u64 -> JSON decimal string -> TypeScript bigint` 属于传输编码约定，不是业务 contract 自身，也
不是生成器自行决定的语义。当前 encoding normalization 只覆盖已手写字段路径，开发者必须知道每个
nested integer 的 carrier 细节。生成器应从 encoding spec 与 schema机械派生 codec，而不是成为
新的语义 owner。

## Finding 4 — Schema 输出不稳定放大审阅成本（P2）

### 证据

- 一次增加少量字段导致三个 schema 文件出现数千行 definition reorder。
- wire 与 service schema 的变更规模相同，显示机械复制/遍历顺序占主导。

### 判断

这不直接产生运行期错误，但会隐藏真实 contract 变化、增加冲突并降低 review 可靠性。应在 generator
输出端稳定 object/definition 顺序，不应通过忽略 generated diff 解决。

## Finding 5 — 浅层 fixture 复制 required contract（P2）

### 证据

- workspace 中至少 5 个 Rust 文件直接构造 `AgentSnapshot`，4 个文件直接构造
  `AgentRuntimeView`。
- frontend 已有共享 fixture，但 required coordinate 仍需在多类 connection/projection tests 同步。
- 两个失败单测来自实现合同已变化、fixture/assertion 未同步。

### 判断

测试跨过了目标 interface，直接复制完整 wire shape。深 module interface tests 建立后，应删除
pass-through tests，并把合法对象构造集中到 test-support builder。

## 应保留的合理边界

| Seam | 保留原因 | 允许理解的事实 |
| --- | --- | --- |
| Complete Agent callable interface | Native/Codex/Remote/Test 多adapter，且Agent独占source history/context | source command/read/live/inspect；复用统一facts |
| Managed Runtime Contract | Complete Agent与Product/UI共同使用的AgentDash-owned事实语言 | canonical observation、context、command、receipt |
| Runtime Wire | local/remote deployment 与握手/revision 是真实变化轴 | envelope、transport revision、serialization |
| Canonical Presentation | 多 Agent 映射同一 Session 产品语言 | Turn/Item/Interaction presentation |
| Frontend Runtime state | target 生命周期、网络取消与 UI render 是前端变化轴 | keyed loading/ready/error，不拥有 source fact |

## 不应保留的泄漏

| 当前 consumer | 不应再理解 |
| --- | --- |
| Complete Agent / Runtime mapper | 两套execution/context/interaction/control类型与JSON transcode |
| Product projection gateway | source context query 构造、revision/digest 校验 |
| HTTP route | `AgentSnapshotRevision` 与 source context DTO |
| Session UI | request generation、committed revision、recipe digest fence |
| Card renderer | event type + Turn terminal + item status 的组合推理 |
| validator source / transport consumer | 每个 nested `u64` 的字段路径与 carrier 规则 |
| pass-through tests | 完整 snapshot/view 必填字段清单 |

## 预期变化半径

| 变化模拟 | 当前典型修改面 | 目标修改面 |
| --- | --- | --- |
| context coordinate 增加 provenance 字段 | source API、Runtime DTO/projector、gateway、route、frontend fence/fixture | canonical fact一次修改、wrapper自动复用；仅真实展示consumer可选修改 |
| 新增 maintenance item terminal outcome | Agent history、projector、item body、reducer、card | source projector + canonical lifecycle；通用 reducer自动闭合 |
| Runtime view 增加 nested `u64` | Rust contract、手工 TS codec、generated codec、fixtures | Rust contract/schema引用既有encoding kind；normalized codec自动派生 |
| schema 增加定义 | 多个 schema 大范围重排 | 新定义与直接引用附近的稳定 diff |

## 与 07-26 架构任务的关系

07-26 负责全仓 stable boundary map、architecture harness 与 blocking gate 体系。本任务不复制该范围；
它为以下边界提供可执行纵向 proof：

- Runtime observation interface depth；
- canonical item terminal invariant；
- generated contract DAG与wire encoding normalization；
- frontend owner-scoped Runtime controller。

完成后将 proof 链接回 07-26 的 BND-10/BND-11 及 AgentRun/Runtime coverage，不在本任务创建子任务。
