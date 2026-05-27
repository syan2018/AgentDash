# 插件系统完整能力对齐技术设计

## Architecture

插件系统分为四层，每层只拥有自己的事实源：

1. **Extension author project**
   - 插件作者维护 `agentdash.extension.json`、`src/extension.ts`、业务协议模块、panel UI、测试与 README。
   - `src/extension.ts` 是 author-owned TS host entry；它注册 runtime actions、protocol channels、workspace panels、commands、flags 和权限声明。
   - 普通业务协议由插件 TS 代码实现，例如 `src/protocol/gitlab.ts`、`src/protocol/local-tool.ts`，不需要平台新增 Rust API。

2. **Extension package and Project installation**
   - `extension-dev pack` 产出 `.agentdash-extension.tgz`，包含 manifest、`dist/extension.js`、panel bundle 与静态文件。
   - 后端 Project artifact 是正式安装事实源，Project extension installation 指向 package artifact / manifest digest / archive digest。
   - Shared Library `ExtensionTemplatePayload` 继续表达平台声明和 Marketplace 模板；packaged artifact 安装继续使用 Project-scoped artifact。

3. **AgentDash runtime control plane**
   - RuntimeGateway extension provider 根据 Project installation 暴露 action surface 与 protocol channel surface，并在 invocation 阶段做 admission。
   - Gateway 不执行 TS bundle，只把 provider/consumer identity、action/channel key、project/session、backend target、trace 和 package artifact 发送给 local backend。
   - 前端 WorkspacePanel 从 Project extension runtime projection 注册 dynamic tab，webview panel 通过 `@agentdash/extension-ui` bridge 访问宿主能力。
   - Canvas runtime bridge 后续以 consumer 身份调用 Project 中已授权的 extension protocol channel，不为具体插件写硬编码 adapter。

4. **Local TS Extension Host**
   - `agentdash-local` 是 supervisor、cache、workspace root 和本机事实源边界。
   - Node-based TS Extension Host runner 加载插件 `extension_host` bundle，执行 `activate`，保存 contributions、action handlers 和 channel handlers。
   - Runner 通过内部 host protocol 向 Rust 请求 built-in capabilities；Rust 侧统一 registry/dispatcher 做参数校验、必要边界检查、执行和错误规范化。

## TS Host Ownership Model

“插件自己顶 TS host”在本任务中定义为：

- 插件包提供自己的 `extension_host` entry 和所有业务协议代码。
- 平台提供 runner/supervisor，并可升级为 per-extension worker 或 per-extension process。
- 插件不直接拥有 `agentdash-local <-> runner` wire protocol，也不自行监听本机端口作为平台控制面的一部分。
- 插件需要长期连接、外部服务 client、缓存、parser、protocol adapter 时，都放在自己的 TS bundle 内，由 action handler 调用。
- 用户已确认采用平台管理 per-extension worker/process 的推荐边界；插件不声明或管理独立 AgentDash 控制面进程。

推荐执行形态：

```text
agentdash-local
  -> ExtensionHostSupervisor
      -> ExtensionWorker(extension_key, package_digest)
          -> AgentDash runner runtime
              -> import dist/extension.js
              -> extension.activate(ctx)
              -> action.invoke(input, invocation_context)
              -> channel.invoke(method, input, invocation_context)
```

第一阶段可以仍使用一个 Node 子进程，但内部代码结构按 per-extension worker/process 设计，避免共享全局状态成为未来拆分阻力。

## Extension Protocol Channels

Protocol channel 是插件向 Project runtime 注册的可复用 API 信道。它和 runtime action 的区别是：

- runtime action 面向 AgentDash runtime surface 中的一个可直接调用能力；
- protocol channel 面向其它 extension、Canvas 或 panel，是 provider 插件导出的协议/API surface；
- channel handler 仍运行在 provider 插件的 TS host bundle 中，consumer 只能通过平台 SDK/bridge 调用。

目标 SDK 形态：

