# Revised Plan Independent Review

审查对象：当前 `prd.md`、`design.md`、`implement.md`、根 manifests、九个
workstream 及其 manifests，并与 07-10 最终设计、目标 crate shape 和本轮三份新增
research 交叉核对。

## Critical

无。

## Major

### M1 — Fork 仍缺少跨 owner 的 durable saga 闭环

父设计已经承认 Product、Runtime、Host、Complete Agent 分属不同写 owner
（`design.md:190-205`），且跨 transaction 只能依靠 stable identity、receipt、inspect
和 reconciliation 收敛（`design.md:851-863`）。但 Fork 时序仍把
`child mapping + lineage + projection + change` 合并成一次 Runtime commit
（`design.md:622-645`）；其中 product lineage 属于 Application，binding/source
coordinate 属于 Host，不能由该 Runtime transaction 原子提交。

实施计划把相关文件分散给 W3、W5/W6 和 W7（`implement.md:181-218`,
`implement.md:261-337`, `implement.md:339-381`），却没有指定：

- saga 的唯一 durable owner、repository/table 和 phase；
- Agent fork side effect 前必须先落下的稳定 intent；
- product graph、Host child binding、Runtime child mapping 各自的提交顺序；
- Agent 已创建 child、平台 outcome unknown 时由谁 inspect 并继续提交同一个 child；
- restart 后如何区分“尚未调用”“已创建但未映射”“已映射但产品 receipt 未完成”；
- terminal `Lost` 时如何保存已知 child coordinate，避免第二次 fork。

这正是当前 research 已确认的 crash gap，也是 PRD R6/AC7 要求关闭的核心问题
（`prd.md:254-274`, `prd.md:421-426`）。在 saga owner、durable phase 和逐边界
reconciliation 写入父设计与工作包前，Fork 不能称为可实施的 crash-safe 方案。

### M2 — W2 与 W8 对 crate move/delete 存在重复 ownership

W2 已负责当前 `agentdash-agent`、`agentdash-agent-types` 和目标
`agentdash-agent-core`，并明确执行 Core 迁移、新 Dash Agent 建立和 types crate 删除
（`implement.md:136-179`）。W8 又负责 crate directory moves/deletions，并再次要求
“当前 agent 内容迁至 Core、新 Dash Agent 占用名称、删除 agent-types”
（`implement.md:385-411`）。

这不是单纯的最终验收重叠：两个工作包都声明实际移动/删除同一文件树和 Cargo identity，
违反父计划要求的唯一文件/module ownership（`implement.md:3-16`）及 workstream
dispatch 规则。应明确为以下一种：

- W2 完成物理 rename/move、建立新 crate 并删除 `agentdash-agent-types`，W8 只做最终
  workspace DAG、composition 和残留 gate；或
- W2 只在明确的临时模块路径实现，不移动 crate，W8 独占全部物理 cutover。

当前文本同时选择了两者，无法安全派发。

### M3 — Complete Agent contract 没有承载 surface apply 与 Agent-native Tool/Hook 回调

父设计要求
`AgentSurfaceSnapshot × RuntimeOffer -> BoundAgentSurface -> AppliedAgentSurface`
并以 applied evidence 决定 availability（`design.md:343-409`），但给出的
`CompleteAgentService` 只有
`describe/create/resume/fork/execute/read/changes/inspect`
（`design.md:258-316`）。合同中没有：

- `BoundAgentSurface` 的 apply/update/revoke 命令与 `AppliedAgentSurface` receipt；
- Agent-native tool callback 到 Runtime Tool Broker 的 typed reverse channel；
- Agent-native blocking/mutating Hook invocation、decision、deadline 和 effect correlation；
- remote service 对上述 reverse call 的 wire/ack/replay 语义。

W4 要求 materialize、applied evidence 和唯一 causal route（`implement.md:220-259`），
W1 又负责冻结 service API/wire（`implement.md:91-134`）。如果 W1 不先定义这条合同，
W4 只能越过 Complete Agent seam 调 adapter-specific API，或把 exact Hook/Tool
退化为 observation，都会破坏统一外层。需要在 W1 contract freeze 前补齐；这属于技术
合同缺口，不需要新的产品决策。

