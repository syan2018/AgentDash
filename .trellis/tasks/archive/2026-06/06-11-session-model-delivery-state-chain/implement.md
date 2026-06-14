# 实施计划

## Parent Role

本任务作为父级执行合同，保持 planning，实际源码改动通过 child tasks 分批推进。每个 child task 必须在自身 artifact 中写明依赖、涉及层、交付物和验收标准。

## Child Order

1. `06-11-agentrun-runtime-trace-meta-convergence`
   - 先拆清 `SessionMeta` 哪些字段保留为 RuntimeSession trace meta，哪些 public projection 上移到 AgentRun Workspace shell/list/status。
   - 输出 DTO 命名建议、受影响 API/frontend 文件清单和 spec 更新点，供 API contract child 使用。
2. `06-11-agentrun-workspace-api-contract`
   - 先固定 AgentRun Workspace DTO、route、command request/response 字段。
   - 输出 generated contracts，供 frontend child 使用。
3. `06-11-agentrun-delivery-command-receipts`
   - 基于已确定 command request 字段实现 idempotency receipt。
   - 输出后端 focused tests。
4. `06-11-launch-frame-hook-atomicity`
   - 可与 command receipt 并行，但合并前要确认 command accepted 状态与 launch accepted boundary 一致。
   - 输出 launch failure 和 hook stale cache 回归测试。
5. `06-11-agentrun-workspace-frontend-route-state`
   - 在 contract 可用后迁移路由、页面、状态、模型选择和 command id。
   - 输出 frontend focused tests 和手动 draft-to-workspace 验证。

## Parent Verification

- `python ./.trellis/scripts/task.py validate ./.trellis/tasks/06-11-session-model-delivery-state-chain`
- 对五个 child task 分别运行 `task.py validate`
- `rg -n "/session/new|/session/:sessionId|SessionPage" packages/app-web/src` 在最终集成后不再命中交互工作台路径
- `rg -n "Hook runtime target mismatch" crates/agentdash-application/src` 只保留真实数据错误文本和回归测试断言
- `pnpm run contracts:check` 在 API contract child 后通过
- `pnpm run migration:guard` 在 command receipt child 新增 migration 后通过
- `.trellis/spec/backend/session/runtime-execution-state.md` 与 `.trellis/spec/cross-layer/frontend-backend-contracts.md` 在最终集成后表达 AgentRun Workspace public command identity

## User Decision

- 已确认采用彻底迁移方案：移除 `/session` 交互入口，canonical route 使用 `/agent-runs/:runId/:agentId`。
- 主会话后续负责恢复上下文、主持 child task 实现、协调 sub-agents、最终质检、提交与推送。
- child task artifacts 已通过 validate；进入实现时仍按 Trellis 流程显式 `task.py start` 对应 child task。

## Resume Protocol

上下文恢复时优先读取本父任务的 `prd.md`、`design.md`、`implement.md` 和 `review.md`，再按 Child Order 读取各 child task。`task.py current` 可能指向最后创建的 child task，这是任务创建顺序带来的指针状态，不代表实施顺序。

主会话作为主持者推进：先启动 runtime trace meta child 固定 `SessionMeta` 边界，再启动 API contract child 固定合同；随后并行或分批推进 command receipt 与 launch/hook child，最后启动 frontend route/state child 消费生成合同。每个 child 的实施结果进入自身 `implement.jsonl`，质检结果进入自身 `check.jsonl`，父任务只记录跨任务决策和最终集成验收。

## Pre-Implementation Notes

- `RuntimeSession` 在当前规范中定位为 delivery / trace substrate；实现时不得把 RuntimeSession route 或 session title 作为工作台事实源。
- `SessionMeta` 只保留 trace/delivery metadata；工作台 title/status/list shell 进入 AgentRun Workspace projection。
- HTTP wire DTO 归 `agentdash-contracts`，TypeScript 由 `pnpm run contracts:check` 校验 drift；frontend child 只消费 generated DTO 和明确 view model。
- Command receipt 属于用户投递幂等，不复用 `session_runtime_commands`，因为后者是 runtime context/frame transition delivery outbox。
- 数据库变更通过新增 migration 推进，按常规任务不得修改既有 migration。
