# WI-00 Decision / Residual Reconciliation

Status: done

Depends On: none

## Scope

- 对账 07-07 archived task、database/capability/mailbox specs 与当前代码。
- 建立 residual matrix：synthetic channel identity、runtime wake bypass、service admission、directive 第二授权、unsupported binding。
- 核实 owner variant evidence 与 cross-task Operation dependency。

## Exit Criteria

- 既有 owner-local persistence 决策没有被静默推翻。
- 每个 residual 有 owning work item、write set 和 verification。
- Story/System 等无证据 owner 有明确删除或保留结论。

## Residual Matrix

| Residual | Current Evidence | Owning Work Item | Closure Evidence |
| --- | --- | --- | --- |
| Extension provider contract 使用 `ProtocolChannel/channel_key/invoke_channel` | manifest、Rust domain/contracts、TS SDK、Gateway、relay/local host 与 examples 均存在完整旧词汇链 | WI-01 | 原子改名并执行 repository-wide static scan |
| Extension protocol resolution 可能按非 qualified key 选取 provider | Gateway invoker 仍承担 provider 查找，调用引用没有稳定表达 install identity 与 contract version requirement | WI-01 | provider-qualified resolver 与冲突/版本测试 |
| Channel domain 混合 medium/topology/lifecycle 维度 | `Channel` 同时保存 `ChannelMedium`、`ChannelTopology`、`ChannelLifecycle` | WI-02 | final domain serde/property tests |
| Stable business lookup 依赖 aliases | `ChannelRef` 只有 owner/id，aliases 无 owner-local uniqueness | WI-02、WI-03 | `ChannelKey/ChannelLocator` 与 create-if-absent repository tests |
| Broadcast admission 不完整 | `plan_broadcast_deliveries` 读取 record 后直接按 participant 生成 delivery，缺少统一 sender/status/operation/policy gate | WI-02 | service admission matrix |
| Capability directive 可直接制造 visible ref | runtime capability replay 接收 `ChannelDirective::Expose` 并写入 `visible_channels` | WI-02 | registry-backed projection tests 与旧 directive scan |
| Hook auto-resume 制造 synthetic channel identity | mailbox delivery 使用 terminal effect UUID 构造 `ChannelMessage.channel_id` | WI-02、WI-04 | 真实 registry locator resolution 与 mailbox wake tests |
| Production binding resolver 只有 unsupported | Companion composition root 注入 `UnsupportedChannelBindingResolver` | WI-03、WI-04 | reverse index + internal binding provider integration tests |
| Owner enum 与 storage evidence 不一致 | LifecycleRun store 已落地；Project 有明确资产需求；Story/System 没有独立创建者、生命周期和 store | WI-02 | 目标 enum 只保留 `Project/LifecycleRun`，domain scan/tests |

## Owner Evidence Gate

| Owner | Decision | Reason |
| --- | --- | --- |
| `LifecycleRun` | 保留 | runtime Channel 随 run 生灭，已有 typed registry 与原子 mutation store。 |
| `Project` | 保留 | Project 公共 Channel 是明确产品资产；物理承载由 Project Assets 实现。 |
| `Story` | 删除 | 当前没有独立创建入口、生命周期、查询或 owner store；Story 关系可由 Project Channel metadata/ref 表达。 |
| `System` | 删除 | 平台消息来源由 participant/origin 表达，不能替代明确的持久化 owner。 |

## Cross-Task Boundary

- Workspace V1 的 canonical Operation descriptor 已冻结 provider provenance 扩展点。
- WI-01 只负责 `ExtensionProtocol` backing provenance；Operation 的执行核心、Interaction 和 Canvas 生命周期由 Workspace 父任务负责。
- Channel 与 Workspace 只通过 `OperationExecutionRef`、`InteractionRef`、attention/correlation refs 集成。

## Validation

- `rg` ChannelService/ChannelMessage/ChannelDirective/ChannelOwner usages。
- 归档 task acceptance 与当前代码逐项对照。

Completed evidence:

- 复核归档 `07-07-channel-communication-capability-model` PRD/design/WI-08/WI-10。
- 复核 database/capability/runtime-gateway/mailbox specs 与当前 domain/application/runtime wake 实现。
- `rg` 已定位 Extension 旧词汇、synthetic id、directive projection、broadcast admission 和 unsupported resolver 的真实写入点。
