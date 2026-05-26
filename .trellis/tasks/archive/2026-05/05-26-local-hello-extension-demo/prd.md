# Local Hello Extension Demo 金线

> 父任务：`05-26-ts-extension-host-sdk`
> 状态：planning

## Goal

创建 `examples/extensions/local-hello/` 独立 demo extension project，并用 packaged archive 在 AgentDash 平台中验证完整插件闭环。

## Requirements

- Demo 目录像真实第三方插件项目：独立 `package.json`、manifest、extension host 入口、webview UI、tests、README。
- Demo 在目录内可运行 `dev`、`validate`、`pack`。
- Packaged archive 被上传到平台 artifact storage 并安装到 Project。
- 安装后不依赖 local dev ref 或源码目录。
- Webview 调用 `local-hello.profile` action，显示 username/platform/backend/session 摘要。
- E2E 覆盖 pack -> install -> projection -> open panel -> invoke action。

## Acceptance Criteria

- [ ] `examples/extensions/local-hello` 可独立开发和打包。
- [ ] Packaged archive install 到 Project 后可用。
- [ ] WorkspacePanel 能打开 demo tab。
- [ ] Panel 显示由 local TS host 返回的 profile。
- [ ] E2E 不依赖源码路径直连。

## Out of Scope

- 不承担 SDK/host/gateway/webview 的基础实现，只作为集成验收消费者。
