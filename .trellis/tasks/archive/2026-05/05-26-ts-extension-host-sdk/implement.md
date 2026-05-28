# Implementation Plan

> 状态：planning draft。执行前需要用户确认 MVP 范围，并将本任务拆成可独立验证的子任务。

## Phase 0: Planning Gate

- [x] 用户确认首个 MVP renderer：直接包含 sandboxed webview/custom UI bundle，schema-driven renderer 仅作为诊断或 fallback。
- [x] 用户确认首版 scope：project-level extension only。其它 Project 通过 Marketplace / package 快速安装获得同类能力。
- [x] 用户确认 extension package archive 的首版权威位置：后端 / Project asset 侧为正式安装事实源；local store 只做 dev mode 与运行缓存。
- [x] 根据确认结果拆分子任务；父任务保留集成验收与跨层契约。

## Phase 1: Cross-layer Contracts

- [x] 扩展 `crates/agentdash-domain/src/shared_library/value_objects.rs`
  - `ExtensionTemplatePayload` 增加 `runtime_actions`、`workspace_tabs`、`permissions`、`bundles`。
  - 增加 typed validation：action key、type id、uri scheme、renderer kind、permission shape、bundle digest。
- [x] 更新 `crates/agentdash-infrastructure/migrations/*`
  - 若 manifest JSONB 足够，迁移只需确保 validators 与 existing rows repair。
  - 若引入 package/bundle 表，新增 migration 和 repository。
- [x] 更新 `crates/agentdash-application/src/session/construction.rs`
  - 增加 extension runtime projection 字段：runtime actions、workspace tabs、permissions、bundle refs。
- [x] 更新 `crates/agentdash-api/src/session_use_cases/construction.rs`
  - 从 enabled project extension installations flatten 新字段。
  - 增加冲突检测：action key / workspace tab type id / uri scheme。
- [x] 更新 `crates/agentdash-api/src/routes/acp_sessions.rs`
  - `SessionContextResponse` 暴露 `extension_runtime` 或等价 projection DTO。
- [x] 更新 `crates/agentdash-contracts`
  - 生成 shared-library/session context 相关 TS contracts。
- [x] 前端更新 `packages/app-web/src/services/session.ts`
  - 使用 mapper 解析 extension runtime，禁止直接信任 raw JSON。

Validation:

```powershell
pnpm run contracts:check
cargo test -p agentdash-domain
cargo test -p agentdash-api session_use_cases::construction
pnpm run frontend:check
```

## Phase 2: SDK Packages

- [x] 新增 `packages/extension-sdk`
  - `defineExtension`
  - `ExtensionContext`
  - `runtime.registerAction`
  - `workspace.registerPanel`
  - `commands.registerCommand`
  - schema helpers and manifest builder
- [x] 新增 `packages/extension-ui`
  - panel bridge client
  - `invokeAction`
  - `openWorkspaceTab`
  - VFS read/write facade
  - event subscribe/emit facade
- [x] 新增 `packages/extension-dev`
  - CLI `init`
  - CLI `dev`
  - CLI `validate`
  - CLI `pack`
  - CLI `install`
- [x] `pack` 集成 bundler，分别产出 extension host bundle 与 webview bundle。
- [x] `validate` 检查安装包自包含：禁止依赖安装脚本成为运行路径；标记 native addon / platform binary / postinstall 下载需求。
- [x] 更新 `pnpm-workspace.yaml` 如需 package 命名分层。
- [x] 新增 `examples/extensions/local-hello/`，作为独立 demo extension project，而不是零散 fixture。
- [x] demo project 包含自己的 `package.json`、`agentdash.extension.json`、extension host 入口、webview UI、测试和 README。

Validation:

```powershell
pnpm --filter @agentdash/extension-sdk typecheck
pnpm --filter @agentdash/extension-ui typecheck
pnpm --filter @agentdash/extension-dev typecheck
pnpm --filter @agentdash/extension-dev test
```

## Phase 3: Local TS Extension Host

- [x] 在 `crates/agentdash-local` 增加 extension host manager。
- [x] 定义 local <-> TS host JSON-RPC protocol。
- [x] 实现 host lifecycle：initialize、activate、deactivate、reload、health。
- [x] 实现 action invocation：invoke_action、result/error normalization。
- [x] 实现 permission-mediated host APIs：HTTP、VFS、env、process 的首版最小子集。
- [x] 让 `pnpm dev` / local dev mode 能发现 `examples/extensions/local-hello`。

Validation:

```powershell
cargo test -p agentdash-local
pnpm run dev:local
```

Manual check:

- 启动 `local-hello` extension。
- 修改 TS handler 后 reload。
- 调用 action 返回新结果。

## Phase 4: RuntimeGateway Proxy

- [x] 在 application/API 层实现 `ExtensionRuntimeActionProvider`。
- [x] provider 读取当前 session project 的 enabled extension action projection。
- [x] provider 通过 `BackendRegistry` 转发到 owning local backend。
- [x] relay protocol 增加 extension action invoke command/response。
- [x] action result 保留 RuntimeGateway trace、extension id、action key、backend id。
- [x] 错误映射到 `ProviderUnavailable` / `CapabilityDenied` / `ProviderFailed`。

Validation:

```powershell
cargo test -p agentdash-application runtime_gateway
cargo test -p agentdash-api
cargo test -p agentdash-relay
```

## Phase 5: Frontend WorkspacePanel Contributions

