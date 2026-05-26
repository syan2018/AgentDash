# TS Extension Host 与插件 SDK 闭环设计

## 设计立场

AgentDash 的插件体系应分成“平台宿主扩展”和“用户运行时扩展”两条线：

- Rust native plugin 继续服务高权限、启动期、管理员治理场景。
- TS extension host 服务用户可开发、项目可安装、本机可热更新的 runtime / UI / protocol extension。

两条线共享 Shared Library / Project Asset / session construction / RuntimeGateway / WorkspacePanel 这些平台事实源。这样用户获得近似 VS Code 的开发体验，同时 AgentDash 仍能保持多用户、多项目、cloud/local/frontend 三段式架构的审计与权限边界。

## 分层架构

```text
Extension author project
  @agentdash/extension-sdk
  @agentdash/extension-ui
  @agentdash/extension-dev
        |
        | validate / pack / install
        v
Shared Library extension_template
        |
        | install
        v
ProjectExtensionInstallation
        |
        | session construction
        v
ExtensionRuntimeProjection
        |
        +--------------------------+
        |                          |
        v                          v
Frontend WorkspacePanel      RuntimeGateway
dynamic tab registry         extension proxy provider
        |                          |
        | invoke(action_key)        | relay / local transport
        v                          v
sandbox renderer / panel      agentdash-local
                                   |
                                   v
                            TS Extension Host
```

## Extension Package Model

SDK 层面对插件作者暴露 `agentdash.extension.json` 或 code-first manifest。最终产物拆成两类事实：

1. **平台声明**：进入 `extension_template`，用于 Project install、session projection、前端菜单和权限展示。
2. **可执行/可渲染 bundle**：由 local extension store 或后端 asset 存储保存，供 `agentdash-local` 和前端 sandbox renderer 加载。

插件分发形态建议采用 `.agentdash-extension.tgz` 或等价 archive，而不是只依赖 native plugin embedded seed。Archive 内部结构：

```text
agentdash.extension.json
package.json
dist/
  extension.js
  panel/
    index.html
    assets/*.js
schemas/
  *.json
assets/
  icon.svg
```

### Dependency Model

插件不是单文件模型。开发期可以使用 npm 依赖，正式包以自包含 archive 交付：

- Extension Host 侧依赖优先通过 esbuild/tsup/rollup 等 bundler 打进 `dist/extension.js`。
- Webview UI 侧依赖按普通前端应用打进 `dist/panel/*` 静态资产。
- 不在用户安装阶段运行 `npm install` / `pnpm install` / `postinstall`，原因是安装时执行任意包管理脚本会扩大供应链和本机执行面。
- `dependencies` 中无法 bundle 的资源必须作为 archive files 显式包含，并记录 digest。
- `devDependencies` 只服务插件作者本地开发，不进入运行事实。
- Native Node addon、平台二进制、语言服务器等重量依赖必须显式声明 `runtime_requirements` 与 platform targets；MVP 可先只支持纯 JS / WASM / 静态二进制随包交付。

这与 VS Code 的思路相近：扩展开发者在发布 VSIX 前解析/打包依赖；用户安装的是一个已经包含运行所需文件的扩展包。VS Code 桌面 extension host 能运行 Node.js 代码，因此扩展可以包含 `node_modules` 或打包后的 JS；Web extension 则必须使用浏览器兼容 bundle。AgentDash 可以采用更收敛的规则：首版强制 pack 产物自包含，安装端只校验和解包。

安装来源分三类：

- **Packaged extension archive**：SDK 打包产物，MVP 主路径；安装后写入 Project extension installation，并把 bundle refs / digests 作为可执行事实。
- **Local dev ref**：`agentdash-ext dev/install --local` 使用本机路径，服务开发闭环；不作为团队共享的长期事实源。
- **Plugin embedded seed**：Rust native plugin 或 first-party plugin 在启动期贡献 extension_template，用于内置示例、企业预置和测试，不作为用户插件开发的唯一通道。

正式安装的 archive 权威存储放在后端 / Project asset 侧。安装流程记录 artifact digest、storage ref、source version 与 package metadata；`agentdash-local` 只在运行时下载、校验、解包并缓存。这样 Marketplace 安装、Project 迁移、团队协作与 source-status 都围绕同一事实源工作。

