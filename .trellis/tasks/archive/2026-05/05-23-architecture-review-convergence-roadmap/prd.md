# 架构 Review 收敛路线图

## Goal

将 2026-05-23 架构 review 汇总拆解为可独立推进、可验收、可排序的 Trellis 子任务，让后续重构先锁 correctness，再逐步收敛组合根、Session pipeline、持久化分层、模块边界和前端契约。

## Source Material

- `docs/reviews/2026-05-23-architecture-review-round/architecture-review-synthesis.md`
- `docs/reviews/2026-05-23-architecture-review-round/runtime-control-plane-review.md`
- `docs/reviews/2026-05-23-architecture-review-round/platform-boundary-governance-review.md`
- `docs/reviews/2026-05-16-zip-static-review/session-launch-refactor-plan.md`

## Requirements

- 建立一个父任务，集中跟踪架构 review 后续重构路线。
- 拆出彼此独立的子任务，每个子任务都有明确目标、范围、验收标准和建议验证方式。
- 子任务之间的执行顺序应体现风险控制：先修运行时正确性，再做结构拆分，再做跨层契约和模块化。
- review 中已经过期的工程完整性问题不得作为待办重复创建；当前工作区已存在 lockfile、workspace、scripts、docs、tests。
- 后续实现任务应按子任务分别进入 Trellis Phase 1/2，不在父任务中直接编码。

## Child Tasks

| 顺序 | 子任务 | 优先级 | 依赖关系 |
| --- | --- | --- | --- |
| 1 | `05-23-relay-runtime-correctness` | P0 | 可立即启动 |
| 2 | `05-23-appstate-bootstrap-kernel-split` | P1 | 建议在 Relay correctness 后启动 |
| 3 | `05-23-session-launchplan-pipeline` | P1 | 可与 AppState 拆分并行设计，实施时需协调 |
| 4 | `05-23-database-schema-source-governance` | P1 | 先做决策与 spec 更新，再执行 schema 策略调整 |
| 5 | `05-23-persistence-ports-layering` | P2 | 依赖 schema/分层基线决策 |
| 6 | `05-23-workflow-vfs-relay-module-boundaries` | P2 | 依赖前几项确定的边界，适合分批拆 |
| 7 | `05-23-frontend-contract-generation-state-convergence` | P2 | 依赖 DTO 生成范围与后端 contract 稳定 |

## Acceptance Criteria

- [ ] 所有子任务已创建并挂到本父任务下。
- [ ] 每个子任务 `prd.md` 包含目标、范围、要求、验收标准和验证建议。
- [ ] 复杂子任务具备 `design.md` 与 `implement.md` 草案，可作为后续 Phase 1 继续细化的起点。
- [ ] 父任务清楚记录推荐执行顺序和跨任务依赖。
- [ ] 未把历史 zip 快照中已解决的缺文件问题列为当前重构任务。

## Out of Scope

- 不在本父任务中直接修改业务代码。
- 不直接启动任何子任务进入 implementation。
- 不在 review 文档中替代 `.trellis/spec/` 的稳定架构契约；实现后需要按任务更新对应 spec。
