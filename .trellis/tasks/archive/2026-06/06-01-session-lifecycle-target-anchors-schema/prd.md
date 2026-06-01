# Session Lifecycle 目标锚点 Schema

## 目标

新增并接入控制面目标锚点 schema，让 `LifecycleRun.session_id`、`LifecycleRun.lifecycle_id` 单 graph 指针、`ExecutorRunRef::AgentSession`、`LifecycleRunLink` 的旧主路径可以被迁出。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 依赖：`06-01-session-lifecycle-spec-convergence`

## 蓝图阶段

- 推进：`target-state-blueprint.md` B1 Target Anchor Schema。
- 退出贡献：在删除 legacy shortcut 之前，graph instances、agents、frames、assignments、associations、gates、agent lineages 的目标 tables/repositories/backfills 已存在。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 优先创建目标 schema 并切换 repository contracts，而不是双读旧/新结构。
- 系统可以在 dispatch、assignment、API demotion 等后续任务接上线前处于部分不可用状态。

## 需求

- 新增 migrations / repository / domain model：`lifecycle_workflow_instances`、`lifecycle_agents`、`agent_frames`、`agent_assignments`、`lifecycle_subject_associations`、`lifecycle_gates`、`agent_lineages`。
- `lifecycle_workflow_instances` 表达某个 `WorkflowGraph` 在 `LifecycleRun` 内的一次生效实例，至少包含 `run_id`、`graph_id`、`role`、`status`、`activity_state`、`created_at`。
- `AgentFrame` 首版使用 revision row，包含 procedure、activity key、capability/context/VFS/MCP surface、runtime session refs。
- `AgentAssignment`、`ActivityExecutionClaim`、`ActivityAttemptState` 的目标 key 必须能区分 `graph_instance_id + activity_key + attempt`。
- backfill root `LifecycleAgent` / `AgentFrame` from existing `LifecycleRun.session_id` and `ExecutorRunRef::AgentSession`。
- backfill root `WorkflowGraphInstance` from existing `LifecycleRun.lifecycle_id`。
- `LifecycleSubjectAssociation` 支持 whole-run 与 agent-scoped association，锚点只允许 run / LifecycleAgent。
- 复核并修复 `SessionMeta.project_id` 在 Postgres session repository 的 create/get/list/save 路径。

## 交付物

- 目标 schema / domain entities / repositories。
- 结构性 backfill：root graph instance、root agent/frame、whole-run subject association、可确定的 agent lineage。
- `SessionMeta.project_id` 持久读写修复。
- `design.md` 与 `implement.md` 中声明的 repository contract。

## 不承担

- 不切业务入口。
- 不完成 AgentFrame builder 或 connector launch 改造。
- 不完成 scheduler/terminal assignment 语义迁移。

## 验收标准

- [ ] 新 schema 可表达 run -> agent -> frame -> runtime session refs。
- [ ] 新 schema 可表达 run -> multiple workflow graph instances -> activity state/attempts。
- [ ] 旧 run/session 快捷关系可被 backfill 到 AgentFrame runtime refs。
- [ ] 旧 run/lifecycle graph 指针可被 backfill 到 root WorkflowGraphInstance。
- [ ] 可确定的 agent parent/child relation 可写入 `agent_lineages`，session lineage 保持 trace/debug 语义。
- [ ] repository/API 内部可以通过 runtime session 找到 frame/agent/run，但 RuntimeSession 不直接拥有业务 owner。
- [ ] `SessionMeta.project_id` 持久读写有测试或等价验证。
- [ ] migration 不保留兼容双轨，只保留必要 backfill 与最终目标结构。
