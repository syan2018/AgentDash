# Extension SDK 开发体验落地执行计划

## Current Execution Scope

本任务从评估转为落地执行，首个 MVP 聚焦本地 preview harness 和 extension host bridge dispatcher。用户已要求直接进入执行，并亲自用浏览器验证开发调试流程。

## Implementation Checklist

- [x] 梳理 `packages/extension-dev` 的 CLI 入口，将 `dev` 从 esbuild watch 扩展为 dev supervisor。
- [x] 增加 Vite dev server，服务插件 panel 页面与 `__agentdash_preview` 虚拟页面，确保 examples 的 React panel 支持 HMR 与 sourcemap。
- [x] 增加 extension dev runtime loader，bundle 并加载 `src/extension.ts`，用 `createExtensionContext()` 激活 extension。
- [x] 实现 runtime action dispatcher，处理 `runtime.invoke_action`。
- [x] 实现 protocol channel dispatcher，处理 `extension.invoke_channel`、self channel 与 dependency alias 的本地解析。
- [x] 实现 preview scaffold，提供近似 WorkspacePanel 的 iframe 容器、mock metadata context 和 bridge request log。
- [x] 将 preview parent 与 iframe panel 的 `agentdash.extension` postMessage request/response 对齐到 `@agentdash/extension-ui` 当前 contract。
- [x] 为 Host API dev behavior 增加本地 dev mock / diagnostic，避免真实 Rust/local relay 能力缺失时页面崩溃。
- [x] 更新 `local-hello` / `protocol-demo` README 的开发命令说明。
- [x] 增加 extension-dev 单元测试覆盖 loader、dispatcher、channel 解析、preview bridge envelope 和 dev server smoke。
- [x] 使用浏览器打开至少一个 example 的 preview，验证渲染、bridge 调用和 request log。

## Validation Commands

```powershell
pnpm --filter @agentdash/extension-dev test
pnpm --filter @agentdash/extension-dev typecheck
pnpm --dir examples/extensions/local-hello run test
pnpm --dir examples/extensions/protocol-demo run test
pnpm --dir examples/extensions/local-hello run pack
pnpm --dir examples/extensions/protocol-demo run pack
```

开发态交互验收需要启动：

```powershell
pnpm --dir examples/extensions/protocol-demo run dev
```

并在 preview URL 中用浏览器验证 panel 调用、HMR、extension reload 与 request log。`local-hello` 可作为附加 smoke。

## Validation Evidence

- `pnpm --dir examples/extensions/protocol-demo run dev -- --host 127.0.0.1 --port 6200` 已启动本地 preview。
- in-app browser 打开 `http://127.0.0.1:6200/__agentdash_preview`，iframe 渲染 `protocol-demo` panel，点击 Run 后显示 Pure TS Action、Workspace API、Process API、Self Channel、Panel Channel 五组结果。
- 独立 Playwright 浏览器上下文再次打开同一 preview，点击 Run 后捕获 `missing: []`、`errors: []`，bridge log 包含 `runtime.invoke_action` 与 `extension.invoke_channel`。
- 浏览器验收截图：`browser-playwright-verification.png`。
- Spec maintenance review：本次新增的是 Extension SDK authoring dev harness，未改变 `.trellis/spec/` 中既有跨层/前端/后端架构不变量；稳定使用说明已落到 `docs/extension-system.md` 与 examples README。

## Risk Points

- `src/extension.ts` dev loader 需要避免 Node ESM cache 影响 reload；实现时应使用稳定的 cache-busting 或临时 bundle 路径。
- preview scaffold 需要保持 `extension-ui` bridge contract 一致，否则 examples 会出现 dev 与 packaged 行为分叉。
- protocol channel canonical key 与 dependency alias 解析必须贴合现有 local runner 的行为，否则 `protocol-demo.consume_demo_channel` 无法作为金线。
- `agentdash-ext pack` 的 artifact 输出不能被 dev server 配置污染；dev-only 状态应留在 dev supervisor 内。
