# WI-01 RuntimeSession Product Internalization

## Objective

把 RuntimeSession 从产品写控制面中移除。用户可见写操作统一进入 AgentRun scoped API，RuntimeSession 只作为 internal trace / diagnostic surface。

## Decisions

D-001, D-003, D-015

## Research Inputs

- `research/runtime-session-internal-model.md`
- `research/projection-permission-api-frontend.md`
- `references/adversarial-first-principles-review.md`

## Scope

- 清点 raw `/sessions/*` 中承担产品写语义的 route：fork、rollback、delete、title/meta patch、tool approval、runtime-control mutation。
- 将产品写能力迁移到 AgentRun scoped application service。
- contracts 中把 runtime session id 从 product command result / workspace identity 降级为 diagnostic trace meta。
- frontend workspace state、service、command availability 不再以 `sessionId` 作为产品前置条件。

## Out Of Scope

- 不拆 SessionPersistence；交给 WI-02。
- 不决定 current delivery 物理形态；交给 WI-06。
- 不整体重建 permission model；交给 WI-09。

## Dependencies

依赖 WI-00 的 route/DTO/frontend state inventory。

## Implementation Notes

- raw session route 可以保留 diagnostic read capability。
- AgentRun scoped endpoint 不应复用 raw session route handler；若存在共享逻辑，应抽成 application service。
- tool approval 先按 runtime connector approval 处理；若 WI-09 确认它是产品可恢复决策，再创建 AgentRun product fact。

## Acceptance

- 用户写操作无法通过 raw RuntimeSession route 绕过 AgentRun control plane。
- AgentRun workspace 初始化、submit、cancel、tool interaction 不要求持有 product-level `runtime_session_id`。
- contracts 中产品 DTO 使用 `AgentRunRef` / `AgentRef` 作为主 identity。
- raw session API 文档和命名表达 diagnostic / trace 定位。

## Validation

- `rg "runtime_session_id|sessionId|/sessions"` 覆盖 api/contracts/frontend。
- 运行相关 API/contract/frontend 类型检查。
- 手动验证 AgentRun workspace 基本路径仍从 `run_id + agent_id` 工作。