### M4 — 07-10 的 `agentdash-application-runtime-session` 清理没有进入可验收范围

07-10 最终 target 明确要求拆解并删除
`agentdash-application-runtime-session`
（`07-10.../target-crate-shape.md:64-75`, `:193-202`）。本轮 PRD 声明继承全部未完成
清理（`prd.md:330-357`），但逐 crate 列表和 AC17 未列该 crate
（`prd.md:334-354`, `prd.md:447-450`）；父 design 的 crate actions 同样遗漏
（`design.md:932-950`）；W8 implement 与 negative gates 也没有搜索或删除它
（`implement.md:395-420`, `implement.md:454-464`）。

因此当前计划即使全部勾选，也可能保留旧 RuntimeSession ports、旧 session persistence
事实链和 Application 对 concrete runtime/session 边界的依赖，却仍通过 AC17。应把该
crate 及 `agentdash-application-ports` 中对应 RuntimeSession/connector 泄漏明确分配给
W3/W7 迁移、W8 删除，并加入 AC17 与 negative gate。

### M5 — Companion 的产品规则在父文档中仍未决，workstream 却私自固化

父 design 只规定必须区分 `ForkParentHistory` 与
`FreshWithContextPackage`，并明确 adoption/slice 到命令的映射属于产品规则
（`design.md:653-660`）；父 implement 也只要求显式区分两类命令
（`implement.md:350-368`）。但 W7 workstream PRD 直接规定
“Companion Full exact fork history，其余 slice fresh typed context”，同时声称
`adoption_mode` 与 history 创建方式正交。

该规则没有出现在父 PRD/design，也没有需求依据或用户确认。它会直接改变 Companion
历史继承、token/context、隐私和成本语义，不能由子 workstream 自行决定。父 PRD 当前
没有 Open Questions，而 `implement.md:5-9` 又要求开始前 Open Questions 清零；两者
并不代表该产品决策已经完成。

此外，父 design 尚未定义 `FreshWithContextPackage` 的 typed package 结构、由
`create` 还是首个 command 交付、所需 context fidelity/capability，以及它与普通
`SubmitInput` 的区别。该 contract 缺口应与 Companion 产品映射一起在父文档落定。

## Minor

### m1 — W3/W8 migration “final consolidation” 与 migration immutability 的交接不清

W3 声明 Runtime/Host migrations 由其持有直到 W8 final consolidation
（`implement.md:181-201`），W8 再负责 final migration（`implement.md:385-429`）。
父 design 同时要求一次 forward hard cut（`design.md:865-876`）。需要明确 W3 只提交
ports/in-memory schema contract，还是允许新增正式 migration；若 W3 已新增正式
migration，W8 只能追加 forward migration，不能重写已提交历史。否则并行工作包会在
migration 文件 ownership 与 guard 上冲突。

### m2 — workstream manifests 对“当前基线 spec”的优先级提示不足

根 manifest 已注明现行 Runtime context/kernel specs 只是拆分前基线，实施以父 design
的新 owner 为准；但 W5 manifest 直接引用仍声明 Managed Runtime 拥有
context/compaction 的 `agent-runtime-context.md`，理由只写“当前 Native
context/compaction 基线”。W3/W5 检查若只按 workstream manifest 执行，容易把旧
Managed Runtime context authority 带回实现。

建议每个引用旧 ownership spec 的 workstream manifest 都明确：
“只读取现有行为/测试事实；owner 与目标 API 以父 design 为准”，并让 W9 更新后的最终
spec 成为验收事实源。父计划已要求 check 重新读取父 artifacts
（`implement.md:496-511`），因此这是 manifest 清晰度问题，不是架构方向错误。

### m3 — Dash Agent history/lifecycle 两个 transaction 的交接需要显式 durable intent

