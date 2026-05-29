# Contract Pipeline Unify Implementation Plan

## Phase 0: Planning Repair

- [x] 确认任务已处于 `in_progress` 但缺少复杂任务必需的 `design.md` / `implement.md`。
- [x] 确认仓库没有 `.github` CI 配置；本任务的 drift gate 先接入根 `package.json` 的 `check` 链路和现有 `pnpm run contracts:check`。
- [x] 明确 spec 冲突：旧 spec 要求 service mapper 校验 `unknown`，本任务决策改为内部 API 信任 generated wire。
- [x] 提交规划修正，作为后续代码改造的独立 commit。

## Phase 1: Core DTOs Into Contracts

- [x] 在 `agentdash-contracts` 增加 `project` / `story` / `task` / `workspace` contract DTO。
- [x] 为 core DTO 涉及的 domain enum/value object 建立 contract wire DTO，并使用穷尽映射，避免 domain 引用 contract 或新增 domain `TS` derive。
- [x] 注册 `generate_ts.rs`，让上述 DTO 生成到 `packages/app-web/src/generated/core-contracts.ts`。
- [x] API handler 改用 contract response 类型，删除 `crates/agentdash-api/src/dto` 中对应重复 response。
- [x] Project access 的 application mapping 保留在 API 层 helper 中，避免 contract crate 依赖 application。
- [x] 运行 `cargo fmt`。
- [x] 运行 `cargo check -p agentdash-contracts -p agentdash-api`。

## Phase 2: Frontend Type Single Source

- [x] 运行 `pnpm run contracts:generate`，提交 generated TS。
- [x] 删除 `packages/app-web/src/types/index.ts` 中与 generated 重复的 Project / Workspace / Story / Task wire 类型。
- [x] 修正前端 imports，让跨层 DTO 从 `src/generated/*` 或 re-export 入口读取。
- [x] 运行 `pnpm -C packages/app-web exec tsc --noEmit`。

## Phase 3: Mapper Removal

- [x] `services/extensionRuntime.ts` 内部 endpoint 直接返回 generated contract response。
- [x] `services/session.ts` 删除 generated DTO 的逐字段 identity mapper，保留真正 view model 或非 generated route-local DTO 转换。
- [x] `services/workflow.ts` 删除 generated DTO 的逐字段 identity mapper，保留 UI view model 转换。
- [x] 更新 PRD 的 mapper 保留清单，逐项说明仍需转换的字段。
- [x] 运行 `pnpm -C packages/app-web exec tsc --noEmit`。

## Phase 4: JsonValue Single Definition

- [x] 调整 `generate_ts.rs`，生成共享 `common-contracts.ts`。
- [x] 让其它 generated contract 文件 import 共享 `JsonValue`，`rg "export type JsonValue" packages/app-web/src/generated` 只剩 1 个命中。
- [x] 运行 `pnpm run contracts:check`。

## Phase 5: Contract Mirror Cleanup

- [x] 删除 `agentdash-contracts` 中仅镜像 domain 的 `McpTransportConfig` / `MountCapability` / `ProjectVfsMountContent` 命名副本。
- [x] 让生成契约从真正 contract/domain wire source 获取这些类型。
- [x] 运行 `rg "struct McpTransportConfig|enum MountCapability|struct ProjectVfsMountContent" crates/agentdash-contracts`，期望 0。
- [x] 运行 `cargo check -p agentdash-contracts -p agentdash-api`。

## Phase 6: Gate And Spec

- [x] 根 `package.json` 的默认 `check` 链路包含 `contracts:check`。
- [x] 更新 cross-layer/frontend spec，记录内部 API 信任 generated wire 的原因和 mapper 适用边界。
- [x] 运行 `pnpm run contracts:check`。
- [x] 运行 `cargo check --workspace`。
- [x] 运行 `pnpm -C packages/app-web exec tsc --noEmit`。
- [x] 更新 wave2 `progress-checklist.md` 和本任务 PRD AC。
- [ ] 归档本 child，并继续下一个 wave2 child。
