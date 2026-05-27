# Extension SDK 开发体验落地设计

## Evidence Summary

现有 extension 主链路已经相当完整，开发态实现应贴合它，而不是另建协议模型。

- `examples/extensions/local-hello` 证明最小 packaged extension 闭环：`src/extension.ts` 通过 `defineExtension()` 注册 runtime action，panel 通过 `@agentdash/extension-ui` 的 `bridge.invokeAction()` 调用，manifest 声明 webview entry 和 extension host bundle。
- `examples/extensions/protocol-demo` 证明完整 TS host 心智：用户自写 `DemoProtocolClient`，在 `ctx.channels.register()` 暴露 provider channel，在 `ctx.runtime.registerAction()` 暴露 runtime action，panel 同时调用 action 与 channel，extension host 内部通过 `ctx.api.channels.self()` 与 dependency alias 调用 channel。
- `packages/extension-sdk` 已有 authoring contract：manifest 类型、runtime action、protocol channel、dependency、Host API facade、permission declaration、`createExtensionContext()`。
- `packages/extension-ui` 已有 panel request/response bridge：panel 使用 `postMessage` 向父级发送 `runtime.invoke_action`、`extension.invoke_channel`、`metadata.get_context`、VFS 与 workspace tab 请求。
- `packages/extension-dev` 已有 validate / pack / install：`packProject()` 产出 self-contained `dist/extension.js` 与 `dist/panel/main.js`，更新 bundle digest 并创建 `.agentdash-extension.tgz`。
- 当前 `agentdash-ext dev` 只是 esbuild watch 到 `dist/`，没有提供 panel preview、HMR dev server、extension host 本地 dispatcher、bridge request log 或近似 WorkspacePanel 的调试壳。

## Product Boundary

首个开发态能力围绕“TS 前端与 TS 后端自通信”建立。这里的 TS 后端就是现有 extension authoring 模型里的 `src/extension.ts` 与其引用的协议 adapter / client / handler 模块。开发态只需要让 panel 能在本地通过同一套 `@agentdash/extension-ui` bridge 调到这些 handler。

真实 Project 安装、artifact 上传、local relay、Rust-backed workspace/process/env/local profile 能力已经由安装态和宿主 runtime 负责。开发态提供 metadata 与 dev diagnostic Host API，用于让纯 TS action/channel 和轻量演示跑起来；真实能力接入可以作为后续增强。

## Recommended Shape

`agentdash-ext dev` 升级为本地 dev supervisor：

1. 启动 Vite dev server，以插件项目根作为 `root`，让 `src/panel/index.html` 可作为真实 panel 页面加载，并保留 Vite HMR 与 sourcemap。
2. 在同一个 server 上提供 `__agentdash_preview` 虚拟 HTML，模拟 WorkspacePanel 的基本容器，并以 iframe 加载 panel entry。
3. 启动 extension dev runtime loader，用 esbuild 将 `src/extension.ts` bundle 到临时 dev 目录，动态导入后调用 `activate(ctx)`，收集 runtime action 与 protocol channel handlers。
4. preview scaffold 作为 iframe parent，监听 `agentdash.extension` postMessage 请求，并通过 HTTP endpoint 转发到 extension dev runtime dispatcher。
5. dispatcher 返回与 `extension-ui` bridge 兼容的 response envelope，让 panel 代码在 dev 与 packaged AgentDash panel 中保持同一调用方式。

建议命令输出：

```text
AgentDash extension dev ready

preview:  http://127.0.0.1:6200/__agentdash_preview
panel:    http://127.0.0.1:6200/src/panel/index.html
runtime:  local extension host dispatcher
```

## Bridge Dispatch

dev dispatcher 复用 `extension-ui` 当前 method 名称：

- `metadata.get_context` 返回固定 dev context：`project_id`、`session_id`、`extension_id`、`extension_key`、`panel_type_id`、`uri`。
- `runtime.invoke_action` 在已激活 extension contributions 中按 `action_key` 查找 handler 并执行。
- `extension.invoke_channel` 根据当前 extension key、channel key、method 和 dependency alias 解析到本地 channel handler。
- `workspace.open_tab` 可在 preview scaffold 内记录请求并展示调试日志。
- VFS / Host API 能力由后续 scope 决定；首个 MVP 可通过可配置 mock 或清晰 dev diagnostic 表达当前 dev context 没有真实宿主能力。

## Browser Validation

开发完成后必须启动 example 的 `pnpm run dev`，使用浏览器打开 `http://127.0.0.1:<port>/__agentdash_preview`。验收时检查：

- preview chrome 与 iframe panel 渲染出来。
- 点击 `protocol-demo` 的 Run 后，request log 至少记录 `runtime.invoke_action` 和 `extension.invoke_channel`。
- pure TS action、provider channel、自有 channel调用成功显示结果。
- workspace/process/env case 使用本地 dev mock 或清晰 diagnostic，并保留页面可操作。
- 通过截图或浏览器状态确认流程真实可见。

## Export Relationship

开发态不改变 `agentdash-ext pack` 的导出事实。packaged artifact 继续包含：

- `agentdash.extension.json`
- `package.json`
- `dist/extension.js`
- `dist/panel/**`

dev harness 只服务 authoring feedback loop；导出仍适配现有 AgentDash manifest、protocol channel、runtime action、webview asset 和 extension host bundle contract。

## Validation Focus

首轮实现以 examples 做金线：

- `local-hello` 在 preview 中能打开 panel，并能通过 dev metadata 或 mock local profile 完成一次 action 调用。
- `protocol-demo` 在 preview 中能调用 pure TS action、provider channel、self channel。依赖真实 workspace/process/env 的 case 用 dev diagnostic 或 mock 配置覆盖。
- panel 源码修改触发 HMR。
- extension host 源码修改后，后续 bridge 调用使用新 handler。
- `agentdash-ext pack` 输出行为保持不变。

## Implementation Decisions

- Dev Host API 默认行为：提供 metadata、local profile、HTTP fetch、memory workspace、env 和 process 的轻量本地实现；真实 Rust/local relay 不在本次开发态壳内模拟。
- 配置入口：先从现有 package / manifest / conventional paths 推断，避免新增 dev config 影响 authoring 心智。
- Runtime reload：使用 extension bundle cache-busting reload，后续 bridge 调用基于最新 `src/extension.ts` bundle。
