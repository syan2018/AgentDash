# Design · Extension App 一体化 SDK 收束

流程图与后续手册草案见 [handbook-flow-draft.md](./handbook-flow-draft.md)。

后端、协议和工具链只读 review 摘要见 [subagent-review-summary.md](./subagent-review-summary.md)。

## 1. 最终方案

当前最清晰的方案不是新增一套 Extension 协议栈，而是把 authoring 入口收束到一个 App 声明，再生成现有运行时已经理解的产物：

```text
agentdash.app.ts
  -> manifest v2 / host bundle / panel bundle
  -> existing runtime action + protocol channel
  -> existing Workspace Module describe / invoke
```

对外只保留：

- `@agentdash/extension`
- `agentdash-ext`
- `Extension App`
- `agentdash.app.ts`
- `Capability`

Agent 可调用能力不是新的 authoring 顶层对象，而是 capability 上的 exposure 标注，最终投影到既有 Workspace Module operation。

## 2. 原始目标覆盖

| 目标 | 覆盖方式 | 边界 |
| --- | --- | --- |
| 独立静态 Web App 打包为 Extension | `agentdash-ext wrap-webapp --dist` 生成最小 App definition、panel bundle、no-op host、package artifact | 只要求已有静态构建产物 |
| 依赖 `/api/**` 的 Web App 低侵入导入 | 显式 `fetch_routes` 绑定到 `httpProxy` / `customChannel` / `backendService` | 不全局劫持任意浏览器 `fetch` |
| 既有本机或远端后端复用 | `httpProxy` 表达已存在 endpoint，请求由 selected local backend 转发 | 不因 URL 是 localhost 自动判断服务归属 |
| Web App 自带后端随插件分发 | `backendService` service bundle + 本机 lifecycle manager | 只用于 Extension artifact 自带服务，属于 M3 |
| 原生 Extension 开发减负 | `defineApp` + capability recipes + `useAgentDash`，manifest/host/panel glue 全部生成 | 低层 host/browser API 只作为逃生口 |
| Agent 调用能力 | capability exposure 生成 Workspace Module operation 投影 | Operation 不是用户侧第二事实源 |
| 闭环开发 | `agentdash-ext dev / validate / pack / install` 消费同一份 `agentdash.app.ts` | dev 诊断要解释 route、permission、dispatch |

## 3. Authoring 模型

Extension App authoring 主线：

```text
Capability (+ Agent exposure annotations) -> Dispatch -> Artifact
```

| 对象 | 含义 |
| --- | --- |
| `Capability` | App 声明自己能接入什么能力，例如 HTTP、本机命令、workspace 文件、自定义 channel、自带后端服务 |
| Agent exposure annotation | capability 中哪些动作允许被 Agent 发现、授权、调用和审计 |
| Workspace Module operation | exposure 被生成到既有 `module_id + operation_key + input` 调用面后的结果 |
| Dispatch | runtime 最终把调用交给哪条既有执行链路 |
| Artifact | package 需要携带的 panel、host、service 产物 |

实现约束：

- 用户只维护 `agentdash.app.ts`；manifest、host entry、panel client 都是生成物。
- `fetch_routes` 只是 panel fetch 兼容配置，默认 `panel_only`。
- `operation_catalog` 是 capability exposure 的生成投影，不是用户维护的第二份事实源。
- `backend_services` 描述 service bundle materialization，不是 Agent 直接调用面。
- bridge 只负责 panel/browser 到 runtime 的传输，不拥有能力语义。

## 4. 包形态

现有三个独立包收束为一个包：

```text
packages/extension/
  src/
    app/          # defineApp、recipes、normalized model
    react/        # useAgentDash
    browser/      # panel bridge、fetch route glue
    host/         # host runtime helpers
    toolchain/    # dev / validate / pack / install / wrap-webapp
    cli/          # agentdash-ext
```

推荐入口：

```ts
import { defineApp, httpProxy, localCommand, workspaceFiles } from "@agentdash/extension";
import { useAgentDash } from "@agentdash/extension/react";
```

高级逃生口：

```ts
import { defineHost } from "@agentdash/extension/host";
import { createBridge } from "@agentdash/extension/browser";
```

默认入口不得导出 Node-only toolchain API，也不得导出需要 host VM 的低层注册 API。

