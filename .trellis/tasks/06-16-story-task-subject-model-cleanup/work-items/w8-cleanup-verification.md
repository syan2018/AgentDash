# W8 Cleanup + Verification

## 状态

pending

## 依赖

- W5 done
- W6 done
- W7 done

## 目标

完成旧模型清理、总体验证、spec finish 和提交前风险记录。

## 输入

- W5 / W6 / W7 的产出和交接。
- `implement.md` 验证命令。
- `.trellis/spec/backend/story-task-runtime.md`
- `.trellis/spec/backend/repository-pattern.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/frontend/state-management.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## 范围

- 搜索并清理旧 TaskStatus、旧 `/tasks/{id}` 心智、`dispatch_preference`、`task.artifacts`、Story Task durable CRUD。
- 确认 Task API 第一版只以 Run / AgentRun workspace 为作用域。
- 确认 Story 页面只消费 Story Task projection。
- 确认 runtime artifacts、latest runtime node、linked runs 只由 SubjectExecutionView / Lifecycle projection 承担。
- 更新 focused tests / E2E 断言方向。
- 更新长期 spec，只记录新边界为什么成立。
- 跑总体验证命令并记录未覆盖风险。

## 验收

- `cargo check --workspace` 按风险面执行并记录结果。
- `pnpm run migration:guard` 通过。
- `pnpm run contracts:check` 通过。
- `pnpm run frontend:check` 通过。
- LifecycleRun aggregate、repository、Story projection、SubjectExecutionView、MCP、frontend focused tests 覆盖关键路径。
- repo 搜索旧字段只剩 migration / 历史 research / 明确允许的文档语境。

## 产出记录

- 待填写。

## 风险与交接

- 待填写。
