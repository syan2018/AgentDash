# WI-09 Projection Permission API Frontend

## Objective

让产品 API、权限、前端 workspace identity 收束到 AgentRun/Lifecycle 控制面；所有 projection/read model 明确可重建性。

## Decisions

D-001, D-014, D-015

## Research Inputs

- `research/projection-permission-api-frontend.md`
- `research/aggregate-ownership.md`

## Scope

- contracts 中 AgentRun product DTO 使用 `AgentRunRef` / `AgentRef` 作为主 identity。
- `runtime_session_id` 从 product command result、workspace list、workspace state、stale guard 中移到 diagnostic trace meta 或删除。
- permission check 从 AgentRun/Lifecycle control plane 进入。
- raw session diagnostic access 通过 AgentRun/Lifecycle 权限派生。
- 标注所有 projection/read model 的 rebuildability。
- frontend workspace state、service、route、command availability 与 AgentRun identity 对齐。

## Out Of Scope

- raw session product write 删除由 WI-01 负责。
- current delivery binding 由 WI-06 负责。
- Lifecycle view projection 的存储形态由 WI-10 负责。

## Dependencies

依赖 WI-00 inventory。与 WI-01 高度相关，实施时需要同步 contracts/frontend 改动。

## Implementation Notes

- delete run 权限需要明确是 owner 删除自己的 run，还是项目治理删除任意 run。
- stale guard 应优先使用 snapshot / run / frame / active turn / workspace revision，而不是 runtime session id。
- diagnostic trace panel 可以保留 runtime trace meta，但不能影响 product command availability。

## Acceptance

- 前端产品路径不需要 raw `sessionId` 才能 start、submit、cancel、fork、tool interaction。
- AgentRun workspace list 不展示或依赖 delivery runtime ref 作为产品状态事实。
- projection 清单标注 rebuild input、业务决策参与情况和丢失后的恢复方式。
- permission route 和 service 不从 RuntimeSession 反向成为产品授权入口。

## Validation

- contracts regenerate / frontend typecheck。
- AgentRun workspace 基本流程浏览器验证。
- `rg "runtime_session_id|sessionId|delivery_runtime_ref|source_runtime_session_id"` 清点产品路径残留。
