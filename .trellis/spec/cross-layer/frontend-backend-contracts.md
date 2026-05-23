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
  -> frontend service mapper / reducer
```

`agentdash-contracts` 是业务 DTO 的归属 crate。它承载 HTTP request/response DTO、NDJSON envelope、跨端共享 enum 和少量 wire value object。`agentdash-api` 使用 contract crate 作为 route 输入输出类型；前端只从 generated 文件消费这些类型。

当前 `agentdash-agent-protocol` 继续承载 Backbone Protocol。它已经是独立 protocol crate，后续只负责 runtime event fact；业务 HTTP DTO 不继续塞入该 crate。

## Invariants

- 业务 HTTP JSON 默认使用 `snake_case`，生成类型保持 Rust serde 字段名。
- Generated TypeScript 只落在 `packages/app-web/src/generated/`，文件头必须注明生成命令。
- 每个生成入口必须有 check mode；CI 或 `pnpm run contracts:check` 用 check mode 发现 drift。
- Frontend service mapper 负责 `unknown -> generated type` 的基础运行时验证和业务归一化；字段名、enum 值和 union 形态不在前端重新定义。
- Route-local DTO 只用于极小的 transport wrapper；跨 feature 复用、前端消费或流式传输的 DTO 必须进入 contract crate。
- NDJSON stream 的 `connected` / `event` / `heartbeat` envelope 也属于 contract，原因是续传游标、事件事实和 reducer 输入需要跨后端与前端共同演进。

## Contract Crate Shape

当前结构从 MCP Preset 开始落地，后续 domain 按同一布局扩展：

```text
crates/agentdash-contracts/
  src/
    lib.rs
    generate_ts.rs
    mcp_preset.rs        # MCP preset CRUD/probe DTO
    session.rs           # Session event page DTO / NDJSON envelope / runtime projection
    workflow.rs          # WorkflowContract / lifecycle / activity DTO
    vfs.rs               # ResolvedVfsSurface / mount / edit capability DTO
    shared_library.rs    # Library asset install/publish DTO
    project_agent.rs     # ProjectAgent config/session summary DTO
```

生成输出按领域拆文件：

```text
packages/app-web/src/generated/
  backbone-protocol.ts
  session-contracts.ts
  workflow-contracts.ts
  vfs-contracts.ts
  shared-library-contracts.ts
  mcp-preset-contracts.ts
  project-agent-contracts.ts
```

## DTO Priority

| Priority | Domain | Why |
| --- | --- | --- |
| P0 | Session stream envelope / Session context DTO | NDJSON reducer、runtime surface、Backbone fact 共同消费，漂移会直接影响会话 UI |
| P0 | Workflow contract / lifecycle / activity DTO | 前端当前有大量 enum normalizer，后端模型变化频繁 |
| P1 | VFS surface / mount DTO | Workspace panel、VFS browser、surface mutation 共用同一地址模型 |
| P1 | MCP Preset DTO | CRUD、probe、publish 复用，适合作为首个小域迁移 |
| P2 | Shared Library / ProjectAgent DTO | 涉及 marketplace、资产安装和 agent preset 配置，适合在 P0/P1 模式稳定后迁移 |

## Migration Plan

1. 保持 `agentdash-agent-protocol` 的 Backbone generated file，并使用 `contracts:check` 做 drift gate。
2. `agentdash-contracts` 先迁移 MCP Preset 与 Session stream envelope 这类边界清晰的 DTO。
3. API route 改为使用 contract crate DTO；frontend service 改为 import generated type。
4. Mapper 中保留运行时校验，但删除 enum/string 联合类型的手写重复定义。
5. Workflow/VFS 这类大域按 value object 分批迁移，迁移一批就删除对应前端手写类型。

## Validation

```powershell
pnpm run contracts:check
cargo check -p agentdash-agent-protocol
pnpm run frontend:check
```

当 `agentdash-contracts` 引入后，`contracts:check` 同时运行所有 contract 生成器。
