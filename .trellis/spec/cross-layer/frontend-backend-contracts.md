# Frontend / Backend Contracts

## Role

前后端契约层定义浏览器、云端 API、本机 runtime 和桌面壳共同消费的 wire DTO、事件 envelope 与生成产物。它的目标是让 JSON/NDJSON 形态由 Rust contract 明确表达，并由生成文件进入前端，而不是让前端长期手写后端 DTO。

## Architecture

标准链路：

```text
Rust contract type
  -> serde wire shape
  -> ts-rs TypeScript generation
  -> packages/app-web/src/generated/*
  -> frontend service / reducer
```

`agentdash-contracts` 是业务 DTO 的归属 crate。它承载 HTTP request/response DTO、NDJSON envelope、跨端共享 enum 和少量 wire value object。`agentdash-api` 使用 contract crate 作为 route 输入输出类型；前端只从 generated 文件消费这些类型。

当前 `agentdash-agent-protocol` 承载 Backbone Protocol 这类 runtime event fact；业务 HTTP DTO 归属 `agentdash-contracts`。

## Invariants

- 业务 HTTP JSON 默认使用 `snake_case`，生成类型保持 Rust serde 字段名。
- Generated TypeScript 只落在 `packages/app-web/src/generated/`，文件头必须注明生成命令。
- 每个生成入口必须有 check mode；CI 或 `pnpm run contracts:check` 用 check mode 发现 drift。
- Frontend service 对内部 API response 信任 generated wire type；字段名、enum 值和 union 形态不在前端重新定义。Mapper 只用于 UI view model 转换、外部/用户输入、第三方 payload，或尚未进入 contract crate 的 route-local 过渡 DTO。
- Route-local DTO 只用于极小的 transport wrapper；跨 feature 复用、前端消费或流式传输的 DTO 必须进入 contract crate。
- NDJSON stream 的 `connected` / `event` / `heartbeat` envelope 也属于 contract，原因是续传游标、事件事实和 reducer 输入需要跨后端与前端共同演进。
- Session turn 控制面复用 Codex app-server protocol 的 input 形态。浏览器发起运行中 steer 时，HTTP DTO 使用 `Vec<UserInput>`，后端服务继续携带 `expected_turn_id` 进入 session control / relay / connector，原因是 Codex `turn/steer` 的幂等前置条件必须由前端可见的实际运行状态一路传递到执行器。

## Contract Crate Shape

当前结构按业务域拆分：

```text
crates/agentdash-contracts/
  src/
    lib.rs
    generate_ts.rs
    mcp_preset.rs        # MCP preset CRUD/probe DTO
    session.rs           # Session event page DTO / NDJSON envelope / runtime projection
    extension_runtime.rs # Project extension runtime surface DTO
    extension_package.rs # Packaged extension artifact upload/install/download DTO
    workflow.rs          # AgentProcedureContract / lifecycle / activity DTO
    vfs.rs               # ResolvedVfsSurface / mount / edit capability DTO
    shared_library.rs    # Library asset install/publish DTO
    project_agent.rs     # ProjectAgent config/session summary DTO
```

生成输出按领域拆文件：

```text
packages/app-web/src/generated/
  backbone-protocol.ts
  session-contracts.ts
  extension-runtime-contracts.ts
  extension-package-contracts.ts
  workflow-contracts.ts
  vfs-contracts.ts
  shared-library-contracts.ts
  mcp-preset-contracts.ts
  project-agent-contracts.ts
```

## Current Baseline

`agentdash-contracts` 现在覆盖这些前端消费的业务 DTO：

| Domain | Generated File | Contract Source |
| --- | --- | --- |
| MCP Preset | `mcp-preset-contracts.ts` | `agentdash-contracts::mcp_preset` |
| Session event stream / projection view | `session-contracts.ts` | `agentdash-contracts::session` |
| Extension Runtime | `extension-runtime-contracts.ts` | `agentdash-contracts::extension_runtime` |
| Extension Package Artifact | `extension-package-contracts.ts` | `agentdash-contracts::extension_package` |
| Workflow / lifecycle / activity | `workflow-contracts.ts` | `agentdash-contracts::workflow` wire DTO |
| VFS surface / mount / Project VFS mount | `vfs-contracts.ts` | `agentdash-contracts::vfs` |
| Shared Library | `shared-library-contracts.ts` | `agentdash-contracts::shared_library` |
| Project Agent | `project-agent-contracts.ts` | `agentdash-contracts::project_agent` |
| LLM Provider | `llm-provider-contracts.ts` | `agentdash-contracts::llm_provider` |

