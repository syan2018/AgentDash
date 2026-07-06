# Implement · Extension App 一体化 SDK 收束

## 执行原则

本任务作为一个 Trellis 任务完成，不再拆子任务。实现时按 [design.md](./design.md) 的主线收束协议：

```text
Capability (+ Agent exposure annotations) -> Dispatch -> Artifact
```

对外只交付 `@agentdash/extension` 与 `agentdash-ext`。旧的 `extension-sdk`、`extension-ui`、`extension-dev` 作为源码来源被并入新包，仓库示例和文档直接迁移到新入口。Workspace Module operation 沿用既有 Agent 调用面，不作为新的 authoring 顶层对象。

推荐实现优先级：

1. 单包合并与示例迁移，消除三包 authoring 心智。
2. `defineApp` 生成 manifest / host / panel glue，复用现有 package validation。
3. capability exposure 生成 Workspace Module operation 投影，复用现有 `describe/invoke`。
4. `wrap-webapp` 支持静态包和显式 fetch route。
5. `backendService` lifecycle manager 只在 service bundle 需要随 Extension artifact 分发时实现。

## 并行派发计划

第一轮实现先打 M0-M2：单 SDK、`defineApp` 生成、`wrap-webapp`、Workspace Module operation 投影。`backendService` M3 只保留协议位置，完整 lifecycle manager 后置。

主会话先负责骨架合并，避免后续 agent 抢同一批 package graph 文件：

- 建立 `packages/extension` 与 `app / react / browser / host / toolchain / cli` 目录。
- 将旧 `extension-sdk`、`extension-ui`、`extension-dev` 的入口和脚本迁移到新包。
- 更新 workspace/package graph，使后续实现只往 `@agentdash/extension` 收口。

第一波 sub-agent 并行：

| Agent | 范围 | 主要落点 | 验收 |
| --- | --- | --- | --- |
| SDK / Authoring | `defineApp`、normalized model、recipes、Agent exposure、permission summary | `packages/extension/src/app/**`、`packages/extension/src/host/**` | recipes 不泄漏 runtime action / protocol channel 心智；权限词表统一到 `process.exec/process.shell` |
| Toolchain / Generator | `dev / validate / pack / install`、manifest/host/panel client generator、surface parity | `packages/extension/src/toolchain/**`、`packages/extension/src/cli/**`、`packages/extension/src/browser/**` | `agentdash.app.ts` 能生成 manifest、host entry、panel client 并通过现有 archive validation |
| Web App Import | `wrap-webapp`、static dist、no-op host、显式 `fetch_routes`、未声明 fetch 诊断 | `packages/extension/src/toolchain/wrap-webapp*`、`packages/extension/src/browser/fetch-route*`、wrap demo | 静态 Web App 可 pack；`/api/**` 必须显式 route；不全局劫持 `fetch("http://localhost:...")` |
| Contract / Workspace Module | manifest/runtime projection DTO、generated operation projection、visibility/provenance、Workspace Module describe/invoke | `crates/agentdash-contracts/src/extension/**`、`crates/agentdash-workspace-module/src/**`、`crates/agentdash-relay/src/protocol/**`、`crates/agentdash-application-runtime-gateway/src/runtime_gateway/**` | Agent 只能 invoke `agent_and_panel`；panel-only fetch route 不暴露给 Agent；不新增 RuntimeGateway 旁路 |

第二波在第一波能编译后派发：

- Examples / Smoke：迁移 `local-hello`、`protocol-demo`，新增 static wrap 和 API route wrap smoke，更新 README/handbook。
- Check Agent：只做验证和 review，检查两条工作流、权限词表、operation projection、localhost/backendService 边界和旧包依赖清理。

M3 单独派发：

- `backend_services[]`、`bundles.kind = backend_service`、service bundle materialize/start/health/stop/log、backend-side route forwarding。
- 只有 M0-M2 合并稳定后再启动，避免 lifecycle manager 拖慢主线。

## 实施顺序

### 1. 收束包结构

- 新建 `packages/extension`，建立 `app / react / browser / host / toolchain / cli` 源码目录。
- 将 `packages/extension-sdk`、`packages/extension-ui`、`packages/extension-dev` 的现有能力移动到新包对应目录。
- 更新 workspace/package graph，让示例和工具链只依赖 `@agentdash/extension`。
- 删除旧包的对外 package 形态，避免后续示例继续传播三包心智。
- 保持依赖方向：`app` 只放声明与模型，`react/browser` 可给 panel 使用，`host` 可给 extension host 使用，`toolchain/cli` 可依赖其它目录但不进入 browser 默认入口。

### 2. 建立 App Definition 与 normalized model

- 实现 `defineApp` 与 recipes：`httpProxy`、`localCommand`、`workspaceFiles`、`customChannel`、`backendService`。
- 将 `agentdash.app.ts` 归一化为 capability、Agent exposure annotation、dispatch、artifact 四类实现对象。
- 校验 key、权限、schema、visibility、route 冲突、bundle entry、panel entry。
- 统一 process 权限词表，以现有后端 permission vocabulary 为准，避免 `process.execute` 与 `process.exec/process.shell` 并存。

### 3. 生成 manifest / host / panel glue

- `agentdash-ext dev / validate / pack / install` 改为读取 `agentdash.app.ts`。
- 生成 `.agentdash/generated/**`：manifest、host entry、panel client、Workspace Module operation catalog 投影、permission summary。
- 生成物必须能回填到现有 extension package archive validation，不绕过当前 manifest/bundle/webview asset 校验。
- `useAgentDash` 从 generated panel client 消费 capability facade，让 App UI 只调用业务能力，不直接拼 bridge method。

