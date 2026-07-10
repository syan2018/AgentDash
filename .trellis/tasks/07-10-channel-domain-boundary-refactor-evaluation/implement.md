# Implement · Channel 术语与领域边界收敛

实施已完成。全量改名、领域收敛、migration 和残留清理由父任务 `work-items/` 统一追踪；最终提交按 decision/spec、ExtensionProtocol rename、Channel domain/admission、V2 persistence、binding provider、production composition、index consistency、residual cleanup 与 final evidence 分主题落地。

## 1. 工作项追踪

| ID | 工作项 | 依赖 |
| --- | --- | --- |
| WI-00 | 历史决策、规范与代码 residual 对账 | 无 |
| WI-01 | ExtensionProtocol 原子改名与 qualified identity | WI-00；Operation descriptor contract 稳定 |
| WI-02 | Channel domain identity、维度与 admission 收敛 | WI-00 |
| WI-03 | Owner-local persistence、migration 与 binding index | WI-02 |
| WI-04 | ChannelBindingProvider 与 delivery integration | WI-02、WI-03 |
| WI-05 | 全量集成、spec 与残留验证 | WI-01 至 WI-04 |

`work-items/README.md` 是状态与证据索引；每个 `WI-*.md` 维护写入范围、退出条件和验证结果。历史 tracker 的完成状态不能替代当前代码检查。

## 2. 全局实施清单

### WI-00 · 历史决策、规范与代码 residual 对账

- 延续已确认的多 owner 领域模型与 owner-local persistence；Project Channel 物理承载归 Project Assets。
- 对账 07-07 已归档任务、数据库规范和当前实现，建立 residual matrix。
- 明确 synthetic channel identity、绕过 ChannelService 的 runtime wake、service-level admission、capability directive 第二授权路径和 unsupported binding 产品路径。
- 对每个 `ChannelOwner` variant 记录真实产品需求、创建者、lifetime、query、store 和 binding resolution；无证据 variant 从目标模型删除。
- 独立 aggregate 只在跨 owner query、独立 retention/claim、不可重建 reverse index 或数据库唯一约束有真实需求时重新评审。

### WI-01 · ExtensionProtocol 原子改名与 qualified identity

- 将 manifest、domain、contracts、generated TS、SDK、Workspace Module、RuntimeGateway、relay/local host、example 与文档统一改为 `protocol/protocol_key/invoke_protocol`。
- 删除 `protocol_channels/channel_key/invoke_channel` 旧字段和旧入口，不提供双读或兼容 alias。
- 完整调用引用携带 provider extension/install identity、protocol key、method 与 contract version requirement，删除全局 key 首个命中。
- manifest authoring 是事实源，canonical Operation descriptor 是 actor-specific 调用投影；底层 protocol invoker 只保留 adapter provenance。
- migration 清理并重建 library/package/install manifest snapshots 与 artifact digests；owned fixtures/scripts 直接重建。

### WI-02 · Channel domain identity、维度与 admission 收敛

- 引入 owner-local unique `ChannelKey/ChannelLocator`，区分全局 `ChannelId`、稳定业务地址与 display/search aliases。
- 收束 canonical participant principal，删除同义 Agent/User/System variants。
- 删除混合 scope/transport/audience 的 `ChannelMedium`；binding 表达 transport/endpoint，owner 表达 authority/lifetime。
- 删除混合 cardinality/audience/thread 的 `ChannelTopology`；membership、delivery audience 和 thread relation 分别表达。
- 将 lifetime 与 retention 分离；拆分 message origin、reply target 与 correlation。
- publish/reply/broadcast 每次重新校验 open status、membership、operation、audience、ingress/egress 与 binding readiness。
- capability 只投影 actor 可见操作面；禁止 directive 暴露不存在或未授权的 ChannelRef。
- 所有 runtime wake、Companion/AgentRun/Interaction attention 必须解析真实 registry channel；不得制造 synthetic channel id。

### WI-03 · Owner-local persistence、migration 与 binding index

- LifecycleRun runtime Channel 继续使用 typed `channel_registry` owner document 与语义 mutation port。
- Project Channel 在 Project Assets 工作项落地前保持明确 owner store contract；本任务不发明临时 ProjectConfig 或独立表 fallback。
- 为 owner-local `ChannelKey` 实现原子 create-if-absent/unique validation，并验证 record owner 与 locator owner 一致。
- external binding inbound resolution 需要可重建或明确持久化的 reverse index；禁止扫描全部 owner documents。
- migration 覆盖 schema_version、key/identity、participant/binding/lifetime/retention 的结构变更；不保留旧结构 decoder。
- 若 evidence 证明必须独立 aggregate，先更新 PRD/design/spec 并获得用户确认，再迁移并删除旧 owner document 权威路径。

### WI-04 · ChannelBindingProvider 与 delivery integration

- 建立 `ChannelBindingProvider` SPI，覆盖 inbound normalize、identity/participant resolution、outbound publish/reply 和 delivery state。
- 以 internal/test provider 覆盖完整 ingress → policy → message → mailbox/gate/attention delivery。
- Extension/Integration 可同时贡献 OperationProvider 与 ChannelBindingProvider，但 ExtensionProtocol 不自动成为 Channel binding。
- Interaction/Operation refs 只作为 message content refs/correlation，不进入 Channel canonical state。

### WI-05 · 全量集成、spec 与残留验证

- Rust、TS SDK、manifest/contracts、relay/local、Workspace Module、frontend 文案、examples、handbook 与 generated contracts 同步。
- 更新 Channel、database、capability、runtime gateway、mailbox 与 cross-layer specs。
- repository-wide static scan 清除旧 Extension Channel 词汇、synthetic channel identity、绕过 service admission 和重复 authority refs。
- 运行 domain property tests、repository mutation/concurrency、migration forward、provider ingress/outbound、mailbox/gate materialization 与 frontend/contract checks。

## 3. 验证策略

- `cargo fmt --check`，受影响 package test/check/clippy。
- `pnpm run contracts:check`、Extension package validate/pack、host surface parity、frontend typecheck/focused tests。
- `pnpm run migration:guard`、干净 PostgreSQL migration、owner document concurrency/roundtrip。
- 静态检查旧 `protocol_channels/channel_key/invoke_channel`，但排除归档历史任务文档。
- 静态检查 `ChannelMessage::new` 的 channel id 均来自 registry/locator resolution。
- `git diff --check`。

## 4. Review Gate

- Channel 一词只属于通信领域；Extension authoring 使用 ExtensionProtocol，调用投影使用 Operation。
- owner-local persistence 基线与当前规范一致，任何独立 aggregate 升级都有真实不变量和用户确认。
- Workspace 双工交互只通过 refs/attention 与 Channel 连接，不把 command/event 并入 Channel。
- `work-items/` 完整、JSONL context 有效，PRD/design/implement 已完成 convergence 并获用户批准。