## 5. 项目结构

新模板结构：

```text
my-extension/
  agentdash.app.ts
  package.json
  tsconfig.json
  src/
    index.html
    main.tsx
    App.tsx        # React 模板习惯，不是平台入口规则
```

生成物：

```text
.agentdash/generated/
  manifest.json
  extension.ts
  client.ts

dist/
  extension.js
  panel/

packed/
  *.agentdash-extension.tgz
```

规则：

- `panel.entry` 是平台识别 Web App 构建入口的规则。
- `App.tsx` 只是 React 模板文件。
- Extension App 模式下不要求用户手写 `agentdash.extension.json` 或 `src/extension.ts`。

## 6. App Definition

最小 DSL：

```ts
import { defineApp, httpProxy, localCommand, workspaceFiles } from "@agentdash/extension";

export default defineApp({
  id: "repo-tools",
  name: "Repo Tools",
  version: "0.1.0",
  panel: {
    entry: "src/main.tsx"
  },
  capabilities: {
    github: httpProxy({
      baseUrl: "https://api.github.com",
      access: "read_write"
    }),
    gitStatus: localCommand({
      command: "git",
      args: ["status", "--short"]
    }),
    files: workspaceFiles({
      access: "read_write"
    })
  }
});
```

所有命令先把 `defineApp()` 归一化为 `AgentDashAppDefinition`，再生成 manifest、host entry、panel client、permission summary 和 diagnostics。禁止各命令 ad-hoc 拼 manifest。

## 7. Capability Recipes

| Recipe | 用途 | 生成与执行 |
| --- | --- | --- |
| `httpProxy` | 远端 API 或已存在本机 HTTP 服务 | 生成 HTTP 权限、host channel/helper；通过 `ctx.api.http.fetch()` 在 selected local backend 发起请求 |
| `localCommand` | 明确短命令 | 生成 process 权限和 run helper；默认用 `process.exec`，shell 语义必须显式声明 |
| `workspaceFiles` | 当前 workspace 文件读写 | 生成 workspace 权限和 read/write/list/stat helper |
| `customChannel` | 复杂结构化协议逃生口 | 生成 `protocol_channels[]` 和 typed client |
| `backendService` | Extension artifact 自带长驻后端服务 | 生成 service bundle 声明、fetch route、service diagnostics；完整 lifecycle 属于 M3 |

权限词表收束到现有后端 vocabulary：`process.exec` / `process.shell`，不继续传播 `process.execute`。

## 8. 请求与 localhost 边界

声明过的 capability request 默认经 bridge 到 selected local backend：

```text
Panel generated client
  -> bridge
  -> cloud/API runtime route
  -> selected local backend
  -> host / httpProxy / backendService / local capability
```

规则：

- localhost 请求若属于已声明 `httpProxy`，由 selected local backend 发起。
- URL 是 `localhost` 不代表它是 `backendService`。
- 只有服务代码随 Extension artifact 分发，并需要 backend 管理生命周期时，才使用 `backendService`.
- 普通浏览器 `fetch()` 不做全局透明劫持。
- 只有显式 `fetch_routes` 会被包装成 bridge request；未声明请求按浏览器原语义执行，并在 dev/validate 中诊断。

## 9. 生成管线

`agentdash-ext dev|validate|pack` 共用一条管线：

```text
load agentdash.app.ts
  -> normalize AgentDashAppDefinition
  -> generate manifest
  -> generate host entry
  -> generate panel client / fetch route glue
  -> validate generated surface
```

生成内容：

- manifest v2：`runtime_actions` / `protocol_channels` / `workspace_tabs` / `permissions` / `bundles`
- host entry：注册 capabilities 需要的 actions/channels
- panel client：提供 `useAgentDash()` 消费的 capability facade
- permission summary：用于 dev preview 和文档
- optional projection：capability exposure 到 Workspace Module operation catalog

校验：

- generated host activation 后的 registered surface 必须与 generated manifest 一致。
- operation catalog projection 单独校验，不并入现有 action/channel surface parity。
- package archive 必须通过现有后端 archive validation。

## 10. Workspace Module 与 Agent 调用

Workspace Module 已有 `list / describe / invoke / present`。本任务只把 capability exposure 投影进去：

```text
Capability exposure
  -> generated operation catalog projection
  -> runtime projection
  -> Workspace Module describe / invoke
```

