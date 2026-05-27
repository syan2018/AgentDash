# Local TS Extension Host

> 父任务：`05-26-ts-extension-host-sdk`
> 状态：planning

## Goal

让 `agentdash-local` 管理 TypeScript Extension Host，支持 project-level extension activation、reload、action invocation 和受限本机 SDK facade。

## Requirements

- `agentdash-local` 启动/停止 extension host 进程或 worker。
- 支持 activate/deactivate/reload/health。
- 支持 invoke action，并把结果和错误归一给 relay / RuntimeGateway。
- 支持 `api.local.getProfile()`，返回 username、platform、backend id、session/project 摘要、workspace root 摘要。
- Host API 必须权限化，不直接把 Node/OS 全权限暴露给插件。
- 支持 dev mode 加载 `examples/extensions/local-hello` 源码目录。
- 支持 packaged mode 从 artifact cache 加载 extension bundle。

## Acceptance Criteria

- [ ] `local-hello.profile` 能在 local TS host 中执行并返回本机 profile。
- [ ] reload 后 action handler 更新生效。
- [ ] 权限未声明时 local API 调用被拒绝。
- [ ] packaged artifact cache 加载路径可用。
- [ ] local host 异常不会拖垮 `agentdash-local` 主进程。

## Out of Scope

- 不实现 RuntimeGateway provider。
- 不实现 webview UI。
