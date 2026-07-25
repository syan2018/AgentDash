# W5 Native Complete Agent production registration component

## Identity

- Frozen base: `fc26d3ffb951461d8e9214b6b4639b88c18d533d`
- Code component:
  `agentdash_integration_native_agent::native_complete_agent_registration`
- Consumer/deletion manifest: `consumer-manifest.json`

该组件把 production composition 所需的 instance identity、Dash execution dependencies、
typed `AgentHostCallbacks`、`DashCompleteAgentStore` 与 `DashAgentCompleteService` 打包为一个
Complete Agent registration。它不直接依赖 Host/数据库实现，也不会自行修改 production
registry。

## Frozen behavior

- Create 安装完整 `InitialAgentContextPackage`，package 与 materialized evidence 保持独立；
- Fork 使用请求指定的 Dash history cutoff，保留 parent/cutoff/digest，绝不构造空 source
  binding；
- ApplySurface 将 callback route、binding generation、source、turn/item/effect identity 与
  deadline materialize 为 typed tool/hook callbacks；BeforeTool/AfterTool 支持
  allow、deny、rewrite input/result，duplicate effect 不重复触发；
- create/resume/fork/execute/apply/revoke receipts 与 inspect 全部进入
  `DashCompleteAgentStore` durable ledger；`DashCompleteAtomicCommit` 把 effect
  receipt/inspection 与 `DashCompleteSourceMutation` 的 repository/metadata 变更放入同一
  CAS transaction，跨服务实例从同一 store 恢复；
- live surface 始终由 durable source metadata 的单一 materializer 重建；apply/revoke
  即使 durable commit 成功后响应丢失，同实例 replay 也会应用新 binding generation 或
  清除旧 callbacks；
- manual/automatic compaction、read、ordered change page 与 inspect 全部由同一
  `DashAgentService` history/effect authority 提供；
- Native legacy driver、journal/context projector、presentation/tool route、旧 driver
  tests 与 Main oracle fixture 已由 W5 owner component 物理删除；
- registration 仅作为 W8 composition 输入；W8 只负责 PostgreSQL store 与 production
  composition，禁止 production 双注册。

## Activation boundary

Native owner deletion 已在本组件完成。W7 负责清除 Infrastructure worker 与 API/Product
对已删 legacy symbols 的 consumers；W8 实现 `DashCompleteAgentStore` /
`DashAgentRepositoryStore` PostgreSQL adapter，把 registration 注册到唯一
`CompleteAgentHost`，并删除 test-support session-parity 中已经零 consumer 的 Native
golden 引用。在 W7 caller 和 W8 durable composition 尚未同时进入 staging set 前，本组件
不单独激活。