```ts
ctx.channels.register({
  channel_key: "gitlab.api",
  version: "1.0.0",
  description: "GitLab API channel",
  methods: {
    listMergeRequests: {
      input_schema,
      output_schema,
      permissions: ["http.fetch:gitlab.example", "env.read:GITLAB_TOKEN"],
      async invoke(input) {
        return gitlabClient.listMergeRequests(input.project);
      },
    },
  },
});

const result = await ctx.api.channels.invoke(
  "gitlab.api",
  "listMergeRequests",
  { project: "agentdash" },
);
```

调用链：

```text
provider extension activate
  -> ctx.channels.register(...)
  -> Project extension runtime projection includes channel surface
consumer extension action / Canvas bridge / panel bridge
  -> invoke channel_key + method + input
  -> RuntimeGateway admission checks consumer, provider, dependency, availability boundaries
  -> local backend routes to provider extension host worker
  -> provider channel handler runs inside provider TS bundle
  -> result returns with provider/consumer trace metadata
```

Manifest 需要同时表达 provider 声明与 consumer 依赖：

```jsonc
{
  "protocol_channels": [
    {
      "channel_key": "gitlab.api",
      "version": "1.0.0",
      "methods": [
        {
          "name": "listMergeRequests",
          "input_schema": true,
          "output_schema": true,
          "permissions": ["http.fetch:gitlab.example", "env.read:GITLAB_TOKEN"]
        }
      ]
    }
  ],
  "extension_dependencies": [
    {
      "extension_id": "gitlab-connector",
      "version": "^1.0.0",
      "channels": ["gitlab.api"]
    }
  ],
  "permissions": [
    { "kind": "extension_channel", "channel_key": "gitlab.api", "methods": ["listMergeRequests"] }
  ]
}
```

### Channel Authoring Sugar

为了让插件闭合开发更顺，SDK 提供两层糖，但所有 runtime 事实仍回到 canonical key：

1. **Self-scope channel key**
   - Provider 可以注册短 key：`ctx.channels.register({ channel_key: "api", ... })`。
   - SDK/pack 阶段把它 canonicalize 为 `{extension_id}.api` 或 `{extension_key}.api`，projection 和 trace 只记录 canonical key。
   - 同一插件内部调用自己的 channel 可以用 `ctx.api.channels.self("api").invoke("method", input)`，不需要写 `local-hello.api`。

2. **Dependency alias**
   - Consumer manifest 给依赖信道起业务 alias，例如 `gitlab`。
   - Consumer 代码调用 `ctx.api.channels.from("gitlab").invoke("listMergeRequests", input)`。
   - Gateway admission 根据 alias 解析到 canonical provider extension/channel，并在 trace 中同时记录 alias 与 canonical key。

3. **Canvas binding alias**
   - Canvas package/runtime bridge 也使用 binding alias，例如 `extensions.gitlab.invoke("listMergeRequests", input)` 或等价 bridge method。
   - Canvas 代码不硬编码 provider extension key；Project runtime projection 负责把 binding alias 解析到 enabled extension channel。

这些糖只影响 authoring ergonomics。权限、依赖、路由、不可用状态和审计仍以 canonical provider extension/channel/method 为准。

规则：

- channel key 必须带 provider namespace，避免不同插件冲突。
- provider channel 的 method schema 和 permission summary 进入 runtime projection。
- consumer 必须声明 dependency 或 binding；usage permission 只在它能提供安装摘要、诊断或审计价值时保留。
- Project 安装时可以允许缺依赖但标记 unavailable，也可以阻止启用；第一版推荐安装允许、runtime projection 明确 unavailable reason。
- 循环调用必须被 trace depth / invocation graph 限制，避免 plugin A -> plugin B -> plugin A 无限递归。
- Consumer 不获得 provider 内部 token、HTTP client、env value 或 JS object；所有访问经过 serialized JSON invocation。
- Canvas 使用同一 channel invocation contract，只是 consumer actor 是 Canvas/runtime bridge，而不是 extension action。

