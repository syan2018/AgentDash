# Extension Package Artifact 存储与分发

> 父任务：`05-26-ts-extension-host-sdk`
> 状态：planning

## Goal

实现 packaged extension archive 的平台权威存储，让 `.agentdash-extension.tgz` 能被上传、校验、保存、安装到 Project，并由 `agentdash-local` 下载、校验、解包运行。

## Requirements

- 正式安装以平台 artifact 为权威事实源，本机 local store 只做 dev mode 与运行缓存。
- 支持 archive upload，保存 storage ref、digest、package metadata、manifest snapshot、source version。
- Project extension installation 引用 artifact digest/storage ref，不引用源码目录。
- 下载 API 要做 Project 权限校验。
- `agentdash-local` 下载后必须校验 digest，解包到可清理的本机 cache。
- 安装端不得执行 `npm install` / `pnpm install` / package lifecycle scripts。

## Acceptance Criteria

- [ ] 后端能保存 extension archive artifact 与 digest。
- [ ] Project installation 能引用 packaged artifact。
- [ ] 后端拒绝 digest 不匹配或 manifest 不合法的 archive。
- [ ] local runtime 能下载、校验、解包 artifact。
- [ ] 测试覆盖上传、安装引用、下载鉴权、digest mismatch。

## Out of Scope

- 不实现 SDK pack。
- 不实现 TS Extension Host 执行。
- 不实现 Marketplace 远程分发。
