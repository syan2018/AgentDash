# Extension Runtime 跨层契约

> 父任务：`05-26-ts-extension-host-sdk`
> 状态：planning

## Goal

扩展 AgentDash 的 extension runtime 契约，让 Project extension installation 能声明 runtime actions、workspace tabs、permissions、bundles，并通过 session construction 投影给前端和 RuntimeGateway。

## Requirements

- 扩展 `ExtensionTemplatePayload`，新增 `runtime_actions`、`workspace_tabs`、`permissions`、`bundles`。
- 对 action key、workspace tab type id、uri scheme、renderer kind、permission shape、bundle digest 做 typed validation。
- Project extension installation 继续作为 session construction 读取的唯一运行时事实源。
- `/sessions/{id}/context` 或等价 runtime projection DTO 暴露 `extension_runtime` 新字段。
- 前端 generated contracts / mapper 能消费 extension runtime projection，不直接信任 raw JSON。
- 冲突策略 fail-fast：同一 Project 的 enabled extensions 不允许重复 action key、workspace tab type id、uri scheme。
- 保持 native plugin embedded seed 与 external packaged extension archive 共用同一 extension template schema。

## Acceptance Criteria

- [ ] Domain validator 拒绝非法 action key、tab type id、renderer、permission、bundle digest。
- [ ] Session construction flatten enabled Project extension installations 时包含 runtime actions、workspace tabs、permissions、bundle refs。
- [ ] Session context DTO 暴露 extension runtime projection，前端 mapper 覆盖空值与非法 shape。
- [ ] Contract generation / check 通过。
- [ ] 测试覆盖 Project 内 extension contribution 冲突。

## Out of Scope

- 不实现 archive artifact 上传/下载。
- 不实现 RuntimeGateway provider。
- 不实现 WorkspacePanel webview 渲染。
