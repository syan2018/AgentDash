# 工作项 2：云端 latest release / update endpoint

## Goal

云端服务端在运行期通过配置的 stable manifest HTTP URL 读取桌面 latest manifest，校验并短缓存后，向桌面端暴露统一的 latest release / update endpoint。

## Scope

- 新增或扩展云端 update endpoint。
- 通过运行期配置读取 stable latest manifest HTTP URL。
- 服务端不持有对象存储 AK/SK，不读取 bucket listing。
- 服务端 build artifact 不内嵌桌面 `latest.json`。
- 未配置 stable manifest URL 时返回可诊断的未配置状态，不影响本地调试。
- `min_desktop_version` 只来自显式服务端运行环境配置；未配置时不触发强制更新。

## Deliverables

- latest release / update endpoint 响应 contract。
- stable manifest HTTP fetch、schema validation、短缓存和错误诊断。
- discovery/version 响应中明确最低版本与推荐版本语义。
- 单元测试覆盖 configured / unconfigured / manifest invalid / fetch failed / min version 显式配置。

## Checkpoints

- [ ] 发布新桌面 latest manifest 后，不需要重构服务端即可返回新版本。
- [ ] 未配置 stable manifest URL 时，更新 endpoint 返回可诊断状态，不阻断本地调试。
- [ ] stable manifest 拉取失败或 schema 错误时，错误可诊断。
- [ ] 未显式配置最低桌面版本时，不会用服务端版本或 latest version 推导强制更新。
- [ ] 服务端不需要对象存储凭据。
- [ ] endpoint 能按 platform、arch、current_version 返回桌面端可消费信息。

## Suggested Validation

- 相关 Rust 单元测试。
- `pnpm run backend:check`
- 针对 release info / update endpoint 的 contract 测试。
