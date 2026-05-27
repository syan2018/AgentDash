# 插件系统当前状态与原始预期证据

## 原始预期

- `.trellis/tasks/archive/2026-05/05-26-ts-extension-host-sdk/prd.md` 定义的目标是面向用户和项目的 TypeScript 插件开发闭环：插件作者在独立仓库中使用 SDK 开发、调试、打包插件，安装后获得 runtime action、workspace panel、命令、渲染器与可审计能力声明。
- 同一 PRD 明确用户价值是“像开发 VS Code extension 一样，在独立环境里完成后端协议与前端面板的闭环开发”，并且本机能力由 `agentdash-local` 与 TS Extension Host 承载。
- `design.md` 给出的 `gitlab-review.list_mrs` 示例显示，用户插件应能在 TS action handler 中通过 `api.http.fetchJson` 这类 facade 自己实现业务协议适配。
- `design.md` 的 TS Extension Host 协议包含 `request_host_api(call)`，意图是插件 host bundle 与 `agentdash-local` 之间有稳定内部协议；插件作者写 TS action/protocol，平台维护 wire protocol、权限和审计边界。
- `local-hello` 的 `api.local.getProfile()` 被原设计定位为受限 built-in facade 示例，不是插件系统的唯一能力模式。

## 当前实现事实

- `packages/extension-sdk/src/index.ts` 目前只暴露 `api.runtime.invoke()` 与 `api.local.getProfile()`；还没有 `api.http`、`api.vfs`、`api.env`、`api.process` 等原规划中的受控 host capability。
- `crates/agentdash-local/src/extensions/host/runner.rs` 内嵌 JS runner，当前只把 `ctx.api.runtime.invoke` 映射到 `runtime.invoke`，把 `ctx.api.local.getProfile` 映射到 `local.get_profile`。
- `crates/agentdash-local/src/extensions/host/permissions.rs` 的 `resolve_host_api` 只处理 `local.get_profile`，未知 host api 直接拒绝。
- `crates/agentdash-domain/src/shared_library/value_objects.rs` 已有 `ExtensionTemplatePayload`、runtime actions、workspace tabs、permissions、bundles，以及 `local.profile.read` 双层权限裁决 helper，但 permission vocabulary 尚未覆盖 HTTP/VFS/env/process。
- `packages/extension-ui/src/index.ts` 已声明 panel bridge 的 `invokeAction`、`openWorkspaceTab`、VFS、event、metadata API；`packages/app-web/src/features/extension-runtime/ui/ExtensionWebviewPanel.tsx` 当前只实际处理 metadata、workspace.open_tab、runtime.invoke_action，VFS bridge 仍返回“尚未接入”。
- `packages/extension-dev/src/pack.js` 已能把 `src/extension.ts` 打成 `dist/extension.js`，并把 panel bundle 一并打包；但 CLI/manifest validation 仍以当前窄权限模型为准。
- `examples/extensions/local-hello` 已验证 packaged archive、Project 安装、WorkspacePanel webview、RuntimeGateway、local TS host 的端到端链路，但示例只有 `getProfile`，不足以说明用户自写 TS host/action/protocol 的完整模型。

## 规划含义

当前实现应视为最小闭环，不是原始预期的完成态。本任务需要把 Extension Host 收口为“平台管理 host lifecycle 与受控 capability，插件作者提供自有 TS host bundle 与业务协议层”的完整支持，而不是继续在 Rust runner 中为每个 demo 增加硬编码方法。

## 用户确认的补充边界

- “自己顶 TS host”采用推荐答案：平台管理 per-extension worker/process，插件提供自己的 host bundle、action 和协议模块。
- 插件各自的协议/API 信道原则上独立注册。
- 其它插件可以依赖前置插件提供的信道完成访问。
- Canvas 后续也应能自由使用插件提供的 API 信道；这里的“自由使用”应落在 Project/session context、依赖声明、权限裁决和 trace 审计之内，而不是为 Canvas 或某个插件写硬编码 bridge。
- 插件闭合开发需要调用自有 host/channel 的糖：作者不应为了调自己的 channel 反复写自己的插件名。规划采用 self-scope shortcut、dependency alias 和 Canvas binding alias；底层仍记录 canonical channel key。
- process/shell 能力按本机可信工具定位处理，短期不做高安全性权限设计。首版应支持通用 shell/process 调用，同时保留 cwd、timeout、输出上限、exit code 和 trace 这些工程护栏。
- 过度设计的权限门禁可以清理。权限声明应服务安装摘要、依赖解析、可用性诊断和审计，不为可信本机工具堆叠没有产品价值的 deny path。
- 新增示例采用独立 `examples/extensions/protocol-demo`，让 `local-hello` 保持最小 Host API 示例定位。