## Host API Registry

当前 `resolve_host_api(match method)` 需要演进成 registry：

```text
HostApiRegistry
  local.get_profile
  runtime.invoke
  extension.channel_invoke
  http.fetch
  vfs.read_text
  vfs.write_text
  vfs.list
  env.get
  process.run
```

每个 entry 包含：

- method name
- typed params decoder
- required extension capability when retained
- required action permission when retained
- execution function
- audit metadata builder
- error mapping

权限语义采用“最低有用门禁”：

```text
project installation and package artifact define what can exist
dependency/binding declarations define who may call which provider surface
workspace/backend/session context defines where calls may execute
capability/action declarations are retained only when they help install summary, diagnostics, or audit
```

`local.profile.read` 可以保留作为内建能力示例，但本任务应清理不提供产品价值的重复 deny path。新增 permission vocabulary 推荐保持简单：

```text
http.fetch:<host-or-pattern>
workspace.vfs.read
workspace.vfs.write
workspace.vfs.list
env.read:<name-or-prefix>
process.run:<command-key>
runtime.invoke:<action-key-or-prefix>
extension.channel.invoke:<channel-key>.<method>
```

Manifest 顶层 declaration 使用结构化 JSON，action permission 使用稳定 string key；仅对真正需要展示、诊断或审计的能力建模。Domain 层提供统一 evaluator，避免每个 host API 自己解析，也避免在不同层堆叠互相重复的权限门禁。

## Built-in Capability Contracts

### Local Profile

- SDK: `ctx.api.local.getProfile()`
- Host method: `local.get_profile`
- Fact source: `agentdash-local` activation profile
- Permission: top-level `{ kind: "local_profile", access: "read" }` + action `local.profile.read`
- Purpose: 展示 built-in host fact source，不作为用户协议层示例。

### HTTP

- SDK: `ctx.api.http.fetchJson(url, options)` and `ctx.api.http.fetch(url, options)`
- Host method: `http.fetch`
- Fact source: local backend outbound network
- Permission: optional top-level host declaration + optional action permission host pattern, retained only when useful for install summary or diagnostics; arbitrary HTTP is acceptable for trusted local tool mode
- Requirements:
  - URL scheme 只接受 http/https。
  - host matching 使用 parsed URL，不用字符串 contains。
  - request/response body 使用 JSON/text/bytes 明确模式。
  - secrets 通过 env/secret facade 引用，不在 manifest 写明文。

### Workspace/VFS

- SDK: `ctx.api.workspace.readText(path, options)`, `writeText`, `list`, `stat`
- Host method: `vfs.*` 或 `workspace.*`
- Fact source: current activation workspace roots / Project VFS mounts
- Permission: workspace read/write/list capability + action permission
- Requirements:
  - 路径解析必须绑定 activation workspace root 或 Project VFS mount。
  - 不提供越过 workspace boundary 的 raw filesystem access。
  - 与现有 VFS surface / local tool executor 的路径安全 helper 复用。

### Env / Secret

- SDK: `ctx.api.env.get(name)` 或 `ctx.api.secrets.get(ref)`
- Host method: `env.get`
- Fact source: local runtime config / declared env mapping
- Permission: env name/prefix declaration when value should be surfaced in install summary or redacted audit
- Requirements:
  - 默认不暴露整个 `process.env`。
  - 返回值在 trace/log 中 redacted。
  - `getProfile` 的 username 不通过 env facade 暴露给插件；仍由 local profile API 表达。

### Process

- SDK: `ctx.api.process.exec(command, options)` 或 `ctx.api.process.shell(command, options)`
- Host method: `process.run`
- Fact source: local backend controlled process executor
- Permission: top-level process execute capability + action permission `process.execute`
- Requirements:
  - 首版按本机可信工具模型提供通用 shell/process 调用，不要求 command allowlist。
  - cwd 绑定 workspace root。
  - stdout/stderr/exit code 有大小和时间限制。
  - trace 记录 command、cwd、exit code、duration 与输出截断信息。
  - 不把 process capability 设计成高安全隔离边界；后续如需要 Marketplace 级安全，再增加 allowlist/profile policy。

