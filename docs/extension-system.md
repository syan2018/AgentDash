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

## 打包、安装和试用

常用命令：

```powershell
pnpm --dir examples/extensions/protocol-demo run dev
pnpm --dir examples/extensions/protocol-demo run validate
pnpm --dir examples/extensions/protocol-demo run test
pnpm --dir examples/extensions/protocol-demo run pack
```

`agentdash-ext dev` 启动本地 Extension Preview：Vite 提供 panel HMR 与 sourcemap，preview scaffold 作为 iframe parent 复用 `agentdash.extension` bridge contract，并把 `runtime.invoke_action` / `extension.invoke_channel` 请求路由到本地加载的 `src/extension.ts`。这个开发态用于快速验证 TS panel 与 TS extension host 的自通信，导出和安装仍以 packaged artifact 为事实源。

安装到 Project：

```powershell
pnpm --dir examples/extensions/protocol-demo run agentdash:install -- --api-url http://127.0.0.1:3001 --project <project-id> --token <token> --overwrite
```

也可以在前端 Assets 页上传 `packed/*.agentdash-extension.tgz`，从归档安装到当前 Project，然后在 Session WorkspacePanel 打开 `Protocol Demo` tab。Panel 会通过 bridge 调用 action，action 再进入本机 TS host、Host API facade 和 self channel。

## 示例

- `examples/extensions/local-hello`：最小 built-in Host API 示例，展示 `ctx.api.local.getProfile()` 从本机 host 获取 profile。
- `examples/extensions/protocol-demo`：完整协议/channel 示例，展示纯 TS action、用户自写 protocol adapter、workspace/env/process/http facade、protocol channel provider、self-channel shortcut、dependency alias 和 panel action/channel 调用。
