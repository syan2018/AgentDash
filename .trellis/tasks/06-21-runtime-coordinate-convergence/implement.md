# Runtime Coordinate 执行计划

## Phase 1: Design Lock

- [x] 定义 AgentRun current delivery binding 字段与持久化位置，详见 `research/rc02-implementation-scope.md`。
- [x] 定义 `DeliveryRuntimeSelectionService` 输入、输出和错误语义，详见 `research/rc02-implementation-scope.md`。
- [x] 明确 repository raw latest API 命名与允许使用范围。

## RC02 Implementation Slice

- [ ] Domain: add `LifecycleAgentCurrentDeliveryBinding`, `DeliveryBindingStatus`, `LifecycleAgent.current_delivery` helpers and slug roundtrip tests.
- [ ] Migration: add nullable current delivery binding columns to `lifecycle_agents`, with status check constraint and runtime-session lookup index.
- [ ] Infrastructure: extend Postgres lifecycle agent row mapping, insert/select/update/list roundtrip and partial-row validation.
- [ ] Application: add `DeliveryRuntimeSelectionService` with `CurrentDelivery`, `RunScopedLatest`, `LaunchPrimary` policies; define `SubjectLatestObserved` boundary without implementing history.
- [ ] Dispatch/write points: after anchor upsert, persist `Ready` binding; after accepted turn commits new current frame, update binding status to `Running` without replacing launch frame evidence.
- [ ] Tests: cover domain helper, repository roundtrip, selection errors, dispatch binding write and accepted-turn binding update.

## Phase 2: Consumer Migration

- [ ] Workspace query 使用 unified selection。
- [ ] Cancel / subject execution control 使用 unified selection。
- [ ] Mailbox delivery target 使用 unified selection。
- [ ] API route-local duplicate resolver 移除或改为调用 selection service。

## Phase 3: Projection

- [ ] SubjectExecutionView 增加 execution history。
- [ ] `latest_runtime_node` 从 history 派生。
- [ ] AgentRun resource surface DTO 表达 frame/source coordinate。

## Validation

```powershell
cargo test -p agentdash-domain lifecycle_agent_current_delivery
cargo test -p agentdash-application delivery_runtime_selection
cargo test -p agentdash-infrastructure lifecycle_agent_current_delivery
cargo test -p agentdash-application accepted_turn_commits_agent_frame_revision_and_current_frame
cargo test -p agentdash-application lifecycle
cargo test -p agentdash-application agent_run
cargo test -p agentdash-domain workflow
cargo check -p agentdash-application
cargo check -p agentdash-infrastructure
```

`pnpm run contracts:check` is not required for RC02 unless RC07/RC08 browser DTO changes are included.
