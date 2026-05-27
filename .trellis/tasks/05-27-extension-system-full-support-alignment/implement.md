# 插件系统完整能力对齐实施计划

## Phase 0: Baseline Review

- [x] 阅读本任务 `prd.md`、`design.md`、`research/current-state-and-source-evidence.md`。
- [x] 复核原始规划：`.trellis/tasks/archive/2026-05/05-26-ts-extension-host-sdk/prd.md` 与 `design.md`。
- [x] 复核相关 spec：`desktop-local-runtime.md`、`shared-library-contract.md`、`runtime-gateway.md`、frontend architecture/type-safety/state-management。
- [x] 记录用户已确认的平台管理 per-extension worker/process 边界。
- [x] 记录 protocol channel 一等贡献要求：其它插件与 Canvas 都可作为 consumer 调用插件注册的 API 信道。
- [x] 记录 channel authoring sugar 要求：self-scope shortcut、dependency alias、Canvas binding alias。
- [x] 记录 process capability 产品定位：本机可信工具，首版通用 shell/process，不做 command allowlist 权限设计。
- [x] 记录权限模型收敛要求：清理过度门禁，保留安装摘要、依赖解析、可用性诊断、workspace 边界、secret redaction 和 trace 审计真正需要的声明。
- [x] 记录新增示例决策：新增独立 `examples/extensions/protocol-demo`，不把完整协议/channel 示例塞进 `local-hello`。

## Phase 1: Contract and Permission Model

- [x] 扩展 `packages/extension-sdk` 的 Host API 类型：local、runtime、http、workspace/VFS、env/secret、process。
- [x] 扩展 `packages/extension-sdk` 的 channel API：provider 侧 `ctx.channels.register`，consumer 侧 `ctx.api.channels.invoke`。
- [x] 增加 channel authoring sugar：`ctx.api.channels.self(...)`、dependency alias client、Canvas binding alias contract。
- [x] 扩展 `ExtensionPermissionDeclaration` TS 类型与 manifest schema。
- [x] 增加 `protocol_channels` 与 `extension_dependencies` manifest schema，并定义 channel key / method / version / schema validation。
- [x] 扩展 `packages/extension-dev/src/manifest.js` validation 和 tests，覆盖新增 permission/capability/runtime requirement。
- [x] 扩展 Rust domain `ExtensionTemplatePayload` permission enum、validator、permission evaluator 和 tests。
- [x] 审计现有 extension permission deny path，删除或降级不提供实际产品价值的重复门禁。
- [x] 更新 generated frontend/backend contracts 所需来源。
- [x] 确保保留下来的 action-level permission string 与 top-level capability 的映射集中维护。
- [x] 确保 channel-level method declaration、consumer dependency 与 action/panel/Canvas binding 的映射集中维护。

Validation:

```powershell
pnpm --filter @agentdash/extension-sdk typecheck
pnpm --filter @agentdash/extension-dev test
cargo test -p agentdash-domain extension
```

## Phase 2: Runner Runtime Extraction

- [x] 为 TS Extension Host runner 建立独立源码维护位置。
- [ ] 将当前 Rust raw string runner 的 context/actions/host api client/protocol 拆成可测试 JS/TS 模块。
- [ ] 保持 Rust `LocalExtensionHostProcess` 协议不漂移，先用 golden tests 锁定 activate/invoke/host_api_request/host_api_response。
- [x] 在 runner context 中加入 channel handler registry，支持 activate 时收集 channel contributions，invoke 时路由到 method handler。
- [x] 通过 build/embed 机制让 Rust 继续获得 runner 源码。
- [x] 保持 `local-hello.profile` 现有链路可跑。

Validation:

```powershell
cargo test -p agentdash-local extension_host
pnpm --dir examples/extensions/local-hello run test
```

## Phase 3: Host API Registry and Built-in Capabilities