首版 runtime scope 采用 project-level extension。Project installation 是 session construction 的唯一运行时入口；Marketplace 可让同一插件快速安装到其它 Project。

建议首版保持 `ExtensionTemplatePayload` 作为 Project runtime 的主要声明，并新增字段：

```jsonc
{
  "manifest_version": "2",
  "extension_id": "gitlab-review",
  "package": {
    "name": "gitlab-review",
    "version": "0.1.0"
  },
  "commands": [],
  "flags": [],
  "message_renderers": [],
  "runtime_actions": [
    {
      "action_key": "gitlab-review.list_mrs",
      "kind": "session_runtime",
      "description": "List merge requests",
      "input_schema": {},
      "output_schema": {},
      "permissions": ["http:gitlab.example"]
    }
  ],
  "workspace_tabs": [
    {
      "type_id": "gitlab-review.mr-panel",
      "label": "GitLab MR",
      "uri_scheme": "gitlab-mr",
      "renderer": {
        "kind": "runtime_panel",
        "entry": "gitlab-review.mr-panel",
        "required_actions": ["gitlab-review.list_mrs"]
      }
    }
  ],
  "permissions": [
    { "kind": "http", "host": "gitlab.example" },
    { "kind": "vfs", "access": "read" }
  ],
  "bundles": [
    {
      "kind": "extension_host",
      "entry": "dist/extension.js",
      "digest": "sha256:..."
    },
    {
      "kind": "panel",
      "entry": "dist/panel.js",
      "digest": "sha256:..."
    }
  ],
  "capability_directives": [],
  "asset_refs": []
}
```

`LibraryAsset.version` 继续表达 asset version；`package.version` 只表达插件包自身版本，用于诊断和发布节奏，不替代 asset source version。

## SDK 包结构

### `@agentdash/extension-sdk`

运行在 TS Extension Host 中：

```ts
export default defineExtension((ctx) => {
  ctx.runtime.registerAction("gitlab-review.list_mrs", {
    input: schema,
    output: schema,
    permissions: ["http:gitlab.example"],
    async handler(input, api) {
      return api.http.fetchJson("https://gitlab.example/api/merge_requests");
    },
  });

  ctx.workspace.registerPanel({
    typeId: "gitlab-review.mr-panel",
    label: "GitLab MR",
    renderer: {
      kind: "webview",
      entry: "./panels/mr-panel.tsx",
    },
  });
});
```

Host API 不直接暴露 Node 全权限对象。文件、HTTP、process、env、runtime invoke 走 `api.*` facade，并由 local host 做权限裁决。

### `@agentdash/extension-ui`

运行在 panel/webview iframe 中：

```ts
const client = createAgentDashPanelClient();
const result = await client.invokeAction("gitlab-review.list_mrs", input);
client.openWorkspaceTab("vfs", "workspace://...");
```

UI SDK 只发送 action key、input、tab command 或 event request。actor/context/trace 由父页面与 API route 组装。

### `@agentdash/extension-dev`

CLI 职责：

- `init`：创建插件项目模板。
- `dev`：启动 watch、manifest preview、local extension host dev session。
- `validate`：校验 manifest、schemas、permissions、bundle refs、action key 格式。
- `pack`：生成 extension archive 与 manifest digest。
- `install`：将插件安装到目标 Project，写入 Project extension installation，并通知 local runtime reload。

## TS Extension Host

首版建议由 `agentdash-local` 管理一个 Node-based extension host 进程。每个插件可以在同一个 host worker 中隔离 module scope，后续再升级为 per-extension worker isolation。

通信协议建议使用 JSON-RPC over stdio 或本机 WebSocket：

```text
agentdash-local -> extension host
  initialize(host_info)
  activate(extension_id, manifest, config)
  deactivate(extension_id)
  invoke_action(extension_id, action_key, input, invocation_context)
  reload(extension_id)
  health()

extension host -> agentdash-local
  register_contributions(extension_id, contributions)
  log(extension_id, level, message)
  request_host_api(call)
  emit_event(event)
```

`agentdash-local` 是权限与本机事实源边界：