### 4. 接入既有 runtime 执行链路

- 将 recipes 编译到现有 runtime action、protocol channel 与 Host API facade。
- `httpProxy` 复用 host `http.fetch`。
- `localCommand` 复用 host `process.exec/shell`。
- `workspaceFiles` 复用 host `workspace.vfs.*`。
- `customChannel` 复用 extension channel invoker。
- dev preview 展示 capability、operation、permission、dispatch、request log，帮助一把梭式开发闭环定位问题。

### 5. 支持既有简单 Web App 快捷导入

- 新增或收束 `agentdash-ext wrap-webapp`，输入静态构建产物并生成最小 Extension App。
- 静态 App 默认生成 panel/tab/no-op host，可完成 package validate 与 install。
- 对 `fetch('/api/**')` 类 Web App，只接受显式 route 映射：`httpProxy`、`customChannel` 或 `backendService`。
- `fetch_routes` 作为 panel 兼容传输配置生成，默认 `panel_only`；Agent 能力只从 capability exposure 生成的 operation catalog 投影暴露。

### 6. 补齐协议字段与 Workspace Module 投影

- 扩展 extension manifest / package DTO：`fetch_routes`、`operation_catalog`、`backend_services`、`bundles.kind = backend_service`。
- 扩展 runtime projection DTO，让前端诊断、runtime gateway、Workspace Module 看到同一份 generated operation catalog 投影。
- Workspace Module `describe` 展示 generated operation catalog，`invoke` 只接受 `visibility = agent_and_panel` 且具备 schema/permission/dispatch 的 operation。
- `present` 继续用于打开或渲染 panel，不把 panel-only fetch route 提升为 Agent 工具。
- Dispatch 仍落到既有 `runtime_action` / `protocol_channel` 主链路；需要 service route 时再进入 `backend_service` 分支。

### 7. 实现 backendService 完整链路

- 协议层先表达清楚：`backendService` 只代表 Extension artifact 自带服务代码，并由当前本机 backend materialize/start/health/stop。
- package artifact 携带 service bundle、digest、runtime、entry、routes、healthPath、operation catalog。
- 云端只保存声明、artifact identity、权限、trace 与审计；实际端口分配、localhost 访问和生命周期由 selected local backend 完成。
- 本机 backend 增加 service lifecycle manager：materialize service bundle、分配端口、启动进程、健康检查、转发 route、停止和日志采集。
- Workspace Module 调用 service operation 时仍走现有 relay/selected backend 路径，最终 dispatch 到本机 backend 管理的 service route。

### 8. 迁移示例与手册草案

- 迁移 `examples/extensions/local-hello` 与 `examples/extensions/protocol-demo` 到 `@agentdash/extension`。
- 更新 README / 手册草案为单入口叙事：创建 App、写 UI、声明能力、预览、打包、安装、Agent 调用。
- 将 [handbook-flow-draft.md](./handbook-flow-draft.md) 中的流程图保持为通用描述，不写入私有协议、私有路径或具体客户 App 名称。

## 主要落点

- 前端 SDK / CLI：`packages/extension-*` -> `packages/extension`
- 示例：`examples/extensions/local-hello`、`examples/extensions/protocol-demo`
- 前端 runtime webview：`packages/app-web/src/features/extension-runtime/**`
- Extension contracts：`crates/agentdash-contracts/src/extension/**`
- Package / SPI：`crates/agentdash-spi/src/extension_package.rs`
- Runtime relay/projection：`crates/agentdash-relay/src/protocol/extension_runtime.rs`
- Runtime gateway：`crates/agentdash-application-runtime-gateway/src/runtime_gateway/**`
- Workspace Module：`crates/agentdash-workspace-module/src/**`
- 本机 extension host/backend：`crates/agentdash-local/src/extensions/**`

## 验证计划

- `pnpm --filter @agentdash/extension run typecheck`
- `pnpm --filter @agentdash/extension run test`
- `pnpm --dir examples/extensions/local-hello run validate`
- `pnpm --dir examples/extensions/local-hello run pack`
- `pnpm --dir examples/extensions/local-hello run test`
- `pnpm --dir examples/extensions/protocol-demo run validate`
- `pnpm --dir examples/extensions/protocol-demo run pack`
- `pnpm --dir examples/extensions/protocol-demo run test`
- `agentdash-ext wrap-webapp --dist <static-demo-dist> --extension-id static-demo --name "Static Demo"`
- `agentdash-ext wrap-webapp --dist <api-demo-dist> --fetch-route "/api/**=api" --extension-id api-demo --name "API Demo"`
- 对 wrap 产物运行 `agentdash-ext validate` 与 `agentdash-ext pack`
- `cargo test -p agentdash-contracts extension`
- `cargo test -p agentdash-workspace-module workspace_module`
- `cargo test -p agentdash-application-runtime-gateway extension`
- `cargo test -p agentdash-local extension`

具体命令以迁移后的 package scripts 和 crate test filter 为准；若 contract DTO 触发生成代码，还需要补跑仓库现有 contract/codegen 校验命令。

## Review Gate

进入实现前确认三件事：

- `design.md` 已以 Capability + Agent exposure annotation / Dispatch / Artifact 收束 authoring 协议主线。
- `prd.md` 的验收项仍覆盖单 SDK、webapp 导入、operation catalog、backendService 完整设计。
- 本任务按 inline 实现推进时，启动前加载对应 `.trellis/spec/`；若改为 sub-agent dispatch，再补 curated `implement.jsonl` 与 `check.jsonl`。
