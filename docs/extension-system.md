# AgentDash 插件系统开发与使用

AgentDash extension 的核心模型是“平台管理运行边界，插件作者维护 TypeScript host”。插件包提供自包含的 `extension_host` bundle、runtime actions、protocol adapters、protocol channels 和 workspace panel；AgentDash 负责 Project 安装、artifact 校验、host lifecycle、内嵌能力 facade、Project/session/backend context、runtime projection、webview bridge 和 trace metadata。

这个边界让插件作者可以像写普通 TypeScript 模块一样组织协议 client、parser、缓存和业务 action。只有需要访问平台事实源或本机能力时，插件才通过 `ctx.api.*` facade 请求宿主。

## 插件项目结构

一个 extension 项目至少包含：

```text
agentdash.extension.json
package.json
src/
  extension.ts
```

常见完整项目会继续包含：

```text
src/
  protocol/
    demo-client.ts
  shared/
    schema.ts
  panel/
    index.html
    main.tsx
    App.tsx
```

`src/extension.ts` 是 author-owned TS host entry。`agentdash-ext pack` 会把它 bundle 成 `dist/extension.js`，并把 manifest 中的 `bundles[].digest` 写成实际 bundle 摘要。安装端只读取自包含 archive，不运行 `npm install`、`pnpm install` 或 package lifecycle script。

## Manifest

Manifest 是 Project 安装、runtime projection、诊断和审计的声明来源：

```jsonc
{
  "manifest_version": "2",
  "extension_id": "protocol-demo",
  "package": {
    "name": "@agentdash/example-protocol-demo",
    "version": "0.1.0"
  },
  "asset_version": "0.1.0",
  "runtime_actions": [{
    "action_key": "protocol-demo.greet",
    "kind": "session_runtime",
    "description": "Return a pure TypeScript greeting",
    "input_schema": true,
    "output_schema": true,
    "permissions": []
  }],
  "protocol_channels": [{
    "channel_key": "protocol-demo.api",
    "version": "1.0.0",
    "description": "Protocol Demo API channel",
    "methods": [{
      "name": "greet",
      "description": "Return a greeting",
      "input_schema": true,
      "output_schema": true,
      "permissions": []
    }]
  }],
  "extension_dependencies": [{
    "alias": "demo",
    "extension_id": "protocol-demo",
    "version": "^1.0.0",
    "channels": ["protocol-demo.api"]
  }]
}
```

`runtime_actions` 是 AgentDash runtime 可直接调用的 action surface。`protocol_channels` 是插件导出的 provider API surface。`extension_dependencies` 让 consumer 插件用业务 alias 表达依赖，而 runtime projection 和 trace 仍记录 canonical provider/channel key。

## Host APIs

Host API facade 是 TypeScript host 访问平台事实源和本机能力的稳定入口：

```ts
ctx.api.local.getProfile();
ctx.api.http.fetch(url, options);
ctx.api.workspace.readText(path);
ctx.api.workspace.writeText(path, content);
ctx.api.workspace.list(path);
ctx.api.workspace.stat(path);
ctx.api.env.get("TOKEN_NAME");
ctx.api.process.shell(command, options);
ctx.api.process.exec(command, args, options);
ctx.api.runtime.invoke(actionKey, input);
ctx.api.channels.invoke(channelKey, method, input);
```

`getProfile` 的实现位于 `agentdash-local` 的 TS Extension Host dispatcher。用户名来自本机后端读取到的本地运行环境，连同 platform、arch、backend、Project/session 和 workspace root 摘要一起返回。它是 built-in host capability，用来说明平台事实源如何通过 `ctx.api.local.getProfile()` 暴露给插件；它不是浏览器原生信道，也不是插件系统的能力上限。

HTTP、workspace/VFS、env 和 process/shell 也由 `agentdash-local` 执行。workspace/VFS 复用 workspace root/path safety helper；process/shell 按本机可信工具模型提供通用执行，同时记录 cwd、timeout、输出上限、exit code 和截断状态。权限声明用于安装摘要、依赖解析、可用性诊断和审计；运行时不再把顶层 capability 当成重复门禁，具体 action 或 channel method 通过 `permissions` 表达自己会使用的能力。

