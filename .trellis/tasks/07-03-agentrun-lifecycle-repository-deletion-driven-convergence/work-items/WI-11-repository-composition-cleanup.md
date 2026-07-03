# WI-11 Repository Composition Cleanup

## Objective

删除业务服务对全量 RepositorySet / service locator 的依赖，让 composition root 成为唯一装配位置。

## Decisions

D-016, D-018

## Research Inputs

- `research/aggregate-ownership.md`
- `research/runtime-session-internal-model.md`
- `research/database-physical-design.md`

## Scope

- 清理 AgentRun/Lifecycle application crate 中复制的大 repository set。
- 为主要 use case 定义小型 deps struct：
  - `AgentRunAdmissionDeps`
  - `AgentRunCommandDeps`
  - `AgentRunCommandQueueDeps`
  - `DeliveryAttemptDeps`
  - `AgentFrameRevisionDeps`
  - `AgentRunForkDeps`
  - `LifecycleStateDeps`
  - `RuntimeTraceDeps`
- 跨聚合写入通过显式 command port / unit of work。
- repository trait 命名与事实分类一致。

## Out Of Scope

- 不重新决定事实边界；依赖 WI-03 到 WI-10。
- 不单独做 schema migration；交给各事实工作项和 WI-12。

## Dependencies

依赖 WI-03 到 WI-10 的边界基本稳定。可以在前置项完成后分批清理。

## Implementation Notes

- 大 RepositorySet 可以留在 bootstrap/composition root 中，但不进入 application service 构造函数。
- 如果某个 service 仍需要大量依赖，应重新审查它是否跨了多个 use case。
- test fixture 也应按 use case deps 构造，避免测试继续掩盖 service locator。

## Acceptance

- application service 构造函数只暴露自身 use case 需要的能力。
- `RepositorySet` / `AgentRunRepositorySet` 不再作为业务层依赖类型出现。
- 新增或保留的 port 都能映射到被删除的旧组合方式。

## Validation

- `rg "RepositorySet|AgentRunRepositorySet|LifecycleRepositorySet"` 确认业务层残留。
- Rust 编译和 application service tests。
- 依赖图人工 review：每个 use case 的 deps 不跨越无关领域。