API routes use contract DTOs for cross-feature HTTP input/output. When a route still needs an application/domain model internally, the API layer owns the mapping into contract DTOs.

Frontend type entrypoints re-export generated contracts directly when the wire shape is ergonomic for UI code. A feature may keep a small UI wrapper around generated contracts when the UI needs a narrower semantic type, such as `AgentPresetConfig` over a JSON blob or nullable view state over omitted wire fields. Service 层不为 generated DTO 做逐字段 identity rebuild，原因是 drift detection 已由 contract check、Rust 编译和 TypeScript 编译负责。

Session projection view DTOs expose `AgentContextEnvelope` provenance to the browser: segment origin, synthetic marker, source range, projection segment id and compaction metadata remain generated contract fields. Frontend service code consumes the generated projection response directly and must not redefine this projection shape outside generated session contracts.

Session branch DTOs also live in `agentdash-contracts::session`: fork request/response, lineage record/view and projection rollback response. Frontend service code consumes the generated relation/status unions and keeps session tree grouping keyed by backend-provided `parent_session_id` / `parent_relation_kind`.

LLM Provider DTOs live in `agentdash-contracts::llm_provider`，原因是管理员 Provider Catalog、用户 BYOK effective list、credential mode、probe 请求、用户凭据验证状态和 Codex OAuth 登录状态都由前端设置页与执行器 discovery 共同消费。前端 API 层消费 `generated/llm-provider-contracts.ts`，只保留 service 调用和轻量 view model 状态；`credential_mode`、`effective_api_key_source`、`global_api_key_configured`、`user_api_key_configured`、`user_credential_verification_status`、`CodexOAuthStatusResponse.status` 等字段不在前端手写重声明。

## Local Decisions

- Workflow wire DTOs live in `agentdash-contracts::workflow` because browser-facing TS generation is a protocol concern. `agentdash-domain::workflow` owns persisted/domain value objects and keeps serialization derives needed by persistence, but does not depend on `ts-rs` or `schemars`.
- Lifecycle steering request 使用 `LifecycleAgentSteeringRequest.input: Vec<codex::UserInput>`，原因是浏览器、云端 API、本机 backend 和 Codex bridge 都在表达同一个 `turn/steer` 命令；route 层只负责鉴权、anchor 解析和 generated DTO 校验，不重新发明 prompt block wire shape。
- VFS, Shared Library and Project Agent use narrow DTOs in `agentdash-contracts` because their API responses intentionally map application/domain internals into stable browser-facing shapes.
- Generated request/response DTOs model serde wire fields. UI-level convenience such as nullable fields, normalized config objects or derived aliases belongs in frontend type entrypoints rather than in the generated file.
- Project extension runtime surface 使用独立 `agentdash-contracts::extension_runtime` 与 `extension-runtime-contracts.ts`，原因是它是 Project enabled extension installations 派生出的全局 runtime surface，不属于 Shared Library marketplace/source-status，也不是 Session Context 私有字段。
- Extension package artifact 使用独立 `agentdash-contracts::extension_package` 与 `extension-package-contracts.ts`，原因是 packaged archive 的上传、安装引用和下载元数据是平台 artifact 契约，不属于 runtime projection 列表，也不属于 Shared Library payload。
- Workspace webview panel 通过 `POST /api/projects/{project_id}/extension-runtime/invoke-action` 进入 RuntimeGateway，父页面 bridge 负责补齐 session、backend 与 Project context，原因是 iframe 内插件 UI 只能发送 action key 与 input，不应持有主前端 token、store 或内部 API client。
- Packaged panel UI 通过 `GET /api/projects/{project_id}/extension-runtime/webviews/{extension_key}/{*asset_path}` 读取 artifact 内文件，服务端只允许读取已声明 workspace tab renderer entry 所在目录，原因是插件 UI 资源属于安装后的 Project artifact，而不是 Shared Library source payload。
- `canvas_panel` workspace tab renderer 复用 packaged panel asset 读取 contract，entry 指向包内 Canvas runtime snapshot，原因是 Canvas-derived extension 应与其它 packaged extension 共享 artifact/source-status/install 语义，同时复用现有 Canvas runtime preview。

## Validation

```powershell
pnpm run contracts:check
cargo check -p agentdash-agent-protocol
pnpm run frontend:check
```

当 `agentdash-contracts` 引入后，`contracts:check` 同时运行所有 contract 生成器。