`ctx.api.runtime.invoke()` 用来调用当前 Project 已启用插件注册的 runtime action。RuntimeGateway 会把 Project 中可执行的 packaged extension host surface 预加载到本机 runner；同插件内调用可以直接路由，跨插件调用需要当前 action 或 channel method 声明 `runtime.invoke:<action_key>` 或 `runtime.invoke`。Runner 会限制嵌套 invocation depth，目标 action、consumer、backend、trace 和 invocation metadata 仍由平台记录。

## Runtime Actions

纯 TypeScript action 只需要注册 handler：

```ts
ctx.runtime.registerAction({
  action_key: "protocol-demo.greet",
  kind: "session_runtime",
  description: "Return a greeting",
  invoke(input) {
    return { message: `Hello, ${input.name ?? "AgentDash"}` };
  },
});
```

需要本机能力时，把协议 adapter 写成普通 TS 模块，再从 action handler 调用：

```ts
const client = new DemoProtocolClient(ctx.api);

ctx.runtime.registerAction({
  action_key: "protocol-demo.workspace_demo",
  kind: "session_runtime",
  description: "Write and read a workspace note",
  permissions: ["workspace.vfs.write", "workspace.vfs.read", "workspace.vfs.list"],
  invoke(input) {
    return client.inspectWorkspace(input);
  },
});
```

新增业务协议只改插件自己的 TS 代码和 manifest 声明。只有新增平台级 fact source 或本机能力时，才需要扩展 AgentDash Host API dispatcher。

## Protocol Channels

Protocol channel 是插件导出的可复用 API surface。Provider 在 TS host 中注册 method handler：

```ts
ctx.channels.register({
  channel_key: "api",
  version: "1.0.0",
  description: "Protocol Demo API channel",
  methods: {
    greet: {
      description: "Return a greeting",
      invoke(input) {
        return client.greet(input);
      },
    },
    runShell: {
      description: "Run a trusted shell command",
      permissions: ["process.execute"],
      invoke(input) {
        return client.runShell(input);
      },
    },
  },
});
```

Provider 可以在代码里使用短 key，例如 `api`。Runner 会按当前 extension scope 生成 canonical key，例如 `protocol-demo.api`。Manifest 和 runtime projection 使用 canonical key，方便 Project 级 discovery、冲突检测和审计。

同一个插件内部调用自己的 channel 使用 self shortcut：

```ts
const result = await ctx.api.channels
  .self("api")
  .invoke("greet", { name: "AgentDash" });
```

Consumer 插件通过 dependency alias 调用前置插件的 channel：

```jsonc
{
  "extension_dependencies": [{
    "alias": "gitlab",
    "extension_id": "gitlab-review",
    "version": "^1.0.0",
    "channels": ["gitlab-review.api"]
  }]
}
```

```ts
const mrs = await ctx.api.channels
  .from("gitlab")
  .invoke("listMergeRequests", { project: "agentdash" });
```

Canvas 后续作为 consumer 使用同一 binding 思路：Canvas 面向 Project runtime projection 中的 binding alias 调用 channel，AgentDash 在 runtime 层解析到 canonical provider extension/channel/method。alias 是 authoring 体验，canonical key 是路由和审计事实。

## Panel Bridge

Workspace panel 使用 `@agentdash/extension-ui`：

```ts
const bridge = createExtensionBridge();
const result = await bridge.invokeAction("protocol-demo.consume_demo_channel", {
  name: "AgentDash",
});
```

Panel request 只传 method 和 params。Project、session、backend、extension actor 和 trace context 由宿主 webview bridge 组装。Panel 可以调用 runtime action、provider channel、workspace VFS 与 workspace tab：

```ts
await bridge.invokeChannel("api", "greet", { name: "AgentDash" });
await bridge.vfs.write("notes/demo.txt", "hello");
const text = await bridge.vfs.read("notes/demo.txt");
await bridge.openWorkspaceTab("protocol-demo.panel", "protocol-demo://demo");
```

`bridge.events` 是 panel 内部的 local event bus，适合在同一个 webview 内解耦组件状态。workspace-level 或 extension-runtime-level event 需要平台事件路由时，应作为新的宿主 bridge contract 设计。

Canvas/panel 直接 channel bridge 使用同一 `extension.invoke_channel` contract 接入。Canvas 代码面向 binding/alias 调用，宿主按 Project runtime projection 解析到 canonical provider extension/channel/method。

## 本地开发流程