### Runtime Invoke

- SDK: `ctx.api.runtime.invoke(actionKey, input)`
- Host method: `runtime.invoke`
- Fact source: RuntimeGateway
- Permission: top-level runtime action reference + action permission
- Requirements:
  - 避免 extension action 任意递归调用导致环路；trace 中记录 parent invocation。
  - 调用目标必须属于当前 Project/session 可见 surface。

### Extension Channel Invoke

- SDK: `ctx.api.channels.invoke(channelKey, method, input)`
- Host method: `extension.channel_invoke`
- Fact source: Project extension protocol channel registry
- Permission: consumer dependency/binding declaration + provider channel availability; usage permission is optional and only retained for diagnostics/audit
- Requirements:
  - provider 与 consumer 都必须属于当前 Project enabled extension runtime，Canvas consumer 必须属于当前 Project/session。
  - 调用路由到 provider extension host，不在 consumer 进程内直接 import provider 代码。
  - trace 记录 provider extension、consumer identity、channel key、method、invocation id。
  - provider 缺失、版本不满足、backend 离线、permission denied 都返回结构化 unavailable/denied 错误。

## SDK and Runner Shape

`packages/extension-sdk` 目标 API：

```ts
export default defineExtension({
  manifest,
  activate(ctx) {
    const client = createGitLabClient(ctx.api.http, ctx.api.env);

    ctx.channels.register({
      channel_key: "gitlab-review.api",
      version: "1.0.0",
      description: "GitLab review API",
      methods: {
        listMergeRequests: {
          input_schema: true,
          output_schema: true,
          permissions: ["http.fetch:gitlab.example", "env.read:GITLAB_TOKEN"],
          async invoke(input) {
            return client.listMergeRequests(input.project);
          },
        },
      },
    });

    ctx.runtime.registerAction({
      action_key: "gitlab-review.list_mrs",
      kind: "session_runtime",
      description: "List merge requests",
      permissions: ["http.fetch:gitlab.example", "env.read:GITLAB_TOKEN"],
      async invoke(input) {
        return client.listMergeRequests(input.project);
      },
    });
  },
});
```

Runner runtime 源码建议放入独立目录，例如：

```text
crates/agentdash-local/src/extensions/host/runner/
  runtime.ts
  protocol.ts
  context.ts
  host_api_client.ts
```

构建策略可以选择：

- 用 `build.rs` 或现有 package script 把 runner TS/JS 打成单个文件，再 `include_str!`。
- 或先以 `.js` 源文件维护，Rust 读取/嵌入该源码。关键是不要把长期维护的 runner runtime 留在 Rust raw string 中。

## Manifest and Domain Contract

`ExtensionPermissionDeclaration` 需要从当前三类扩展为完整但不过度的 capability model。建议结构：

```jsonc
{
  "permissions": [
    { "kind": "local_profile", "access": "read" },
    { "kind": "http", "hosts": ["gitlab.example"], "access": "read" },
    { "kind": "workspace", "access": "read_write" },
    { "kind": "env", "names": ["GITLAB_TOKEN"], "access": "read" },
    { "kind": "process", "access": "execute" },
    { "kind": "runtime_action", "action_key": "other-extension.action" },
    { "kind": "extension_channel", "channel_key": "gitlab-review.api", "methods": ["listMergeRequests"] }
  ]
}
```

Action-level permissions remain strings for compact projection and audit:

```jsonc
{
  "runtime_actions": [
    {
      "action_key": "gitlab-review.list_mrs",
      "permissions": ["http.fetch:gitlab.example", "env.read:GITLAB_TOKEN"]
    }
  ]
}
```

Domain evaluator 输出统一 decision：

