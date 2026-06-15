# AgentRun runtime entry session 残留收束

## Goal

把 RuntimeSession 从业务 runtime 入口中继续降级为 message stream / connector trace substrate，并收束当前散落的 session-first API、resolver、hook、mailbox、launch planner 与 workspace query 入口。

目标模型：除消息流、transcript、terminal/connector trace 读取之外，外部业务入口统一使用 AgentRun control-plane identity，例如 run / agent / frame / lineage / orchestration node coordinate。RuntimeSession 只作为可选 message stream projection，不再承担 AgentRun、orchestration node 或业务权限归属的入口语义。

本任务是 `06-15-agentrun-lifecycle-surface-projector` 的并行子线路。它先做 review 和路线设计，可与主线 projector 实现并行推进；实现阶段应拆成可独立验收的小步。

## Background

当前代码中 session 残留分布较广：

- session launch / hub / connector start 仍以 `session_id` 作为大量方法签名的入口。
- `RuntimeSessionExecutionAnchor` 以 `runtime_session_id` 为 lookup key，并记录 run / agent / frame / optional orchestration node reference。
- workflow projection、task view、hook、mailbox 等路径存在 `resolve_by_runtime_session` / `find_by_session` 形式的反查。
- VFS lifecycle surface 当前仍有 `agent_run_session` scope 命名，容易把 message stream evidence 误读成业务 owner。
- specs 已多处强调 RuntimeSession 是 trace substrate，但代码入口仍不够统一。

这些残留会持续制造模型噪音：调用方容易从 session 推断业务状态、orchestration 关系或权限边界，而不是从 AgentRun control-plane identity 出发。

## Requirements

- 审计所有 session-first runtime 入口，区分：
  - 必须保留的 message stream / connector trace API。
  - 应迁移为 AgentRun runtime address 的业务 API。
  - 应迁移为 orchestration node coordinate 的 node execution API。
  - 仅测试或兼容夹层中暂存的 session lookup。
- 设计统一的 AgentRun runtime address / command target / surface target 模型。
- 明确 RuntimeSession 在目标架构中的合法出现位置：
  - message stream
  - transcript / compaction
  - connector runtime trace
  - terminal stream / event replay
- 明确 RuntimeSession 不应再作为以下语义的入口：
  - AgentRun workspace / resource surface
  - lifecycle node execution ownership
  - mailbox command routing
  - hook control target
  - capability / permission ownership
  - subject association lookup
- 为迁移制定分阶段路线，避免一次性大改所有 session launch / hub 代码。
- 与主线 `AgentRunLifecycleSurfaceProjector` 对齐：projector 使用 AgentRun runtime address，message stream 只作为 optional projection ref。
- 输出可执行子任务建议，每个子任务包含文件范围、迁移原则、测试关注点。

## Acceptance Criteria

- [ ] research 文档列出 session-first 入口清单，并按 message stream / business runtime / orchestration node / tests 分类。
- [ ] design 文档定义 AgentRun runtime address、message stream ref、orchestration node coordinate 的边界。
- [ ] design 文档明确 RuntimeSession 的合法使用范围和禁止作为业务入口的范围。
- [ ] implement 文档拆出 2-4 个可并行或顺序执行的收束阶段。
- [ ] 路线中明确哪些路径应先改为 wrapper / adapter，哪些可以直接迁移。
- [ ] 路线中明确与 `AgentRunLifecycleSurfaceProjector` 主任务的依赖和互不阻塞点。
- [ ] 后续实现后，新增 runtime surface / mailbox / hook / workspace API 不再要求以 `runtime_session_id` 作为业务入口。

## Scope

- Session model review。
- Runtime entry convergence design。
- AgentRun-first target model。
- Migration plan and test plan。
- Specs follow-up list。

## Out Of Scope

- 本任务的 review 阶段不直接重写 session hub / connector runtime。
- 不改变消息流、transcript、terminal trace 对 RuntimeSession 的合法使用。
- 不把 orchestration node execution 迁移到 session-owned subtree。

## Dependency Notes

- 与主任务 `06-15-agentrun-lifecycle-surface-projector` 并行。
- 主任务应先采用 `AgentRunRuntimeAddress + Option<MessageStreamProjectionRef> + Option<OrchestrationNodeProjectionInput>` 的 projector contract。
- 本子任务后续迁移 mailbox / hook / launch planner 时，应复用主任务沉淀的 address/ref 命名和 projection facts。

## Planning Status

- 当前状态：planning / review。
- 下一步：subagent 审计代码与 specs，产出 research/session-entry-audit.md；主会话根据审计结果补齐 design.md / implement.md。