- [x] 在 `agentdash-local/src/extensions/host/` 增加 Host API registry/dispatcher。
- [x] 将 `local.get_profile` 迁入 registry entry。
- [x] 实现 `runtime.invoke` entry，并明确 recursion/trace/admission 规则。
- [x] 实现 `extension.channel_invoke` entry，并明确 provider/consumer/dependency/trace/admission 规则。
- [x] 实现 HTTP entry，包含 URL parse、method/body/header contract、timeout 和 response normalization；host declaration 仅在保留为安装摘要或诊断信息时参与。
- [x] 实现 workspace/VFS entry，复用现有 workspace root/path safety helper。
- [x] 实现 env/secret entry，显式 allowlist、redaction 和 missing secret 错误。
- [x] 实现 process entry，支持通用 shell/process command、cwd boundary、timeout、output size limit、exit code capture。
- [ ] 为每个 entry 增加 permission allowed/denied tests、参数非法 tests 和错误消息 tests。
- [x] 为 channel invocation 增加 provider missing、dependency missing、version mismatch、permission denied、recursive call depth tests。
- [x] 将 permission denied 测试聚焦在仍有产品价值的边界上；对已清理的过度门禁删除对应测试。

Validation:

```powershell
cargo test -p agentdash-local extension_host
cargo test -p agentdash-domain extension
```

## Phase 4: Protocol Channel Registry, RuntimeGateway and Projection Alignment

- [x] 更新 extension runtime projection，让新增 capability/permission/protocol channel/dependency 可被前端、Canvas 和 admission 使用。
- [x] RuntimeGateway extension provider 在 action invocation 阶段校验 Project installation、action declaration、package artifact、backend target 和 permission summary。
- [x] RuntimeGateway extension provider 在 channel invocation 阶段校验 provider installation、consumer identity、dependency declaration、channel method declaration、package artifact、backend target 和 permission summary。
- [ ] Gateway 与 local host 使用同一 domain evaluator 或同构 helper，避免同一 manifest 两边裁决不同。
- [x] 输出 metadata/trace 包含 provider extension key/id、consumer identity、action/channel key、method、capability family、backend id、invocation id。
- [x] trace 同时记录 channel alias 与 canonical provider extension/channel，确保 sugar 不影响审计。
- [x] 为 Canvas runtime bridge 定义 extension channel consumer contract，确保 Canvas 能按 Project/session context 调用插件信道。

Validation:

```powershell
cargo test -p agentdash-application extension
cargo test -p agentdash-api extension
```

## Phase 5: Frontend Webview Bridge

- [x] 对齐 `@agentdash/extension-ui` 与 `ExtensionWebviewPanel` 支持的方法。
- [x] 增加 panel/Canvas-facing `extension.invoke_channel` bridge method，参数只包含 channel key、method、input，Project/session/backend/actor/context 由宿主组装。
- [x] 接通 panel VFS read/write 或从 SDK 中移除未实现声明；本任务倾向接通。
- [x] 定义 events 的宿主语义：panel-local、workspace-level，或 extension runtime event；实现与文档保持一致。
- [x] 增加 bridge model/ui tests，覆盖 method params、unknown method、permission/admission error 显示。
- [x] 保持 WorkspacePanel dynamic tab 仍从 Project extension runtime projection 生成。

Validation:

```powershell
pnpm --filter @agentdash/extension-ui typecheck
pnpm --filter app-web test -- extension-runtime
pnpm --filter app-web typecheck
```

## Phase 6: Example Extension

- [x] 更新或新增示例插件，覆盖三类 action：
  - built-in Host API：`profile`
  - pure TS：`greet` / `echo`
  - user protocol adapter：`fetch_demo` / `list_items`
  - protocol channel provider/consumer：`demo_channel` / `consume_demo_channel`