插件开发的最快反馈路径是 `agentdash-ext dev`。它会在插件项目内启动本地 Extension Preview，让 panel 继续使用 `@agentdash/extension-ui` 的真实 bridge contract，同时把请求交给本地加载的 `src/extension.ts` handler：

```powershell
pnpm --dir examples/extensions/protocol-demo run dev
```

启动成功后 CLI 会输出两个 URL：

```text
preview: http://127.0.0.1:6200/__agentdash_preview
panel:   http://127.0.0.1:6200/src/panel/index.html
runtime: local extension host dispatcher
```

日常开发优先打开 `preview`。Preview 页面提供接近 WorkspacePanel 的 iframe 容器和 bridge request log；iframe 内加载真实 panel 页面，父页面负责接收 `agentdash.extension` postMessage 请求，再通过本地 dispatcher 调用 extension host。这样 panel 代码在开发态和安装态使用同一套调用方式：

```ts
const bridge = createExtensionBridge();

await bridge.invokeAction("protocol-demo.greet", { name: "AgentDash" });
await bridge.invokeChannel("api", "greet", { name: "AgentDash" });
```

开发态 dispatcher 会 bundle 并加载 `src/extension.ts`，执行 `activate(ctx)`，收集 `ctx.runtime.registerAction()` 和 `ctx.channels.register()` 注册的 handler。Panel 发出的 `metadata.get_context`、`runtime.invoke_action`、`extension.invoke_channel` 会在同一个本地进程中完成路由；同插件 self channel 和 manifest 中声明的 dependency alias 也会按当前 extension scope 解析。

Panel 前端由 Vite 服务，支持 HMR 和 sourcemap。修改 `src/panel/**` 后浏览器会刷新对应模块；修改 `src/extension.ts` 或它引用的 TS 模块后，下一次 bridge 调用会重新加载 extension bundle，适合快速验证 TS panel 到 TS host 的自通信。

开发态提供轻量本地 Host API 行为，用来支撑 authoring loop：

- `metadata.get_context` 返回固定 dev Project/session/extension context。
- `ctx.api.local.getProfile()` 返回本地 dev profile 摘要。
- `ctx.api.workspace.*` 使用内存 workspace，适合验证读写/list/stat 交互形状。
- `ctx.api.env.get()` 和 `ctx.api.process.*` 使用当前本机 Node 进程能力。
- `ctx.api.http.*` 使用标准 `fetch`。

这些能力让纯 TS action、protocol channel、self/dependency channel 和常见 facade 调用可以在单一开发环境内跑通。真实 Project 安装、artifact 校验、local relay、Rust-backed workspace/process 审计和权限投影仍由安装态 runtime 负责。

推荐开发循环：

```powershell
pnpm --dir examples/extensions/protocol-demo run dev
# 在 preview 中调试 panel 和 bridge
pnpm --dir examples/extensions/protocol-demo run validate
pnpm --dir examples/extensions/protocol-demo run test
pnpm --dir examples/extensions/protocol-demo run pack
```

`protocol-demo` 是完整开发样例：点击 Preview 中的 Run 会同时验证 pure TS action、workspace/process facade、provider channel、self channel、dependency alias 和 panel channel bridge。`local-hello` 是最小样例，适合确认 manifest、panel 和 built-in Host API 的基础闭环。

## 打包、安装和试用

常用命令：

```powershell
pnpm --dir examples/extensions/protocol-demo run validate
pnpm --dir examples/extensions/protocol-demo run test
pnpm --dir examples/extensions/protocol-demo run pack
```

`agentdash-ext pack` 仍是导出事实源。它会生成自包含 `.agentdash-extension.tgz`，其中包含 manifest、`package.json`、`dist/extension.js` 和 `dist/panel/**`，并把 bundle digest 写回 manifest。安装态只读取这个 artifact。

安装到 Project：

```powershell
pnpm --dir examples/extensions/protocol-demo run agentdash:install -- --api-url http://127.0.0.1:3001 --project <project-id> --token <token> --overwrite
```

也可以在前端 Assets 页上传 `packed/*.agentdash-extension.tgz`，从归档安装到当前 Project，然后在 Session WorkspacePanel 打开 `Protocol Demo` tab。Panel 会通过 bridge 调用 action，action 再进入本机 TS host、Host API facade 和 self channel。

## 示例

