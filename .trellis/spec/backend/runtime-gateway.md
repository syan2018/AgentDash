# Runtime Gateway

Runtime Gateway 是 application-facing typed execution seam。AgentRun 使用具名 `AgentRunRuntime` facade；其他平台 action使用各自具名 gateway。Gateway 不暴露 Driver、Integration factory、placement transport或vendor DTO。

## Agent Runtime Path

```text
Application product command
  -> AgentRunRuntime facade
  -> AgentRuntimeGateway execute/snapshot/events
  -> Managed Runtime
  -> Integration Driver Host
```

- product coordinate只解析为 `AgentRunRuntimeBinding`；不存在字符串 connector/executor分支。
- extension/Canvas/VFS调用从 `run_id + agent_id` 获取canonical binding与Business Surface resource facts。
- command availability、stale guard与typed unsupported在Driver副作用前验证。
- Gateway implementation无持久状态；operation/snapshot/events由Managed Runtime repository持有。
- Remote placement走RuntimeWire，不能经generic Backbone/JSON command transport。

必须测试无binding、stale guard、unsupported、duplicate operation、cross-project authorization与remote Lost。