父 design 把 Dash history transaction 与 Dash lifecycle transaction 分开，后者提交
“command/effect settlement + next history append intent”
（`design.md:851-860`），但目标 schema 只列出
`dash_agent_session/history_entry/history_branch/command/effect/change`
（`design.md:832-844`），未明确 append intent 的 identity、claim/retry、CAS 和
exactly-once 约束。W2 的 replay/A/B/C tests 能证明 fold 语义，却不能单独证明 lifecycle
settlement 后 crash 不会丢失或重复 history contribution。

这不改变 history-only Session 结论，但实施前应把该 intent 归入
`dash_agent_command` 或独立 typed ledger，并写入 W2/W8 的 constraint 与 crash test。

## Confirmed

1. **统一 Managed Runtime 外层被正确保留。** PRD FP1/R1 与 design 都保持
   `Application/AgentRun -> Managed Runtime -> Host -> Complete Agent Service`
   （`prd.md:27-36`, `prd.md:157-179`; `design.md:19-30`, `design.md:89-182`）。
2. **Complete Agent、Dash Agent、AgentCore 层次正确。** Dash Agent 与 Codex/企业
   Agent 同为 Complete Agent 实现，只有 Dash Agent 下接纯 Core
   （`design.md:165-188`, `design.md:514-575`）。
3. **`Session` 已收窄为 history-maintained state。** 定义、命名限制、Dash lifecycle
   外置和 history fold 一致（`prd.md:80-88`, `prd.md:136-153`;
   `design.md:59-75`, `design.md:518-554`）。
4. **外部 Agent source authority 与 Runtime projection 已原则性拆开。**
   ownership/authority/fidelity、Codex ThreadStore authority、snapshot reconcile 和
   projection 禁止反向写回均清楚（`design.md:190-228`, `design.md:577-603`,
   `design.md:739-790`）。
5. **Compaction owner 已按 Agent 实现分级。** Dash Agent 拥有 exact history
   transformation/A-B-C，Codex 使用 native compaction，其它实现走 capability gate；
   Runtime 只投影统一 activity/operation（`prd.md:276-300`;
   `design.md:662-737`）。
6. **Tool/Hook 的唯一 causal route 原则正确。** desired/offer/bound/applied 分层、
   required gate 和禁止双执行均明确（`prd.md:302-313`;
   `design.md:343-410`）。M3 只指出 service/wire 落地 seam 尚未补出。
7. **Runtime change 与 external source change 没有混成同一 log。** 平台始终提供 durable
   snapshot/change，source tail 可按能力降级且 gap 通过 source snapshot reconcile
   （`design.md:331-341`, `design.md:739-790`）。
8. **目标 crate DAG 已补上 07-10 缺失的 Dash Agent 中层。**
   `Dash adapter -> agent -> agent-core`、Runtime/Host/service API 的禁止反向依赖总体正确
   （`design.md:878-960`）。M2/M4 是迁移 ownership 和遗漏清理，不否定目标 DAG。
9. **Fork cutoff 与外部能力差异已有合理基线。** completed Turn 是 common exact
   minimum，Item/source cursor 只有在 Bound surface 证明 Exact 时开放，不做隐式取整
   （`design.md:605-620`）。
10. **多数原 research open questions 已由父设计关闭。** source change tier、
    Codex context fidelity、pi adapter scope、Host 独立 crate、hard cut 和 common fork
    cutoff 都已有明确选择；不需要再次提交用户决策。

## Remaining user decisions

### 1. Companion 模式到 history 创建方式的产品映射

需要用户确认每一种 Companion slice/adoption 模式究竟使用：

- `ForkParentHistory`；或
- `FreshWithContextPackage`。

尤其需要确认 W7 workstream 提出的“Full 必须 exact fork、其余 slice 全部 fresh”是否是
目标产品规则，以及 `adoption_mode` 是否确实与 history 创建方式正交。父 design
`design.md:653-660` 当前只定义两类命令，没有做该映射。

除这一项外，本次发现的 Fork saga、surface callback seam、crate ownership、migration
交接和 07-10 清理遗漏都属于可由架构约束直接修正的技术问题，不需要扩大为新的用户产品
决策。