- `examples/extensions/local-hello`：最小 built-in Host API 示例，展示 `ctx.api.local.getProfile()` 从本机 host 获取 profile。
- `examples/extensions/protocol-demo`：完整协议/channel 示例，展示纯 TS action、用户自写 protocol adapter、workspace/env/process/http facade、protocol channel provider、self-channel shortcut、dependency alias 和 panel action/channel 调用。

## Workspace Module

**Workspace Module 是项目工作空间里"可被 agent 与用户协作消费的能力模块"的统一认知单元。** 它把不同来源的协作能力归一成同一种 descriptor，让 agent 工具与项目设置页用同一份 projection 认知、列举、调用与裁切，避免每类来源各自一套 DTO 和调用约定。

### 来源（kind）与 module_id

Workspace Module 由三类来源聚合而成，各有稳定 id 前缀：

- **Extension**（`ext:{extension_key}`）：已安装并 enabled 的扩展。其 `runtime_actions` 与 `protocol_channels[].methods` 投影为 module 的 operations，`workspace_tabs` 投影为 ui_entries。
- **Canvas**（`canvas:{mount_id}`）：项目可见的 Canvas。投影为带 ui_entry（供 present/read）的 module；Canvas binding 是声明式数据引用而非可 invoke 的 RPC，本轮 canvas module 不暴露 invokable operation。
- **Builtin**（`builtin:{key}`）：平台内置能力，预留位（当前为空）。

聚合由单一 canonical 函数 `build_workspace_modules(ext_projection, canvases)` 完成（application `workspace_module` 层）。Agent 工具与 `GET /api/projects/{id}/workspace-modules`（项目设置页数据出口）共用它——**不存在第二份聚合或 DTO**。

### 与 Runtime Surface 的术语边界

- **Runtime Surface** 指 extension runtime projection 的底层分量：`runtime_actions`、`protocol_channels`、`workspace_tabs`、`bundles`、`permissions` 等。它是"扩展声明了什么 / 运行时暴露了什么"的低层事实，按扩展边界组织。
- **Workspace Module** 是建立在 Runtime Surface（及 Canvas / Builtin）之上的**协作认知层**：按"能力模块"而非"扩展内部结构"组织，统一 kind / status / operations / ui_entries / permission_summary 的表达，服务 agent 与项目管理面。

简言之：Runtime Surface 回答"扩展暴露了哪些底层 action/channel/tab"，Workspace Module 回答"工作空间里有哪些可协作模块、各自能做什么、是否可用"。代码中保留的 `Surface`/`surface` 命名专指底层 runtime projection，不与 Workspace Module 混用。

### 四工具（list / describe / invoke / present）

Agent 经四个元工具消费 Workspace Module（application `workspace_module/tools.rs`）：

- **list**：返回 `WorkspaceModuleSummary` 列表（kind / title / source / status / operation_summary / permission_summary，不含完整 schema）。
- **describe**：返回单个 `WorkspaceModuleDescriptor`（含每个 operation 的 input/output schema 与 ui_entries）。
- **invoke**：按 operation 的结构化 `dispatch` 分量直接派发——`RuntimeAction` 走 RuntimeGateway、`ProtocolChannel` 走 channel invoker、`Canvas` 以 canvas actor 派发、`Builtin` 预留。invoke 不字符串拆 `operation_key`，并把 provenance（operation 来源 / backend）与 runtime trace 落进结果 details，统一审计。
- **present**：把 module 的 ui_entry 呈现给会话用户（webview / canvas panel）。

### 可见性裁切与诊断

- **裁切**：workspace module 可见性经 capability 通道（`WorkspaceModuleDimension`）在 agent 侧生效——`mode=All` 放行全部，`mode=Allowlist` 仅放行 allowlist 内 module。声明式可见性事实源是 ProjectAgent preset（`visible_workspace_module_refs`），投影进 base `CapabilityState.workspace_module`，经 `effective_capability_json` 序列化/还原；清空白名单 → `mode=All`（回到全部可见）。能力维度模型、AccumulationPolicy 与各维度投影路径见 spec：[Capability Dimension Pipeline](../.trellis/spec/backend/capability/capability-dimension-pipeline.md)。项目设置页（ProjectAgent 配置）即编辑入口。
- **诊断**：当 extension runtime bundle 缺失时，对应 module 的 `status` 标为 `unavailable` 并携带 `reason`，list/describe 与项目设置页据此呈现诊断。
