# 前后端契约生成全量迁移

## Goal

建立 `agentdash-contracts` 作为业务 wire DTO 事实源，并按批次把前后端共享 DTO 从 route-local / frontend-handwritten 类型迁移到 Rust -> TypeScript 生成链路。

## Requirements

- 新增 `crates/agentdash-contracts`，只承载跨层 wire DTO、NDJSON envelope、共享 enum 和生成器；不承载 domain entity、repository record 或 application orchestration state。
- `agentdash-api` route 使用 contract DTO 作为请求/响应类型；route 仍负责鉴权、调用 application service、错误映射。
- 前端从 `packages/app-web/src/generated/*-contracts.ts` 消费 wire type；service mapper 只做 `unknown -> generated type` 的运行时校验和 view model 转换。
- 每个生成入口必须支持 check mode，并纳入 `pnpm run contracts:check`。
- 迁移按批次推进，单个提交只迁一个 bounded context。
- 不做兼容字段别名，不在前端同时支持 snake_case/camelCase 或旧 enum 值。

## Migration Batches

| Batch | Domain | Target |
| --- | --- | --- |
| 1 | MCP Preset | 建立 `agentdash-contracts` crate、生成 `mcp-preset-contracts.ts`，API/前端改用 generated DTO |
| 2 | Session stream/context | 生成 Session NDJSON envelope、Session context/runtime projection DTO，收敛 stream parser 类型 |
| 3 | Workflow | 生成 Workflow contract、Activity lifecycle、transition、port、capability config，删除前端重复 enum union |
| 4 | VFS | 生成 `ResolvedVfsSurface`、mount summary、edit capability、surface source DTO |
| 5 | Shared Library | 生成 Library asset install/publish/status DTO |
| 6 | ProjectAgent | 生成 ProjectAgent config、summary、session/open result DTO |

## Acceptance Criteria

- [x] `agentdash-contracts` crate 建立，并接入 workspace。
- [x] `contracts:generate` / `contracts:check` 同时覆盖 Backbone 与业务 contract 生成。
- [x] Batch 1 MCP Preset 完成：Rust DTO、API route、前端 generated type、service mapper 和 typecheck 全部通过。
- [x] Batch 2-6 的迁移边界在 `design.md` 中明确，后续可按批次提交。
- [x] `.trellis/spec/cross-layer/frontend-backend-contracts.md` 与 frontend type-safety/state-management/hook spec 保持同步。
- [x] 验证通过：`pnpm run contracts:check`、`cargo check -p agentdash-contracts -p agentdash-api`、`pnpm run frontend:check`。

## Out of Scope

- 不生成 infrastructure persistence record。
- 不把所有 domain entity 直接暴露给前端。
- 不迁移 UI view model；view model 可以继续由 generated DTO 转换得出。