- [x] 新增独立 `examples/extensions/protocol-demo`；`local-hello` 保持最小 Host API 示例。
- [x] 示例 manifest 声明所有 top-level capabilities 与 action-level permissions。
- [x] 示例 manifest 声明 protocol channel provider surface 与 consumer dependency。
- [x] 示例代码使用 self-channel shortcut 和 dependency alias，避免在闭合调用里手写自己的插件名。
- [x] 示例 panel 调用多个 action，并展示成功/错误状态。
- [x] 示例 tests 覆盖 pure TS action、protocol adapter mock、channel provider/consumer、permission declaration。
- [x] `pack` 后 archive 可通过前端 Assets 页安装，并在 session WorkspacePanel 打开试用。

Validation:

```powershell
pnpm --dir examples/extensions/local-hello run validate
pnpm --dir examples/extensions/local-hello run test
pnpm --dir examples/extensions/local-hello run pack
pnpm dev
```

Manual check:

- [x] 前端上传 packaged archive。
- [x] 从归档安装到 Project。
- [x] 在 session WorkspacePanel 打开插件 panel。
- [x] 调用 profile、pure TS、protocol adapter action。
- [x] 调用 provider channel，并验证 consumer 插件或 Canvas-like panel 经统一 bridge 访问。
- [x] 缺权限或 backend 离线时显示可诊断错误。

## Phase 7: Docs and Spec Update

- [x] 新增 `docs/extension-system.md`。
- [x] 更新示例 README，链接主文档。
- [x] 更新 `.trellis/spec/cross-layer/desktop-local-runtime.md`，记录 Host API registry、用户自有 TS host bundle 与 built-in capability 的职责边界。
- [x] 更新 `.trellis/spec/cross-layer/shared-library-contract.md`，记录新的 extension permission/capability/channel/dependency schema。
- [x] 更新 `.trellis/spec/frontend/architecture.md` 或 type-safety/state-management 中与 bridge/projection 相关的长期约束。

Docs acceptance:

- [x] 文档说明 `getProfile` 从哪里来、为什么是 built-in host capability。
- [x] 文档说明如何写一个纯 TS action。
- [x] 文档说明如何写一个用户自有协议 adapter，并通过 Host API facade 获得 HTTP/env/VFS/process 能力。
- [x] 文档说明如何注册 protocol channel，以及其它插件/Canvas 如何声明依赖并调用该信道。
- [x] 文档说明 self-channel shortcut、dependency alias、Canvas binding alias 与 canonical key 的关系。
- [x] 文档说明 process/shell 当前按本机可信工具模型提供通用执行，不按 Marketplace 安全模型约束。
- [x] 文档说明权限声明的定位：服务安装摘要、依赖解析、可用性诊断和审计，不为可信本机工具堆叠过度门禁。
- [x] 文档说明 pack/install/frontend trial flow。

## Phase 8: Final Verification

- [x] `cargo test -p agentdash-domain extension`
- [x] `cargo test -p agentdash-local extensions::host::tests`
- [x] `cargo test -p agentdash-application extension`
- [x] `cargo test -p agentdash-api extension`
- [x] `pnpm --filter @agentdash/extension-sdk typecheck`
- [x] `pnpm --filter @agentdash/extension-ui typecheck`
- [x] `pnpm --filter @agentdash/extension-dev test`
- [x] `pnpm --filter app-web typecheck`
- [x] Example `validate/test/pack`
- [x] 前端安装和试用插件手工验证

## Risk Files

- `packages/extension-sdk/src/index.ts`
- `packages/extension-ui/src/index.ts`
- `packages/extension-dev/src/manifest.js`
- `packages/extension-dev/src/pack.js`
- `crates/agentdash-domain/src/shared_library/value_objects.rs`
- `crates/agentdash-local/src/extensions/host/*`
- `crates/agentdash-application/src/runtime_gateway/*`
- `packages/app-web/src/features/extension-runtime/*`
- `examples/extensions/*`
- `docs/extension-system.md`

## Review Gate Before Start

- 当前无剩余 open question；用户 review 通过后即可 `task.py start`。
