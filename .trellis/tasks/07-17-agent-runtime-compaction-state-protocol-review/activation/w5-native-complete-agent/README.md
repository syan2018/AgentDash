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
  `DashCompleteAgentStore` durable ledger；跨服务实例从同一 store 恢复；
- manual/automatic compaction、read、ordered change page 与 inspect 全部由同一
  `DashAgentService` history/effect authority 提供；
- Native-owned typed projector 恢复 provider transcript，不从 journal JSON 同构转码；
- registration 仅作为 W8 composition 输入；旧 driver registration 删除与新 Complete
  Agent registration 必须在同一 commit，禁止 production 双注册。

## Activation boundary

当前 frozen revision 的 production Host 仍选择 legacy driver route。W8 消费本组件时必须
按 manifest 删除 Native-owned driver/journal/tool 路径，并把 registration 注册到唯一
`CompleteAgentHost`，同时实现 `DashCompleteAgentStore` 与 `DashAgentRepositoryStore`
PostgreSQL adapter。在 W7 caller 和 W8 durable Host/Dash repositories 尚未同时进入
staging set 前，本组件不单独激活。
