# Channel 术语与领域边界收敛

## Goal

评估并收敛项目内两套 `Channel` 语言：Extension / Workspace Module 所消费的版本化 protocol 调用面，以及全局通信领域 `Channel`。形成可执行的重构建议，使“稳定调用契约”与“稳定通信空间”各自拥有清晰的身份、生命周期、权限和运行边界，并为后续 Workspace Module 通用交互系统提供无歧义的基础词汇。

本任务已完成目标模型评审并进入正式实施；完整代码改造在同一个父任务内推进，由 `work-items/` 统一跟踪依赖、状态、写入范围和验收证据。

## Background

- Extension manifest 以 `protocol_channels[].channel_key + methods` 声明 provider API，并被 Workspace Module 投影为 operation。
- 全局 `agentdash-domain::channel` 已包含 owner、participant、binding、message、delivery、capability 等通信语义。
- 两者都提供稳定、可寻址的交互入口，但稳定对象分别是“版本化调用契约”和“有状态通信关系”。
- 项目尚未上线，重构应追求最终正确模型，不保留旧命名兼容层；若修改持久化结构，需要明确 migration 处理。

当前审计已确认：Extension 侧是 typed request/response service contract，应收束为 `ExtensionProtocol` 并投影为 `Operation`；全局 Channel 保留通信语义，但需要拆分 medium/topology/lifecycle 等交叉维度，补齐稳定 `ChannelKey`、admission 与多 owner persistence。

持久化延续 07-07 Channel 任务已经确认并写入数据库规范的基线：领域模型允许有真实产品需求的多 owner，runtime Channel 进入 owner-local `ChannelRegistryDocument`，Project Channel 的物理承载由 Project Assets 收束。独立 aggregate 不是默认目标，只有跨 owner 查询、独立 retention/claim、跨 owner binding reverse index 或数据库唯一约束等真实不变量需要时才引入。

## Requirements

- R1：完成两套 Channel 从 domain、application、contracts、relay、SDK、Workspace Module、前端和文档的全链路影响面审计。
- R2：按身份、owner、生命周期、拓扑、transport/binding、participant、消息/投递、权限/capability、trace 和持久化逐维比较，明确哪些概念可复用、哪些必须隔离。
- R3：判断 Extension Protocol Channel 应重命名、重定位还是与全局 Channel 建立 adapter 关系，并给出一致的目标词汇与字段映射。
- R4：审查全局 Channel 自身的维度正交性，包括 `ChannelMedium`、`ChannelLifecycle`、`ChannelRef`、aliases、`ChannelAddress`、participant identity 和 owner store routing。
- R5：明确 Extension / Integration 如何为全局 Channel 贡献 provider adapter、binding transport 或 normalization，而不让调用协议冒充通信空间。
- R6：说明全局 Channel 与 Workspace Module、Canvas、MCP、Agent capability 以及拟议通用双工交互系统的边界。
- R7：输出按依赖顺序排列的原子重构方案、迁移影响、验证方式和风险点，不设计兼容/回退路径。
- R8：复核 07-07 已归档任务的 residual closure，特别是 synthetic channel identity、绕过 `ChannelService` 的 runtime wake、service-level admission 和 capability directive 第二授权路径；不能把既有 tracker 的完成状态当成当前代码已满足目标不变量的证据。
- R9：父任务内建立 `work-items/`，所有实施步骤、依赖、状态、检查证据和设计回退都在该目录统一管理。

## Acceptance Criteria

- [ ] 两套 Channel 的当前事实源、调用链和消费者都有代码或规范证据。
- [ ] 给出明确的保留、改名、删除和新增概念清单，并解释每项选择对应的领域不变量。
- [ ] 全局 Channel 的稳定身份、生命周期和 transport 维度形成正交目标模型。
- [ ] Extension provider API 与全局 Channel adapter 的关系可被具体数据流验证。
- [ ] 重构计划覆盖 Rust、TS SDK、manifest/contracts、relay、Workspace Module、前端文案、文档与数据库 migration。
- [x] 规划产物经过用户评审并已进入实现。
- [ ] 既有 owner-local persistence 决策与当前规范完成对账；任何推翻都明确记录新证据、被替代规范和 migration 影响。
- [ ] `work-items/README.md` 能追踪全部实施项，并且每个工作项都有依赖、退出条件和验证方式。

## Out of Scope

- 本任务不实现具体企业 IM provider。
- Project Channel 的资产物理模型由 Project Assets 设计承接。
- 完整 Channel message event log 与跨 owner 长期审计存储在出现真实查询和保留需求后单独设计。
