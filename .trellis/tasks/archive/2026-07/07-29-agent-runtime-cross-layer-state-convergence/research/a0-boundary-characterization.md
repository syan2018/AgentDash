# A0 Boundary Characterization

## 基线

- 规划提交：`5a59ebe09`
- 当前实施基线：`9797af179`
- `9797af179`已经统一Runtime receipt与Complete Agent public effect identity，删除
  `AgentRuntimeView.operations`、execution/compaction中的重复operation identity以及持久化changes镜像。
  本任务直接继承该结果，不重新建立operation投影。

## 生产构造路径

`AgentRuntimeView`只有一个生产构造入口：

```text
AgentRun Product binding
  -> resolve CompleteAgentService
  -> CompleteAgentService::read(AgentReadQuery)
  -> project_authoritative_agent_view
  -> AgentRuntimeView
```

其它`AgentRuntimeView { ... }`字面量均位于contract或application测试。生产投影中的
`source_binding`固定为`None`；Product binding由
`AgentRunProductRuntimeViewObservation::Current { product_binding, view }`在application层组合。

未绑定与不可用也已由application层表达：

```text
AgentRunProductRuntimeViewObservation
  = Absent { requested_target }
  | Current { product_binding, view }
```

因此contract层不需要新增provisioning/attached Runtime view状态机。Product provisioning是
launch/fork/companion use case，不是source observation的一个分支。

## Identity / Revision Domain

| 坐标 | Owner | A0结论 |
| --- | --- | --- |
| `AgentSourceCoordinate` | Complete Agent | 保留，不进入browser contract |
| `RuntimeThreadId` | Product association | 保留，只作为Product安全wrapper坐标 |
| turn/item/interaction ID | concrete Agent history | Runtime view直接复用，不重建第二组ID |
| public effect / Runtime receipt handle | Complete Agent public effect | 已由`9797af179`收束为同值opaque handle |
| Agent snapshot/context revision | Complete Agent observation | observation/context共用同一revision |
| Product owner document revision | Product aggregate | 不进入`AgentRuntimeView` |
| Runtime live lane sequence | 当前连接 | 仅排序partial lane，不作为durable observation revision |
| surface revision | Complete Agent accepted surface | source/private evidence；Product binding只保存自己的frame intent |

## 重复事实清单

以下两组当前仍是同构事实，需要在A1删除Runtime副本：

- lifecycle；
- execution active turn / queued compaction / last compaction outcome；
- context coordinate；
- interaction request/status/resolution；
- authority/fidelity；
- turn/item/interaction identity；
- initial context package/contribution/provenance；
- input content block与interaction response。

`agent_snapshot_projection.rs`中的逐枚举映射、ID重建与serde transcode是这些重复事实的主要证据。

## Product-only内容

以下内容不属于canonical Agent observation：

- `RuntimeThreadId`；
- committed Product binding；
- launch frame与execution profile；
- Product provisioning/fork/activation receipt；
- terminal、workflow、gate等Product-owned effect；
- browser-safe source identity/thread-name provenance digest。

这些内容只能包裹或组合observation，不能覆写observation。

## A1约束

1. 一个contract crate，Complete Agent service与Product wrapper引用同一canonical facts。
2. `CompleteAgentSnapshot`携带source/private evidence；`AgentRuntimeView`只增加
   `RuntimeThreadId`与browser-safe presentation evidence。
3. application层继续表达Absent/Current与Product binding；contract层不新增Product状态机。
4. Runtime mapper只做source identity隐藏、validation和安全证据派生。
5. 同revision observation进入Product wrapper后逐值保持不变。

## Characterization Results

- `cargo test -p agentdash-agent-runtime --lib`：17/17。
- Native canonical projection focused tests：7/7，包含compaction
  succeeded/failed/lost/cancelled。
- frontend Runtime transport、Session reducer与compaction card：12/12。
- `pnpm run contracts:check`：全部生成合同无drift。
- 新增Runtime wrapper characterization，固定source identity不进入wire、revision/context坐标
  不变以及Product只增加`RuntimeThreadId`的当前行为。
