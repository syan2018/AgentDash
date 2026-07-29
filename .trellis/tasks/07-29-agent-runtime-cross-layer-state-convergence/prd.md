# Agent Runtime 状态面与控制面深模块收束

## Goal

以 Context/Compaction 修复暴露出的跨层扩散为事故样本，重塑 Agent Runtime 的 observation、
canonical presentation 与 frontend state seam，使 Complete Agent 的 source authority、
Runtime 的 normalized projection、Product/API 的路由职责和前端的交互状态各自闭合。

任务不是追求“少几个 crate”或“少改几个生成文件”，而是让一次事实变化只由事实 owner、一个
projection seam 和真实 consumer 理解。中间 transport、Product facade 和 UI 外壳只传递稳定合同，
不再共同解释 revision、terminal、fidelity 或恢复规则。

## Background And Evidence

- `0187fb58d` 为修复 compaction terminal 与 context revision fence 修改了 47 个文件；
  其中 3 个 schema 文件产生 4,372 行 diff，说明生成输出稳定性放大了审阅噪声。
- Context coordinate 当前依次穿过 Complete Agent Service API、Runtime contract、Runtime projector、
  Product projection gateway、HTTP query、generated TypeScript、前端 request fence 和 UI。
- Runtime context endpoint 最终直接返回 `AgentContextSnapshot`，使 Product/UI 消费了本应停在
  Complete Agent seam 的 source contract。
- canonical item stream 只有 started/updated/completed 事件形状，没有统一 terminal outcome；
  compaction failure/lost/cancelled 曾只能依赖 Turn terminal，导致 item 永久运行。
- wire encoding normalization 对 nested `u64` 仍需手写 decode/encode path；Rust contract 新增
  坐标字段时，编码规范不能通过 generator 自行闭合到 runtime validation。
- 多个 crate 的测试直接构造完整 `AgentSnapshot` / `AgentRuntimeView`，required field 扩展会驱动
  pass-through fixture 级联修改。

详细证据与判断见 `assessment.md`。

## Requirements

### R1 — Managed Agent Runtime 只保留一套事实语言

- `agentdash-agent-service-api` 与 `agentdash-agent-runtime-contract` 合并为一个
  `agentdash-agent-runtime-contract` crate；Complete Agent seam 作为其中的具名 module/interface，
  不再拥有平行的 execution/context/interaction/control vocabulary。
- 已确认保留source read与Product view两个aggregate wrapper：
  - source wrapper携带 `AgentSourceCoordinate` 与 Agent-private evidence；
  - Product wrapper携带 `RuntimeThreadId` 与 Product可观察附加字段；
  - 两者复用同一个 canonical observation/context recipe。
- 同一identity/revision domain、authority/fidelity、execution、source control availability、
  interaction、initial/context recipe和conversation record各有一个定义；禁止靠JSON transcode
  在两套同构类型间搬运。
- `AgentSourceCoordinate`/`RuntimeThreadId`、source observation revision/Product projection
  revision等不同domain保持类型隔离；Product lifecycle/admission与source lifecycle/control通过
  显式组合建模，不以同名字段互相覆盖。
- `AgentRuntimeView`使用discriminated Product state表达provisioning与attached/source-backed状态；
  source-backed分支嵌入canonical observation，不再平铺复制Agent snapshot。
- 两个wrapper都不是新的Agent事实owner：相同observation必须逐值一致，Product wrapper不得覆写、
  修正或缓存出另一份execution/context/interaction；内部一致性由contract shape与Runtime
  observation tests共同保证。
- 删除 `agentdash-agent-service-api` crate、独立schema/generated TypeScript和所有依赖声明。
- `agentdash-agent-runtime-wire` 继续作为真实跨进程transport seam，但只包裹统一Runtime contract，
  不重新声明业务事实。
- `agentdash-agent-protocol` 继续只拥有canonical conversation presentation，不拥有执行控制事实。

### R2 — 建立深的 Runtime Observation module

- Runtime 提供一个面向 Product/Application 的 observation interface，内部组合
  `CompleteAgentService + source binding + Product wrapper projection`。
- interface 至少覆盖 authoritative Runtime view、context projection 和 live reconcile；
  调用方不再自行拼接 source query、required revision、coordinate validation 和 projection。
- context read使用统一contract中的canonical context recipe；source/Product只增加各自wrapper，
  API、generated frontend contract与UI不接触source wrapper。
- Runtime observation 统一校验 source identity、snapshot revision、context revision、
  recipe digest、authority 和 fidelity。
- context body 仍按需读取，不塞入 Runtime view；view 只携带原子 context coordinate，避免大对象
  随每次 live update 重传。

### R3 — 收束前端 Runtime state owner

- `features/agent-run-runtime` 拥有 target-keyed Runtime view/context controller。
- controller 统一维护 loading/refreshing/error、request generation、abort、required coordinate、
  committed coordinate 和迟到响应拒绝。
- Session UI 只消费 ready/loading/error view model 与 command availability，不再实现一致性协议。
- Product view 与 Runtime view 保持独立 owner；一侧失败不能覆盖另一侧已提交状态。

### R4 — 建立 canonical item terminal invariant

- 每个 `item_started` 必须按同一 item identity 恰好产生一个 terminal boundary。
- terminal boundary 明确表达 `succeeded | failed | lost | cancelled`；`item_completed` 表示生命周期
  结束，不等价于成功。
- `item_updated` 只表达非终态进度，不承担失败终态的特殊兜底。
- compaction 在 AgentDash canonical protocol 中只有一个无歧义的 item shape；不得继续依赖两个
  同 discriminator 的 untagged variant。
