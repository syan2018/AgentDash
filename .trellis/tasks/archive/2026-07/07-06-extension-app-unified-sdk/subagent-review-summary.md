# Subagent Review Summary

本文件汇总本任务规划阶段的只读 subagent review 结论，作为后续实现依据。

## 1. 总结

当前后端和本机 runtime 已经具备完整 Extension 执行主链路：

```text
package artifact
  -> runtime projection
  -> RuntimeGateway / channel invoker
  -> relay 到当前本机 backend
  -> TS extension host
  -> Host API 执行 http / process / workspace / env / local 能力
```

因此本任务的重点不是重造执行面，而是：

- 收束单 SDK / CLI authoring 入口。
- 扩展 manifest / contract / runtime projection。
- 从 capability exposure 标注生成可投影到 Workspace Module 的 operation catalog。
- 为完整 `backendService` 增加 service bundle 与本机 lifecycle manager 的协议约束。

## 2. 已有能力

后端已有：

- Extension package artifact、archive digest、manifest digest、storage ref 与安装链路。
- manifest v2 的 `runtime_actions`、`protocol_channels`、`workspace_tabs`、`permissions`、`bundles`。
- runtime projection 对 enabled installation 的 action/channel/tab/bundle 投影。
- webview asset 从 package artifact 按声明 panel 目录读取。
- RuntimeGateway 对 packaged extension runtime action 的 discover/invoke。
- Extension channel invoker 对 provider/method/dependency alias/permission 的解析与调用。
- relay 到 selected local backend 的 action/channel command。
- local backend TS extension host artifact cache、activation、action/channel invocation。
- Host API facade：`http.fetch`、`process.exec/shell`、`workspace.vfs.*`、`env.read`、`local.getProfile`。
- Workspace Module `list / describe / invoke / present`，并已把 runtime actions / protocol channel methods 投影为 Agent operation。

## 3. 协议缺口

当前协议不能一等表达：

- `backend_services[]`
- `bundles.kind = backend_service`
- `fetch_routes[]`
- `operation_catalog[]`，作为 capability exposure 的生成投影
- operation 级 `visibility = panel_only | agent_and_panel`
- operation provenance / recipe 来源
- panel-only `/api/**` 兼容路由
- Workspace Module `backend_service` dispatch，除非将其降级为 protocol channel method

当前 `bundles` 只支持 `extension_host`，因此完整 `backendService` 的 service bundle/lifecycle 不能靠现有 manifest 表达。

## 4. 实现分级

| 等级 | 是否新增后端执行机制 | 内容 |
| --- | --- | --- |
| M0 | 否 | 纯静态 `wrap-webapp`，生成 panel/tab/no-op host |
| M1 | 否 | `defineApp` recipes 编译到现有 runtime action / protocol channel / Host API |
| M2 | 否 | `fetch_routes` + `operation_catalog` + Workspace Module visibility/projection，仍复用 action/channel 执行 |
| M3 | 是，仅本机 lifecycle manager | `backendService` service bundle materialize/start/health/stop/log；仍复用 selected backend、relay、artifact cache、permission、trace、Workspace Module |

## 5. 关键决策

- `backendService` 不新增 RuntimeGateway 旁路。
- `backendService` 不新增第二套 Workspace Module 聚合器。
- `backendService` 不让云端直连 localhost。
- 访问已有 localhost 服务使用 `httpProxy`，由当前本机 backend 发起请求。
- 只有 Extension artifact 自带服务代码，并需要 backend 管理生命周期时，才使用 `backendService`。
- `/api/**` wildcard fetch route 默认 `panel_only`。
- Agent 可调用能力必须来自 capability 上的显式 exposure 标注，并生成具备 schema、description、permissions、dispatch、visibility 的 operation projection。
- `protocol_channels.methods` 与 `runtime_actions` 仍是现有 runtime 投影的主要来源；`operation_catalog` 是生成投影，用于承载 visibility/provenance/operation key 或 backend service exposure。

## 6. Toolchain 结论

现有三个包是工程拆分，不应继续作为用户概念：

- `extension-sdk`：host 类型、`defineExtension()`、`createExtensionContext()`、Host API facade。
- `extension-ui`：browser panel postMessage bridge。
- `extension-dev`：`agentdash-ext` CLI、dev preview、validate、pack、install。

收束为 `@agentdash/extension` 时可直接复用：

- host SDK 类型与注册 helper。
- browser bridge。
- manifest validation、surface parity、dev runtime dispatcher、pack archive、install 上传。

需要新增：

- `defineApp()` normalized model。
- manifest / host entry / panel client / permission summary generators。
- `useAgentDash()`。
- `wrap-webapp` M0/M1。
- capability exposure 到 `operation_catalog` projection 的 generator 与 validator。
- backend service contract validator；完整 lifecycle manager 留到 M3。

## 7. 实现注意

- 现有 action/channel surface parity 不应直接包含 `operation_catalog`，否则 backend service operation 会被错误要求 TS `activate()` 注册。
- 应新增独立校验器：
  - `validateOperationCatalogProjection`
  - `validateBackendServices`
  - `validateFetchRoutes`
  - 扩展后的 `validateBundleDefs`
- `process.execute` 与当前 permission vocabulary 中的 `process.exec/process.shell` 存在不一致，单包收束时应统一。
