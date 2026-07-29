# Agent Runtime 状态面与控制面收束实施计划

## 实施状态

- A0已提交`9828df7f4`：固定跨层行为基线与边界证明。
- A1已提交`470d26a67`：统一Agent observation、Runtime context state plane与前端controller。
- A2已提交`0ecab434e`：统一canonical item terminal lifecycle。
- A3已提交`371300f7a`：统一schema递归encoding generator与generation manifest。
- A4已完成：共享coherent builder、blocking architecture gate、长期规范与父架构账本收尾。
- 本任务不创建子任务；各slice顺序完成hard cut、局部验证与独立提交。

## Slice A0 — Characterization And Boundary Proof

### 目标

固定现有正确行为与错误复发样本，建立后续 hard cut 的测试面。

### 工作

- 记录 Context/Compaction 当前纵向调用图与 owner。
- 建立identity/revision domain表，证明turn/item/interaction/effect、source observation、
  Product projection与surface revision哪些同域、哪些必须隔离。
- characterization覆盖source出现前的provisioning view与source-backed attached view，防止合并时
  丢失Product独有状态。
- 增加wrapper一致性证明：同一source observation进入attached Product state后逐值不变，Product
  只能增加binding/operation/admission/presentation evidence。
- 在 Runtime observation interface 层建立 stale/behind/mismatch/source mismatch/live gap tests。
- 建立 canonical lifecycle invariant fixture：
  `started -> progress* -> terminal(outcome)`，覆盖四种 outcome 和 reconnect。
- 为 wire encoding normalization 建立 nested object/array/optional/map/union scalar
  characterization。
- 记录两次 schema generation 的 semantic digest 与文本 diff。

### 完成定义

- 测试能复现旧架构的 source DTO leak、缺 terminal 与绕过 encoding spec 的手写 nested scalar
  扩展点。
- 不修改生产行为。

## Slice A1 — Unified Contract / Runtime Observation / Context State Plane

### 目标

删除Service API/Runtime Contract双层事实语言，让Runtime深module独占source read、Product wrapper与
context coherence。

### 工作

- 将`agentdash-agent-service-api`全部module并入`agentdash-agent-runtime-contract`。
- 统一同domain identity/revision、authority/fidelity、execution、source availability、interaction、
  initial/context recipe与conversation nested facts；source/Product只保留最小wrapper。
- 保留application层既有Absent/Current Product observation；`AgentRuntimeView`只包装成功读取的
  canonical observation与browser-safe presentation evidence，不新增provisioning状态机。
- Product lifecycle operation/admission留在Product use case，Runtime view只携带source control
  availability。
- 删除`agentdash-agent-service-api` crate、workspace member/dependency、schema与generated TS。
- 删除`agent_snapshot_projection.rs`中的同构serde transcode和逐枚举复制。
- 在统一Runtime contract增加Product context requirement/projection/error。
- 在 Runtime crate 实现 `RuntimeObservation`，组合既有 `CompleteAgentService`。
- Product projection gateway只负责 target/binding resolve与调用 observation。
- API route改为 Runtime DTO，不再 import source context contract。
- generated frontend contract只暴露统一Runtime Product wrapper，不生成Service API浏览器合同。
- 把 request/abort/generation/committed coordinate/state preservation 移入
  `features/agent-run-runtime` controller。
- Session context inspector只消费 controller state。
- 删除旧source DTO frontend exposure和`contextSnapshotFence.ts`。

### 验证

- Native/Codex统一contract compile与context lower-bound、equal-coordinate、behind/mismatch。
- target switch、迟到响应、refresh failure保留、coordinate advance。
- `agentdash-agent-service-api`、重复fact type与source DTO frontend exposure负向搜索。

### 预期收益

新增 context coordinate 字段不再修改 Product route 与 Session UI 协议逻辑。

## Slice A2 — Canonical Item Lifecycle / Control Plane

### 目标

把异步 item 的 running/terminal/outcome 收束为 canonical invariant。

### 工作

- 定义 AgentDash-owned item terminal evidence。
- Native、Codex projector统一产生 exactly-once terminal。
- `item_updated` 回归 progress-only。
- compaction 收敛为单一 canonical item shape，移除同 discriminator untagged branch。
- Session reducer输出 `{ item, lifecycle }`。
- card registry/card shell只消费 lifecycle view model。
- 删除 compaction terminal特殊fallback与Turn terminal补推理。