- frontend reducer 在 feed seam 折叠 lifecycle，card renderer 不再组合 event type、Turn terminal
  和 item-specific status 猜测终态。

### R5 — 传输规范拥有 normalized encoding

- wire encoding spec 明确 JSON carrier、domain value 与前端运行时值之间的规范化规则；
  Rust contract/schema 是该规则在具体 DTO 上的唯一结构输入。
- generator 只是 encoding spec 的机械实现，根据 schema/type metadata 递归生成 nested wire
  scalar decode/encode；新增
  `RuntimeU64` 字段不要求手改 TypeScript path。
- normalized codec 必须覆盖 object、array、optional、map、union/ref，并有 malformed wire
  negative fixtures。
- frontend transport 只调用 encoding normalization 产出的 decoder，不维护 enum allowlist或局部
  bigint 修补。

### R6 — 稳定 schema 与生成 diff

- JSON Schema object definitions 和 properties 使用确定性输出顺序；语义未变化时输出字节稳定。
- 不排序有语义顺序的 union/event arrays。
- contract check 区分 source drift 与纯生成物 drift，并给出 owner/input/output。

### R7 — 以深模块 interface 作为测试面

- Runtime observation 的一致性、stale response、source mismatch、live gap/re-read 在 observation
  interface 测试，不在 Product/API pass-through 层重复。
- canonical lifecycle 通过通用 invariant suite 覆盖 success/failed/lost/cancelled 与 reconnect。
- adapter 测试只证明 vendor/source fact 到 Complete Agent contract 的映射。
- 引入 canonical test builders/fixtures，集中构造合法 snapshot/view/coordinate；builder 不进入
  production interface，也不通过默认值掩盖 required contract。
- 删除被更深 interface tests 取代的浅 pass-through 测试。

### R8 — 硬切与防复发

- 项目未上线，迁移直接到唯一最终合同，不保留旧 DTO、双读、fallback 或 compatibility re-export。
- 添加负向架构门禁：
  - Product/API/frontend Runtime 路径不得依赖 `AgentContextSnapshot`；
  - canonical projector 不得产生无 terminal 的 started item；
  - normalized encoding 不得存在按字段路径手工补丁；
  - generated output 必须可重复生成且无 diff。
- 本任务向 `07-26-module-coupling-stable-boundary-review` 提供 Agent Runtime boundary proof，
  但不接管其全仓 architecture harness。

## Acceptance Criteria

- [ ] Runtime observation interface 封装 source read、projection 与 context coordinate 校验；
      Product gateway/API 不再理解 source context query 细节。
- [ ] `agentdash-agent-service-api` crate、Cargo dependency、独立schema和
      `packages/app-web/src/generated/agent-service-api.ts`全部删除。
- [ ] Agent source snapshot与Product Runtime view复用同一canonical observation；execution、
      context、interaction、availability不再各定义两套同构类型。
- [ ] `agent_snapshot_projection.rs` 不再通过serde transcode或逐枚举映射复制canonical owner事实，
      只校验并构造Product wrapper/derived fields。
- [ ] 前端 Runtime 路径只消费 Runtime-owned generated contract，不再 import
      `AgentContextSnapshot` 或自行实现 revision/digest commit fence。
- [ ] 切换 target、live coordinate 前进、旧响应迟到、source snapshot 暂时落后和刷新失败均由同一
      controller 正确收敛。
- [ ] 所有 canonical started item 在 success/failed/lost/cancelled 下都具有 exactly-once terminal
      evidence；reconnect read 与 live fold 结果一致。
- [ ] canonical protocol 中不存在两个 JSON discriminator 相同的 compaction item branch。
- [ ] 新增一个使用既有 encoding kind 的 nested Runtime wire integer，只修改 Rust contract/schema
      标注；regenerated codec 自动通过 round-trip 与 malformed input tests。
- [ ] 连续运行两次 contract/schema generation，第二次 `git diff --exit-code` 通过。
- [ ] Runtime/Complete Agent fixture 通过 canonical builders 构造；pass-through fixture 与重复测试已删除。
- [ ] architecture negative gates 能在服务合同泄漏、缺 item terminal、绕过 normalized encoding
      或生成漂移时失败。
- [ ] Native 与 Codex 至少各有一条 view → context → live/terminal → reconnect 纵向组合测试。
- [ ] `pnpm run contracts:check`、相关 Rust tests、frontend typecheck/lint/tests 与
      `git diff --check` 通过。
- [ ] 没有旧 DTO、兼容读取、fallback、双写或 dead protocol branch。

## Out Of Scope

- 合并 Runtime implementation、Runtime Wire和canonical presentation；三者仍有独立变化原因。
- 改变 Complete Agent 对自身 history/context/compaction 的 authority。
- 重做 ContextFrame 的模型输入权威；该事实由 07-23 任务继续拥有。
- 全仓 architecture harness、RepositorySet 或非 Runtime 领域耦合治理；由 07-26 任务拥有。
- 与 Runtime state/control plane 无关的 UI 视觉改版。

## Constraints

- 实施基线必须包含 `0187fb58d` 的 terminal/context fence 行为，先 characterization 后替换。
- authority cutover 按 slice 一次完成；不为缩小 PR 留运行期双轨。
- 共享脏工作区中只修改本任务声明的文件；并行会话改动导致验证失败时记录而不覆盖。
- 任务保持单任务推进，不创建子任务。

## Notes

本任务是 07-26 全仓稳定边界评估的 Agent Runtime 纵向落地样本，但作为独立架构任务审阅和执行。
两个wrapper边界于2026-07-29确认；R1是该决策的唯一需求定义。
