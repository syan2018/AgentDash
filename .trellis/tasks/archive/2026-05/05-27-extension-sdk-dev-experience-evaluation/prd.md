# Extension SDK 开发体验落地

## Goal

实现 Extension SDK 的开发态体验，让现有完整的 AgentDash extension authoring / packaging / runtime bridge 模型获得一个轻量本地调试壳。

本任务的目标不是重做 extension 协议，而是围绕当前 examples 已经证明的能力，补齐“TS 前端 panel 与 TS 后端 extension host 自通信”的独立开发体验：开发者可以在不 pack、不上传、不安装到 Project 的情况下，快速预览接近真实 WorkspacePanel 的渲染效果，并调通 `@agentdash/extension-ui` panel bridge 到 `src/extension.ts` 注册的 runtime action / protocol channel。

## Requirements

- 实现必须以现有实现为基线：`local-hello` 和 `protocol-demo` 已经证明 packaged extension、panel webview、runtime action、protocol channel、self/dependency channel shortcut、built-in Host API facade、manifest validation、pack/install 的主链路可用。
- 开发态必须复用现有 authoring 心智：插件作者继续维护 `src/extension.ts` 作为 TS 后端入口，继续在 panel 中使用 `@agentdash/extension-ui` bridge，继续通过 `agentdash-ext pack` 导出 AgentDash artifact。
- 开发态需要提供一个本地 preview harness，近似真实 panel 的尺寸、背景、iframe 边界和 bridge 行为，让需要快速查看前端效果的人无需进入完整 AgentDash Project。
- 本地 bridge harness 需要能加载 extension entry、执行 `activate(ctx)`、收集 `ctx.runtime.registerAction()` 和 `ctx.channels.register()` 的 handler，并响应 panel 发出的 `runtime.invoke_action`、`extension.invoke_channel`、`metadata.get_context` 请求。
- 本地开发体验需要支持 panel HMR / sourcemap，以及 extension host 侧变更后的 reload 或重启，使 TS 前端与 TS 后端的自通信反馈足够短。
- 真实 AgentDash 导出路径仍以当前 manifest、package artifact、extension host bundle、webview bundle 为事实源；开发态只是 authoring layer，不改变安装态 runtime contract。
- 开发态 Host API 对真实 Rust/local relay 能力可以先提供明确诊断；必须至少让纯 TS action/channel、panel metadata 和 examples 中可 mock 的自通信路径可运行。
- 必须亲自启动 dev 流程，并使用浏览器验证 preview 中的 panel 渲染、bridge 调用和 request log。

## Acceptance Criteria

- [x] `agentdash-ext dev` 启动本地 preview harness，输出 preview URL，并保持进程运行。
- [x] preview 页面提供接近 WorkspacePanel 的 iframe 容器、panel toolbar/context 摘要、bridge request log。
- [x] preview iframe 加载 examples 的 panel 页面，panel 仍使用 `@agentdash/extension-ui` 当前 bridge contract。
- [x] dev dispatcher 能加载 `src/extension.ts`，执行 `activate(ctx)`，并处理 `metadata.get_context`、`runtime.invoke_action`、`extension.invoke_channel`。
- [x] `protocol-demo` 在 preview 中能通过浏览器点击运行 pure TS action、provider channel 和 self/dependency channel 演示；workspace/process/env 通过本地 dev mock 运行，不造成 preview 崩溃。
- [x] panel 源码支持 Vite HMR/sourcemap；extension host 源码变化后后续 bridge 调用使用自动 reload 生效。
- [x] `agentdash-ext pack`、validate、install 的既有导出行为不被 dev harness 改变。
- [x] 增加自动化测试覆盖 dev runtime dispatcher / preview bridge envelope / CLI dev server smoke 能力。
- [x] 使用浏览器实际打开 preview URL，验证渲染、bridge 调用与 request log。

## Notes

- 2026-05-27 evidence review 已运行：
  - `pnpm --dir examples/extensions/local-hello run test`
  - `pnpm --dir examples/extensions/protocol-demo run test`
  - `pnpm --filter @agentdash/extension-dev test`
- 2026-05-27 implementation closure 已使用 in-app browser 和独立 Playwright 浏览器验证 `protocol-demo` preview：Run 后 action/channel/self/dependency alias 均返回结果，bridge request log 正常记录，独立浏览器上下文捕获 `errors: []`。