### 验证

- success/failed/lost/cancelled、无assistant item、terminal-only turn。
- duplicate terminal、terminal without start、identity mismatch负向测试。
- snapshot/read/live/reconnect parity。
- 其它长时 item family回归，证明通用 invariant未误判 tool success/failure。

### 预期收益

新增异步 maintenance item只需 source body/projector，不再为每个 card重写终态协议。

## Slice A3 — Transport Encoding Normalization / Stable Schema

### 目标

固定 wire carrier规范，让 Rust contract 变化自动生成正确 normalization codec，并把 schema diff
降为语义变化。

### 工作

- 在 wire encoding spec 固定 domain/JSON/TypeScript 三端表示。
- 为 wire scalar添加引用 encoding kind 的 generator可读 metadata。
- 实现 schema递归 traversal与decoder/encoder生成。
- 删除 `agent_runtime_validators.ts` 中绕过 encoding spec 的字段路径式 nested scalar逻辑。
- canonicalize definitions/properties object order，保留 union/event array顺序。
- generator输出 manifest：root、input、output、digest。
- contract check连续生成两次并验证第二次 clean。

### 验证

- nested object/array/optional/map/union round-trip。
- number、unsafe integer、malformed string、missing field负向测试。
- Runtime view/update/receipt全根类型 codec测试。
- schema semantic equivalence与deterministic output测试。

### 预期收益

新增采用既有 encoding kind 的字段不再需要手改 TypeScript validator；generated diff可审阅。

## Slice A4 — Test Topology / Architecture Gates / Cleanup

### 目标

让深 module interface成为长期测试面，并阻止旧路径复发。

### 工作

- 在 runtime test-support引入 coherent builders。
- 迁移跨 crate完整 snapshot/view literals。
- 删除被 observation/lifecycle/encoding interface tests覆盖的浅测试。
- 添加 blocking gates：
  - Runtime frontend不得依赖 Agent Service context DTO；
  - Product/API不得构造 source context query；
  - started item必须终态闭合；
  - normalized encoding不得包含字段路径补丁；
  - generator rerun必须clean。
- 更新 Runtime Context、Kernel、Frontend Contract/State specs。
- 将 boundary proof链接回 07-26 architecture stability ledger。

### 验证

- `cargo test`：Service API、Runtime Contract、Runtime、Wire、Native、Codex、Application AgentRun、
  contract generator相关 suites。
- `pnpm run contracts:check`
- frontend focused tests、typecheck、lint。
- `git diff --check`
- old-path absence searches。

### 预期收益

required contract扩展只修改 canonical builder或真实行为测试，不再批量修 pass-through literals。

## 顺序与提交边界

1. A0先固定行为。
2. A1完成 state plane authority cutover并删除旧DTO。
3. A2完成 control/presentation terminal hard cut。
4. A3独立收束生成链，避免与协议语义diff混在同一提交。
5. A4只做测试面替换、门禁和文档收尾，不承载新的authority决策。

每个 slice 使用项目提交格式，提交备注列出行为合同、删除的旧知识和验证结果。A1/A2不得为了拆提交
留下可运行的双轨。

## 完成证据

- `agent-runtime:guard`固定source DTO owner、canonical item terminal、retired path absence与
  schema-recursive codec边界，并与`contracts:check`共同进入PR quick/full gate。
- Agent Runtime相关Rust crates共通过300余项lib/integration/generator测试；
  `cargo check --workspace --all-targets`通过。
- `contracts:check`通过；前端typecheck与105个文件、521项测试通过。
- 本任务涉及的context controller定向ESLint通过。全量ESLint仍有32个其它模块的既有
  React Hooks错误，不属于本边界收束的修改面。
- `git diff --check`与old-path absence检查通过。

## Stop Conditions

- 若发现 Product gateway 对 context read存在尚未记录的授权或业务policy，先把它建模为具名
  Product interface并更新设计，不把policy移动进Runtime。
- 若 Codex native protocol无法表达通用 terminal outcome，adapter必须投影为AgentDash canonical
  evidence；不得让vendor DTO泄漏到frontend。
- 若 schema canonicalization改变语义digest，停止A3并先修复generator，不接受“看起来只是排序”。
- 若并行工作修改同一文件，只记录冲突并等待/协调，不覆盖对方修改。