```rust
ExtensionPermissionDecision {
    requested_permission,
    action_key,
    capability_family,
    allowed,
    reason,
}
```

Evaluator 的目标是收敛判断来源，而不是把所有能力都做成多层 deny-by-default。实现时应优先清理：

- 顶层 capability 与 action permission 表达同一件事但不给用户额外信息的重复门禁。
- 对本机可信工具没有现实安全收益、只让示例和开发流程变复杂的 command allowlist。
- SDK 类型里声明可用、宿主层却永远拒绝的半接入能力。

保留的门禁应当能回答明确问题：这个 Project 是否安装了 provider、consumer 是否声明了依赖、目标 backend 是否可用、workspace/cwd 是否越界、secret 是否需要 redaction、trace 是否能解释调用来源。

## Frontend Bridge

Panel bridge 和 `@agentdash/extension-ui` 必须保持一致：

- `runtime.invoke_action`：已接通，补 trace/permission error 展示。
- `extension.invoke_channel`：新增，用于 panel 或 Canvas-like runtime 调用 Project 中已授权 extension channel。
- `metadata.get_context`：已接通，保持。
- `workspace.open_tab`：已接通，保持。
- `vfs.read` / `vfs.write`：需要接到 Project/session VFS routes，并按 panel 所属 extension/tab 做 admission。
- `events.emit` / `events.subscribe`：需要定义 panel-local event bus 或明确只作为 intra-panel helper；若作为平台事件，需要后端/WorkspacePanel 路由。

桥接请求仍只传 method + params；Project/session/backend/actor/context 由宿主组装。

## Example and Docs

新增 `examples/extensions/protocol-demo` 作为完整协议/channel 示例，`examples/extensions/local-hello` 保持最小 Host API 示例。`protocol-demo` 结构如下：

```text
src/
  extension.ts
  protocol/
    demo-client.ts
  shared/
    schema.ts
  panel/
    App.tsx
```

Actions：

- `protocol-demo.greet`：纯 TS 输入输出，不调用 Host API。
- `protocol-demo.fetch_demo`：通过 `ctx.api.http.fetchJson()` 或本地 fixture client 展示用户自写 protocol adapter。
- `protocol-demo.shell_demo`：通过 `ctx.api.process.shell()` 展示本机可信工具模型下的通用 process/shell。
- `protocol-demo.demo_channel`：注册 protocol channel method。
- `protocol-demo.consume_demo_channel`：consumer action 或 Canvas-like panel 通过 channel invocation 调用 provider method。

Docs：

- `docs/extension-system.md`
  - mental model
  - authoring project structure
  - manifest and permissions
  - built-in Host APIs
  - user-owned TS host/protocol adapter
  - extension protocol channel provider/consumer
  - Canvas consuming extension channels
  - pack/install/frontend use flow
  - `getProfile` source and role
  - debugging checklist

## Migration and Compatibility

项目仍在预研期，不保留旧 contract 兼容层。涉及 manifest/domain/DTO 改动时：

- 更新 Rust domain value object 与 validator。
- 更新 API generated contracts。
- 更新 frontend mapper/types。
- 更新 `extension-dev` validation。
- 如 Project installation / artifact 表需要新字段，新增 migration 并迁移现有 seed/demo 数据到新正确形态。

## Risks

- Process/env capability 风险最高；本任务按本机可信工具模型处理 process，不做 Marketplace 级安全隔离，但仍需要 cwd、timeout、输出限制和 trace，避免调试体验被挂死进程或无限输出拖垮。
- HTTP permission pattern 容易被字符串匹配绕过，必须用 URL parser 和 host canonicalization。
- Panel bridge 与 host SDK 容易形成两套 API，需要在 docs 中区分 panel-side `@agentdash/extension-ui` 与 host-side `@agentdash/extension-sdk`。
- Runner 抽离如果和功能扩展同时做，改动面较大；实施时应先抽测试保护，再迁移 capability。
