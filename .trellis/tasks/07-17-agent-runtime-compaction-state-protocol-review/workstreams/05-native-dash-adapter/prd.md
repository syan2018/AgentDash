# W5 — Native / Dash Adapter

## Depends On

- W2 Dash Agent & AgentCore
- W3 Runtime State & Host Coordination
- W4 Surface / Tool / Hook

## Ownership

- `agentdash-integration-native-agent`
- Native/Dash adapter tests and composition

## Goal

让 Native 路径以完整 Dash Agent 身份实现 Complete Agent service，补全真实 history fork
和 Dash-owned compaction，移除空 history fork 与 adapter-produced Runtime facts。

## Exit Criteria

- Complete Agent service conformance 通过；
- initial package create/apply 映射 Dash history 并返回 applied digest/fidelity；
- 产品 fork 调用 Dash history fork，child 独立恢复；
- current fork 6/6 regression 保持通过；
- manual/automatic compaction 由 Dash Agent 执行；
- surface applied evidence 与 effect inspect 可验证；
- adapter 不拥有第二套 history/projection。
