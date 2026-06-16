# W3 Contracts + Generated TS

## 状态

done

## 依赖

- W2 done

## 目标

建立 Task plan、Story Task projection 和 Run-scoped Task command 的 Rust wire contract，并同步 generated TypeScript。

## 输入

- W1 / W2 稳定后的 domain 和 repository shape。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/frontend/type-safety.md`
- `crates/agentdash-contracts/src/task/contract.rs`
- `crates/agentdash-contracts/src/story/contract.rs`
- `crates/agentdash-contracts/src/runtime/workflow.rs`
- `packages/app-web/src/generated/task-contracts.ts`
- `packages/app-web/src/generated/workflow-contracts.ts`

## 范围

- 更新 Task plan DTO、Task projection DTO、Run-scoped Task command request / response。
- `TaskResponse` 不再包含 `dispatch_preference`、`artifacts` 或 execution status。
- Story Task projection DTO 表达来源关系。
- Task execution view 使用 `SubjectExecutionView`。
- 重新生成 TypeScript contracts。

## 范围边界

- 该节点只稳定 wire contract 与 generated TS，原因是前端和 MCP 需要消费同一份生成类型。
- UI 迁移进入 W5，原因是类型错误应暴露旧 surface，而不是通过本地兼容字段掩盖。

## 验收

- `pnpm run contracts:check` 通过。
- generated TaskStatus 只包含 `open / active / review / blocked / done / dropped`。
- generated Task plan DTO 不含 `dispatch_preference`、`artifacts` 或 runtime execution status。
- 前端 type errors 可作为 W5 收口输入，不通过手写别名掩盖。

## 产出记录

- `crates/agentdash-contracts/src/task/contract.rs` 已切到 LifecycleRun plan item wire contract：
  - `TaskStatus` / `TaskPlanStatus` 只包含 `open / active / review / blocked / done / dropped`。
  - `TaskResponse` 表达 `owning_run_id`、plan 字段、agent assignment 字段、`context_refs` 和可选 `story_ref`。
  - 新增 `RunTaskPlanResponse`、`CreateRunTaskRequest`、`UpdateRunTaskRequest`、`UpdateRunTaskStatusRequest`、`RunTaskCommandResponse`。
  - 不再导出 `TaskDispatchPreference`、`Artifact`、`ArtifactType`。
- `crates/agentdash-contracts/src/story/contract.rs` 新增 Story Task projection DTO：
  - `StoryTaskProjectionResponse`
  - `StoryTaskProjectionItem`
  - `StoryTaskProjectionSource`
  - `StoryTaskProjectionSourceKind = owning_run / linked_run / story_ref`
- `crates/agentdash-contracts/src/generate_ts.rs` 已同步 task/story 导出，并让 task contract 先于 story contract 生成，保持 `TaskResponse` 归属 `task-contracts.ts`。
- `packages/app-web/src/generated/task-contracts.ts` 与 `packages/app-web/src/generated/story-contracts.ts` 已重新生成。

## 验证记录

- `cargo check -p agentdash-contracts` 通过。
- `pnpm run contracts:generate` 通过。
- `pnpm run contracts:check` 通过。

## 风险与交接

- W4 以此 contract 作为 API/read model 输出边界。
- W5/W6/W7 不得重新定义 DTO 或状态集合。
- 旧 Story-scoped Task API、前端 service/store/UI 和 MCP 仍引用旧 `TaskResponse::from(domain::task::Task)`、`dispatch_preference`、`artifacts` 与旧状态；这些类型错误应由 W4/W5/W6 收口，不在 W3 做兼容字段。
- Task runtime artifacts、latest runtime node 和 linked runs 继续通过 `SubjectExecutionView` 消费；W4 不需要新增 Task 专属 runtime DTO。