- 校验 plugin installation 是否属于当前 Project/session。
- 将 runtime invocation 绑定到 workspace root / backend id / project id。
- 过滤 HTTP/VFS/process/env 调用。
- 将执行日志、trace、错误归一到 RuntimeGateway result。

## RuntimeGateway Integration

云端 API 注册一个 `ExtensionRuntimeActionProvider` 或等价 proxy provider。它不直接执行 TS，而是根据 session/project 查找 extension action declaration，再通过 backend registry 路由到对应 `agentdash-local`。

调用路径：

```text
Panel iframe
  -> parent bridge invokeAction(action_key, input)
  -> API route
  -> RuntimeInvocationRequest(actor/context/trace由宿主生成)
  -> RuntimeGateway.invoke
  -> ExtensionRuntimeActionProvider
  -> BackendRegistry command
  -> agentdash-local
  -> TS Extension Host invoke_action
```

Provider 的 `supports` 必须校验：

- action key 属于当前 session project 的 enabled extension installation。
- action kind 与 RuntimeContext 匹配。
- 当前 actor 允许调用该 action。
- 本机 backend 在线且能运行该 extension。

## Session Projection and Frontend

`SessionContextResponse` 应新增 `extension_runtime`，或在 current runtime projection DTO 中提供同等字段。前端使用 mapper 完成 `unknown -> typed` 转换，并将 extension runtime 作为 WorkspacePanel 的单一输入之一。

WorkspacePanel 增加 dynamic tab registration：

- built-in tabs 继续通过 `registerBuiltinTabTypes()` 注册。
- extension tabs 从 `runtimeData.extensionRuntime.workspace_tabs` 派生。
- `typeId` 必须带 extension namespace，冲突时 projection 或 frontend registry fail-fast。
- tab layout 继续持久化 `type_id + uri + title + pinned`。
- 当 extension 被禁用或 unavailable，已保存 tab 渲染 unavailable state，而不是静默替换成其它 tab。

首版 renderer 主路径应是 `webview`：插件贡献自己的前端 bundle，AgentDash 在 WorkspacePanel 中提供 sandboxed iframe / webview 容器，并注入 `@agentdash/extension-ui` bridge。插件 UI 负责绘制体验；AgentDash 负责加载、隔离、尺寸/生命周期、权限展示、bridge 消息路由与后端调用审计。

`runtime_panel` 和 `canvas_panel` 仍然有价值，但定位不同：

- `runtime_panel`：用于开发诊断、快速验证 action schema、无自定义 UI 的轻量插件。
- `canvas_panel`：用于 Canvas 转插件和已有 Canvas runtime preview 复用。
- `webview`：用于真实用户自定义 UI，是插件面板能力的默认目标。

Webview bridge 只允许发送平台命令，不暴露主前端内部对象：

```text
panel iframe
  -> postMessage bridge
  -> WorkspacePanel host
  -> AgentDash API / RuntimeGateway / VFS service
```

Bridge command baseline：

- `runtime.invokeAction(action_key, input)`
- `workspace.openTab(type_id, uri)`
- `vfs.read(uri)` / `vfs.write(uri, content)`
- `events.subscribe(topic)` / `events.emit(event)`
- `metadata.getContext()`

## Canvas Promote to Extension

Canvas 转插件可以作为第一批验证样本：

```text
Canvas
  files / entry_file / sandbox_config / bindings
  runtime_bridge required actions
      |
      v
ExtensionTemplatePayload
  workspace_tabs: [{ renderer.kind = "canvas_panel" }]
  bundles/assets: canvas files
  runtime_actions/capability_directives: derived from bridge usage
```

产物仍走 Shared Library publish/install 流程。安装到 Project 后，session projection 暴露 workspace tab，前端打开 tab 后用 Canvas runtime panel 运行。

## Demo Extension Project

需要提供一个完整、独立、可调试的 demo extension project，建议路径：

```text
examples/
  extensions/
    local-hello/
      package.json
      agentdash.extension.json
      src/
        extension.ts
        panel/
          main.tsx
          App.tsx
        shared/
          schema.ts
      tests/
        extension.test.ts
      README.md
```

它在仓库中作为 SDK consumer 存在，但结构上模拟外部插件仓库。首版可以通过 workspace/file dependency 指向本仓 SDK 包；发布态文档展示如何切换成 registry dependency。

`local-hello` 的 runtime action：

