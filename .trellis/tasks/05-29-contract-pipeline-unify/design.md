# Contract Pipeline Unify Design

## Objective

本任务把浏览器消费的业务 HTTP DTO 收敛到 `agentdash-contracts`，让 Rust serde wire shape、TypeScript generated 文件、API handler 输出和前端 service 类型来自同一个契约源。目标不是增加运行时兼容层，而是让 drift 在 `pnpm run contracts:check` / TypeScript / Rust 编译中暴露。

## Current Split

当前仍有一条手同步线：`crates/agentdash-api/src/dto/{task,story,workspace,project}.rs` 定义 response struct，前端 `packages/app-web/src/types/index.ts` 再手写 `Task` / `Story` / `Workspace` / `Project` 等类型。与此同时，已有 `agentdash-contracts` 能生成 VFS、Session、Workflow、Extension、Project Agent、LLM Provider 等契约。

此任务按“generated contract 是 wire DTO 事实源”处理：API 可以有局部 route query/body wrapper，但跨 feature 复用、前端消费或流式消费的 request/response DTO 必须进入 `agentdash-contracts`。

## Architectural Boundaries

- 依赖方向是 `agentdash-domain <- agentdash-contracts <- agentdash-api`。Domain 不能引用 contract/protocol/DTO；contract 作为应用边界外侧的 wire DTO 层，可以依赖 domain 做显式转换。
- Generated DTO 不直接暴露 domain entity。即使字段语义来自 domain，也优先在 `agentdash-contracts` 定义 wire DTO / wire enum，并通过穷尽 `From` / helper 映射 domain 值，避免为了 TS 生成把 `TS` derive 继续扩散到 domain。
- `agentdash-contracts` 不能依赖 `agentdash-application` 或 `agentdash-api`。Project access 里来自 `ProjectAuthorization` 的字段在 API 层适配成 contract DTO，避免 contract crate 反向依赖 application。
- Domain 不在本任务新增 `TS` / `schemars` 暴露面；后续 `domain-purification` 会继续把 domain 从生成职责里剥离。本任务只移动 API-facing DTO 和必要的 wire value object。
- Frontend 内部端点信任 generated wire：service 函数直接 `api.get<GeneratedType>()` / `api.post<GeneratedType>()`，不再用 identity mapper 逐字段重建相同对象。
- View model 可以保留，但必须表达 UI 语义差异，例如 `AgentPresetConfig` 对 JSON blob 的收窄、局部 nullable state 或排序分组结果；它不负责重声明后端 enum/string union。

## Target Contract Shape

新增或扩展 `agentdash-contracts` 模块：

- `project.rs`
  - `ProjectResponse`
  - `ProjectAccessSummaryResponse`
  - `ProjectSubjectGrantResponse`
  - `ProjectDetailResponse`
- `story.rs`
  - `StoryResponse`
- `task.rs`
  - `TaskResponse`
- `workspace.rs`
  - `WorkspaceBindingResponse`
  - `WorkspaceResponse`

这些类型注册到 `generate_ts.rs`，生成 `project-contracts.ts`、`story-contracts.ts`、`task-contracts.ts`、`workspace-contracts.ts`，或在实现时按现有生成文件组织合并到一个清晰的 core 文件。选择标准是 import 面最稳定、不会把 unrelated domain 都塞进一个超大文件。

## Mapping Strategy

- 简单 domain entity 到 response 的转换可以放在 `agentdash-contracts` 中实现 `From<T>`，因为 contract crate 已依赖 domain；转换必须是穷尽映射，不能用字符串兜底或 wildcard 掩盖新增枚举值。
- Domain enum/value object 若要进入前端，先成为 contract wire enum/value DTO，再由 `From<domain::...>` 显式映射。这样 Rust 后端仍有 domain model 和 DTO model 两层，前端则只消费 generated DTO 单源。
- 需要 application-only 输入的转换留在 API 层函数中，例如 `ProjectAuthorization -> ProjectAccessSummaryResponse`。由于 orphan rule，API 层不为外部类型实现 `From`，而是使用局部 helper。
- API route 返回类型改为 contract DTO，`crates/agentdash-api/src/dto` 中对应重复定义删除或只保留当前任务外的 DTO。

## Generated JsonValue

`ts-rs` 当前会在多个 generated 文件中重复发射 `JsonValue`。本任务把 generator 改为生成共享 `common-contracts.ts`，其它 generated 文件通过 import 使用该类型。实现时优先在 generator 收口，不在前端手工删除 generated 内容，保证 `contracts:check` 能重现相同输出。

## Frontend Cleanup

- `packages/app-web/src/types/index.ts` 删除与 generated 重复的 `Task` / `TaskStatus` / `Story` / `StoryStatus` / `Workspace` / `Project` 等 wire type。
- 需要继续跨 feature 导出的类型从 generated 文件 re-export，或者在使用处直接 import generated contract。
- `services/extensionRuntime.ts` / `services/session.ts` / `services/workflow.ts` 的 identity mapper 按“wire 信任”删除。保留项必须是 view model 转换或外部输入归一化，并记录在 PRD 的 mapper 保留清单。

## Spec Updates

当前 `.trellis/spec/cross-layer/frontend-backend-contracts.md`、`.trellis/spec/frontend/type-safety.md`、`.trellis/spec/frontend/state-management.md` 仍把 service mapper 描述成必需运行时校验边界。本任务实现后要同步改成：

- generated contract 是内部 API wire DTO 事实源；
- 内部 API service 信任 generated wire；
- mapper 只在 UI view model 转换、外部/用户输入、第三方 payload 或非 generated legacy surface 中存在。

## Validation

- `pnpm run contracts:generate`
- `pnpm run contracts:check`
- `cargo check -p agentdash-contracts -p agentdash-api`
- `pnpm -C packages/app-web exec tsc --noEmit`
- 任务完成前按父级 gate 再运行 `cargo check --workspace`。