约束：

- Agent 只能调用 `visibility = "agent_and_panel"` 的 operation。
- 每个 Agent 可调用 operation 必须有 key、description、input/output schema、permission summary、dispatch 和 provenance。
- `fetch('/api/**')` wildcard 默认只服务 panel。
- backend service route 要暴露给 Agent，必须在对应 capability 上显式声明 exposure。

## 11. Web App 快捷导入

静态包装：

```powershell
agentdash-ext wrap-webapp --dist ./dist --extension-id my-app --name "My App"
```

生成：

- 最小 App definition 或 normalized definition
- no-op host entry
- panel bundle
- manifest/package artifact

带 API route：

```powershell
agentdash-ext wrap-webapp --dist ./dist --fetch-route "/api/**=api"
```

fetch route 必须显式绑定到：

- `httpProxy`：已存在远端或本机 HTTP endpoint
- `customChannel`：fetch-compatible host handler
- `backendService`：Extension artifact 自带服务

评估模式：

| 模式 | 判断 | 处理 |
| --- | --- | --- |
| `static` | 只有静态 UI | 直接包装 |
| `fetch-route` | 有 `/api/**` 等相对请求 | 要求声明 route 绑定 |
| `backend-service` | 自带长驻服务 | 需要 `backendService` service bundle |
| `unsupported` | SSR、未声明动态网络、Service Worker 等 | 报告阻塞原因 |

## 12. backendService

`backendService` 只表示 Extension artifact 自带后端服务：

```ts
backendService({
  entry: "src/server/index.ts",
  routes: ["/api/**"],
  runtime: "node",
  healthPath: "/health"
})
```

完整 M3 需要表达：

- `backend_services[]`
- `bundles.kind = backend_service`
- service bundle digest/files/runtime/entry/routes/healthPath
- selected local backend materialize/start/health/stop/log
- fetch route 到 service 的 backend-side forwarding

运行边界：

- 云端不直连 localhost。
- 端口只存在于 selected local backend 内部。
- service operation describe 来自 generated operation projection，不从运行中的 HTTP server 临时抓取。
- route wildcard 只用于 panel 兼容迁移；Agent invoke 需要显式 exposure。

## 13. 后端复用边界

必须复用既有主链路：

```text
package artifact
  -> runtime projection
  -> RuntimeGateway / channel invoker
  -> relay 到 selected local backend
  -> TS extension host
  -> Host API
```

不新增：

- 第二套 Extension 执行面
- RuntimeGateway 旁路
- Workspace Module 聚合旁路

需要补：

- manifest / contract / runtime projection 字段：`fetch_routes`、generated `operation_catalog`、`backend_services`、`bundles.kind = backend_service`
- Workspace Module visibility/provenance projection
- backendService lifecycle manager，仅 M3
- `validateFetchRoutes` / `validateOperationCatalogProjection` / `validateBackendServices`

## 14. 实现分级

| 等级 | 后端执行机制 | 内容 |
| --- | --- | --- |
| M0 | 不新增 | 纯静态 `wrap-webapp`，生成 panel/tab/no-op host |
| M1 | 不新增 | `defineApp` recipes 编译到现有 runtime action / protocol channel / Host API |
| M2 | 不新增 | `fetch_routes` + operation projection + Workspace Module visibility/provenance |
| M3 | 新增本机 lifecycle manager | `backendService` service bundle materialize/start/health/stop/log |

推荐第一刀做到 M0-M2；M3 的协议位置先定稳，完整 lifecycle manager 单独落。

## 15. 测试策略

最低测试集：

- app definition normalize tests
- manifest generation golden tests
- host generation + activation surface parity tests
- panel client bridge mock tests
- wrap-webapp static/API route tests
- operation projection tests
- pack archive validation tests
- migrated example smoke tests

## 16. 不变量

- 一个 Extension App 只有一个声明事实源：`agentdash.app.ts`。
- 用户默认不手写 manifest、host entry、browser bridge glue。
- `operation_catalog` 是生成投影，不是用户输入。
- `backendService` 只用于 Extension artifact 自带服务。
- 已存在 localhost 服务使用 `httpProxy`，由 selected local backend 发起请求。
- 单 SDK 是对外产品形态；源码目录拆分不变成新用户概念。
