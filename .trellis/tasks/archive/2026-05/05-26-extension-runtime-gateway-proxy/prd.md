# Extension RuntimeGateway Proxy

> 父任务：`05-26-ts-extension-host-sdk`
> 状态：planning

## Goal

让插件 runtime action 通过 AgentDash `RuntimeGateway` 统一调用，并路由到对应 `agentdash-local` TS Extension Host。

## Requirements

- 新增 extension runtime provider 或 proxy provider。
- Provider 依据 session/project enabled extension projection 判断 action 是否可见。
- Gateway actor/context/trace 继续由宿主组装，panel 只传 `action_key + input`。
- Relay protocol 增加 extension action invoke command/response。
- 错误映射到 Gateway error categories。
- Invocation trace 包含 extension id、action key、backend id。

## Acceptance Criteria

- [ ] `RuntimeGateway.invoke` 可调用 `local-hello.profile`。
- [ ] 未安装/未启用/无权限 action 被拒绝。
- [ ] backend offline 返回可诊断错误。
- [ ] trace 与 output metadata 包含 extension identity。
- [ ] Tests 覆盖 provider supports 与 relay response mapping。

## Out of Scope

- 不实现 TS host 内部 action handler。
- 不实现 webview bridge。
