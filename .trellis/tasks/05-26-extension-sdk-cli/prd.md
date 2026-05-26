# Extension SDK 与开发 CLI

> 父任务：`05-26-ts-extension-host-sdk`
> 状态：planning

## Goal

新增面向插件作者的 TypeScript SDK 与开发 CLI，让插件能在独立目录中开发、验证、打包、安装。

## Requirements

- 新增 `@agentdash/extension-sdk`：extension host 侧 API。
- 新增 `@agentdash/extension-ui`：webview panel bridge API。
- 新增 `@agentdash/extension-dev`：`init`、`dev`、`validate`、`pack`、`install` CLI。
- `pack` 产出自包含 archive，不要求安装端执行依赖安装。
- `validate` 校验 manifest、action keys、workspace tabs、permissions、bundle refs、native dependency constraints。
- `install` 上传 packaged archive 并写入 Project extension installation。
- SDK 需要支持 `local-hello` demo 作为真实 consumer。

## Acceptance Criteria

- [ ] SDK packages 可独立 typecheck/test。
- [ ] `agentdash-ext pack` 生成 `.agentdash-extension.tgz`、manifest、digest。
- [ ] `agentdash-ext validate` 能发现非法 manifest / 非自包含依赖。
- [ ] `agentdash-ext install` 能调用平台 API 上传 archive 并安装到 Project。
- [ ] `examples/extensions/local-hello` 只通过 SDK/CLI 使用平台能力。

## Out of Scope

- 不实现后端 artifact storage。
- 不实现 local TS host。
- 不实现 webview host 容器。