- [x] `WorkspaceRuntimeData` 增加 `extension_runtime`。
- [x] 新增 extension tab descriptor factory。
- [x] `tabTypeRegistry` 支持 runtime contribution lifecycle。
- [x] `AddTabMenu` 订阅 registry snapshot 变化，显示 plugin tabs。
- [x] plugin tab unavailable state：extension disabled / action missing / backend offline。
- [x] 实现首个 renderer：
  - `webview` 主路径：sandboxed iframe + bridge + asset loading + lifecycle。
  - `runtime_panel` 可选：schema form + invoke + result view，用于开发诊断或 fallback。
- [x] 补充 WorkspacePanel tests。

Validation:

```powershell
pnpm run frontend:check
pnpm run frontend:test
```

Manual check:

- 安装 `local-hello` extension。
- 打开 session。
- `+` 菜单出现插件面板。
- 打开插件 webview 面板后，用户自定义 UI 能通过 `@agentdash/extension-ui` bridge 调用 runtime action 并展示结果。
- 刷新页面后 tab layout 恢复。

## Phase 6: Install / Pack / Project Flow

- [x] `extension-dev pack` 输出 archive、manifest、digest。
- [x] `extension-dev install` 上传 archive artifact，并调用 AgentDash API 写入 Project extension installation；MVP 主路径支持外部 archive 安装，而不是只支持 native plugin embedded seed。
- [x] 后端保存 artifact storage ref、digest、package metadata 与 source version。
- [x] `agentdash-local` 按 Project installation 下载、校验、解包 archive，并复用本机 cache。
- [x] 安装端不执行 `npm install` / `pnpm install` / package lifecycle scripts；所有运行依赖来自 archive contents。
- [x] Marketplace / Assets UI 可展示 extension template 新字段摘要。
- [x] Project source-status 覆盖 extension installation。
- [x] Extension enable/disable 触发 session runtime projection refresh。

Validation:

```powershell
cargo test -p agentdash-application shared_library
cargo test -p agentdash-infrastructure
pnpm run frontend:test
```

## Phase 7: Canvas Promote to Extension

- [x] 定义 Canvas -> ExtensionTemplate mapper。
- [x] 后端发布路径从 Canvas 读取 files、entry、sandbox config、bindings。
- [x] Extension manifest 写入 `workspace_tabs` with `canvas_panel` renderer。
- [x] 前端增加 Promote action 入口和安装后打开 tab 路径。
- [x] 使用 Canvas runtime preview 抽象渲染 promoted extension。

Validation:

```powershell
cargo test -p agentdash-application canvas
cargo test -p agentdash-api canvases
pnpm run frontend:test
```

## Phase 8: Demo Extension and End-to-end Verification

- [x] `examples/extensions/local-hello` 可在目录内独立运行 `dev`、`validate`、`pack`。
- [x] demo action `local-hello.profile` 通过 extension backend SDK 调用 `api.local.getProfile()`，返回受限本机 profile。
- [x] demo webview 通过 `@agentdash/extension-ui` 调用 action 并展示 username / platform / backend id / session 摘要。
- [x] 新增 packaged artifact E2E：`local-hello` 源码目录执行 `pack`，上传 archive artifact，Project installation 引用 artifact storage ref/digest。
- [x] E2E 安装后不依赖 local dev ref：清理或忽略 demo 源码路径，`agentdash-local` 从平台 artifact 下载、校验、解包 packaged extension。
- [x] 新增 E2E：packaged local-hello install -> session context exposes extension_runtime -> open panel -> invoke action -> display local profile。
- [x] 新增 E2E：Canvas promote -> install -> workspace tab opens。
- [x] 运行关键检查。

Validation:

```powershell
pnpm run contracts:check
pnpm run backend:check
pnpm run backend:test
pnpm run frontend:check
pnpm run frontend:test
pnpm run e2e:test:critical
```

## Risky Files / Rollback Points

- `crates/agentdash-domain/src/shared_library/value_objects.rs`
  - 风险：manifest schema 变化影响 seed/install validators。
  - 回滚点：保持 manifest v1/v2 validator 分支或 migration repair 独立提交。
- `crates/agentdash-api/src/routes/acp_sessions.rs`
  - 风险：session context DTO 扩展影响前端 runtime state。
  - 回滚点：新增字段保持 optional，并由 mapper 兼容空值。
- `crates/agentdash-application/src/runtime_gateway/*`
  - 风险：provider routing 影响 Canvas/MCP runtime actions。
  - 回滚点：extension provider 独立注册，不改现有 provider 行为。
- `packages/app-web/src/features/workspace-panel/*`
  - 风险：dynamic registry 影响 built-in tabs。
  - 回滚点：extension contribution lifecycle 独立于 built-in registration。
- `crates/agentdash-local/*`
  - 风险：TS host 进程生命周期影响本机后端稳定性。
  - 回滚点：feature flag 或 local config 控制 extension host 启动。

## Suggested Child Tasks

- `extension-runtime-contracts`
  - 扩展 domain/API/contracts/session projection。
- `extension-sdk-cli`
  - 新增 SDK 和 dev CLI。
- `extension-package-artifacts`
  - 后端 archive artifact 存储、digest、下载与 local cache。
- `local-ts-extension-host`
  - 本机 TS host 与 JSON-RPC protocol。
- `extension-runtime-gateway-proxy`
  - RuntimeGateway proxy provider 与 relay command。
- `workspace-panel-extension-tabs`
  - 前端动态 tabs 与 renderer。
- `local-hello-extension-demo`
  - 独立 demo extension project 与端到端金线验证。
- `canvas-promote-extension`
  - Canvas 转插件示例与发布安装链路。

## Review Gates Before `task.py start`

- [x] 用户确认 Phase 0 三个产品决策。
- [x] 根据决策创建或确认子任务树。
- [x] 每个子任务都拥有自己的 PRD / design / implement。
- [x] 父任务不直接启动实现，除非用户要求把 MVP 合并为一个实现任务。
