# WorkspacePanel Extension Webview

> 父任务：`05-26-ts-extension-host-sdk`
> 状态：planning

## Goal

让前端 WorkspacePanel 能根据 session `extension_runtime` 注册动态插件 tab，并在 sandboxed webview 中加载用户自定义 UI bundle。

## Requirements

- `WorkspaceRuntimeData` 接收 `extension_runtime`。
- 从 projection 生成动态 `TabTypeDescriptor`。
- `AddTabMenu` 能展示插件 tab。
- `workspaceTabStore` 能保存/恢复 plugin tab 的 `type_id + uri`。
- Webview iframe 使用受控 bridge 调用 `@agentdash/extension-ui` 命令。
- 插件 disabled、action missing、backend offline、bundle missing 时有 unavailable state。
- 不把主前端 token、store、internal API client 暴露给 webview。

## Acceptance Criteria

- [ ] `local-hello` tab 出现在 `+` 菜单。
- [ ] 打开 webview panel 后能通过 bridge 调 `local-hello.profile`。
- [ ] 刷新页面后 plugin tab layout 恢复。
- [ ] 插件不可用时显示诊断状态。
- [ ] Frontend tests 覆盖 registry lifecycle 与 bridge message validation。

## Out of Scope

- 不实现 TS Extension Host。
- 不实现 artifact storage。