```ts
ctx.runtime.registerAction("local-hello.profile", {
  permissions: ["local.profile.read"],
  async handler(_input, api) {
    return api.local.getProfile();
  },
});
```

`api.local.getProfile()` 由 `agentdash-local` 提供受限 facade，返回可展示但低敏的信息：

- username 或 display name。
- platform / arch。
- backend id。
- project id / session id 摘要。
- workspace root basename 或 redacted path。

Demo webview panel 使用 `@agentdash/extension-ui` 调用 `local-hello.profile`，展示结果和错误状态。它必须经过真实 `pack -> install -> session projection -> WorkspacePanel webview -> RuntimeGateway -> agentdash-local -> TS Extension Host` 链路，而不是直接 mock API。

Demo 的验收以 packaged archive 为准：

- `agentdash-ext pack` 生成 `.agentdash-extension.tgz`、manifest、digest。
- 平台接收 archive artifact，并保存 storage ref / digest / package metadata。
- Project installation 引用该 artifact，而不是引用 demo 源码目录。
- `agentdash-local` 从平台下载 archive、校验 digest、解包到运行缓存。
- 新打开或刷新后的 session 能从 projection 看到 `local-hello` tab contribution。
- WorkspacePanel 打开 packaged webview bundle 后，通过 bridge 调用 packaged extension host action。

Local dev ref 只验证开发体验，不算平台级发布验收通过。

## Database and Migration Shape

`project_extension_installations` 已有 `manifest JSONB` 与 `config JSONB`，首版可以优先通过 manifest 扩展承载声明。若需要管理 bundle 实体，建议新增专门表或资产记录：

- `extension_packages`：package identity、version、digest、storage ref。
- `project_extension_runtime_bindings`：project installation 到 local/runtime bundle 的绑定状态。

所有 schema 变化必须通过 migration；Shared Library typed validator、contract generation、frontend mapper 同步更新。

## Operational Notes

- `pnpm dev` 会同时拉起云端后端、本机后端和前端。TS Extension Host dev mode 应能接入这条链路，避免插件开发者手动拼多个服务。
- Rust 后端无法热重载；Extension Host reload 应限定在 TS extension worker 和 local runtime projection refresh，不依赖重启云端主服务。
- 插件 action 与 panel 加载失败应展示 extension unavailable / action unavailable 状态，并保留诊断信息给开发者。

## Trade-offs

- **TS host vs Rust native plugin**：TS host 更适合用户闭环开发和前端共享类型；Rust native plugin 更适合高权限宿主能力。
- **schema-driven panel vs webview**：webview 是插件自定义 UI 的主路径，schema-driven 只承担诊断、轻量插件或 fallback。选择 webview 会让 MVP 更大，但它直接验证用户真正需要的“自定义 UI + SDK 信道”闭环。
- **bundle 存后端 vs local store**：正式安装以后端 / Project asset 侧为权威，利于项目迁移、团队共享、审计和 source-status；local store 只用于 dev mode 与运行缓存。代价是 MVP 需要 artifact 上传、存储、下载与 digest 校验链路。

## Proposed MVP

推荐首个 MVP：

1. 扩展 `extension_template` schema，新增 runtime actions 与 workspace tabs。
2. `/sessions/{id}/context` 暴露 `extension_runtime`。
3. 前端 WorkspacePanel 支持 dynamic tab，首版 renderer 使用 sandboxed webview，并提供 `@agentdash/extension-ui` bridge。
4. TS SDK 支持 defineExtension、registerAction、registerPanel、validate、pack。
5. `agentdash-local` 拉起最小 TS Extension Host，支持 invoke action。
6. RuntimeGateway 增加 extension proxy provider。
7. 后端保存 packaged extension archive artifact，Project installation 引用 artifact digest/storage ref。
8. `extension-dev pack` 产出自包含 extension host bundle 与 webview bundle，不要求安装端执行依赖安装。
9. 提供 `examples/extensions/local-hello/`，作为真实 SDK consumer，自定义 panel bundle 调用本机 TS action 并展示本机 profile。

Canvas promote 可以作为 MVP 后半段或第二阶段。它适合验证现有 Canvas runtime 如何包装成 extension，但不替代 webview 作为用户自定义 UI 主路径。
