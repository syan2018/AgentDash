# Runtime Coordinate 执行计划

## Phase 1: Design Lock

- [x] 定义 AgentRun current delivery binding 字段与持久化位置，详见 `research/rc02-implementation-scope.md`。
- [x] 定义 `DeliveryRuntimeSelectionService` 输入、输出和错误语义，详见 `research/rc02-implementation-scope.md`。
- [x] 明确 repository raw latest API 命名与允许使用范围。

## RC02 Implementation Slice

- [x] Domain: add `LifecycleAgentCurrentDeliveryBinding`, `DeliveryBindingStatus`, `LifecycleAgent.current_delivery` helpers and slug roundtrip tests.
- [x] Migration: add nullable current delivery binding columns to `lifecycle_agents`, with status check constraint and runtime-session lookup index.
- [x] Infrastructure: extend Postgres lifecycle agent row mapping, insert/select/update/list roundtrip and partial-row validation.
- [x] Application: add `DeliveryRuntimeSelectionService` with public `CurrentDelivery` selection only; raw anchor ordering remains repository/history evidence, not a public business policy.
- [x] Dispatch/write points: after anchor upsert, persist `Ready` binding; after accepted turn commits new current frame, update binding status to `Running` without replacing launch frame evidence.
- [x] Tests: cover domain helper, repository row mapping/partial validation, selection errors and policies.

## Phase 2: Consumer Migration

- [x] Workspace query 使用 unified selection：workspace detail/list delivery refs、command stale guard frame/runtime 校验、resource surface session evidence 均改用 `DeliveryRuntimeSelectionService::CurrentDelivery`；raw anchor latest 只保留为 workspace runtime refs 列表证据。
- [x] Cancel / subject execution control 使用 unified selection：subject execution cancel、terminal cancel reconcile、companion gate/control delivery target 均改用 `DeliveryRuntimeSelectionService::CurrentDelivery`；显式 runtime session 只作为 current delivery stale 校验。
- [x] Mailbox delivery target 使用 unified selection：mailbox command target 通过 `DeliveryRuntimeSelectionService::CurrentDelivery` 解析 current frame/runtime session，并移除 latest anchor fallback。
- [x] API route-local duplicate resolver 改为调用 `DeliveryRuntimeSelectionService::CurrentDelivery`；raw anchor 只保留历史 evidence。

## Phase 3: Projection

- [x] SubjectExecutionView 增加 execution history：`runtime_attempts` 表达 run / agent / runtime session / frame / orchestration node / attempt / status / observed_at / artifacts。
- [x] `latest_runtime_node` 从 history 首项派生。
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
cargo test -p agentdash-application workspace
cargo check -p agentdash-application
cargo check -p agentdash-infrastructure
```

`pnpm run contracts:check` is not required for RC02 unless RC07/RC08 browser DTO changes are included.
