# Implement · Channel 术语与领域边界收敛

本文件是后续重构切片，不代表本评估任务已经进入实现。

## 1. 实施顺序

### C1 · ExtensionProtocol 原子改名

- 将 manifest、domain、contracts、generated TS、SDK、Workspace Module dispatch、RuntimeGateway、relay/local host protocol 与示例统一改为 `protocol/protocol_key/invoke_protocol`。
- 删除 `protocol_channels/channel_key/invoke_channel` 旧字段和旧入口，不提供双读或兼容 alias。
- package validation、surface parity、typed client 与 diagnostics 同步更新。
- dispatch 改为显式 provider identity + protocol key + method + contract version requirement，删除全局 key 首个命中。
- migration 清理并重建 library/package/install manifest snapshots 与 artifact digests；owned Canvas/fixtures 中的旧脚本直接重建。

验收：仓库中除全局通信模块及明确历史 task 文档外，不再以 Channel 命名 request/response provider contract。

### C2 · Operation 调用面收束

- 将 ExtensionProtocol method 生成到 canonical Operation descriptor。
- Browser/Canvas/component 只消费 actor-specific Operation surface；ExtensionProtocolInvoker 降为内部 dispatch adapter。
- 保留 provenance，使 trace 能定位 extension package、protocol 与 method。

验收：Agent 与 UI 对同一 method 使用同一 schema、visibility、capability admission 和 trace 主链路。

### C3 · 全局 Channel 领域正交化

- 确认 canonical principal identity，收束重复 participant variants。
- 删除 `ChannelMedium`，把 transport/endpoint 放入 binding，把 scope 放入 owner。
- 删除混合 cardinality/audience/thread 的 `ChannelTopology`，以 membership、delivery policy 与 thread relation 分别表达。
- 将 lifecycle 与 retention 分离。
- 拆分 message origin、reply target 与 correlation。
- 引入 owner-local unique `ChannelKey/ChannelLocator`，明确 ChannelId、ChannelRef 与 aliases 的约束。
- 关闭 capability directive 绕开 registry membership 的第二授权路径，并补齐 service-level admission。

验收：每个字段只回答一个领域问题，domain validation 能直接表达唯一性和生命周期不变量。

### C4 · persistence 与 migration

- 先完成 Project/Story/System owner 的真实消费者、lifetime、query、binding 与 transaction evidence matrix。
- 若只有 LifecycleRun 成立：删除其它 owner，升级 owner-local registry schema，迁移/reset JSONB 并补齐 owner/key/admission validation。
- 若多 owner 成立：建立独立 Channel aggregate repository，将 registry 展开到新表，校验 owner/ID/policy/delivery refs 后删除旧 JSONB column。
- exact ChannelRef 随选定 persistence 只保留一种 authority 形状；更新 test support 与 repository contract tests。

验收：领域声明的每个 owner 都有真实 store 与可验证 use case；未实现 owner 不留在 enum 中，不再通过 owner enum 猜 store 实现。

### C5 · binding provider 与端到端验证

- 建立 `ChannelBindingProvider` SPI，替换默认 unsupported resolver 的产品路径。
- 以一个 internal/test provider 覆盖 inbound normalize、participant resolution、policy、mailbox materialization、reply/publish 与 delivery state。
- 验证 interaction/operation refs 仅作为 message content refs/correlation，不改变 Channel 事务边界。

## 2. 主要影响面

- Rust domain/contracts/application/runtime gateway/relay/local host
- `packages/extension` authoring、host、browser 与 toolchain generator
- Workspace Module operation projection
- Canvas/Extension frontend bridge 与诊断文案
- generated TypeScript contracts、examples、handbook
- PostgreSQL migration、repository 与 integration tests

## 3. 验证策略

- repository-wide `rg` 确认旧 Extension Channel 词汇被清除。
- Extension package validate/pack 与 host surface parity tests。
- Workspace Module describe/invoke 和 RuntimeGateway actor/capability tests。
- Channel domain property/validation tests。
- migration forward test 与 repository concurrency test。
- provider inbound/outbound、mailbox/gate/outbox materialization integration tests。

## 4. 任务依赖

- C1 可先于 Workspace 双工交互任务实施。
- C2 的 canonical Operation descriptor 应与 `OperationProgram` 设计共同定稿。
- C3 可先推进；C4 必须通过 owner evidence gate。Interaction attention/handoff 接入前完成唯一的 ChannelRef 与 persistence 方案。
