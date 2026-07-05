# WI-06 Delivery Binding Anchor

## Objective

明确 `RuntimeSessionExecutionAnchor` 与 current delivery selection 的边界：anchor 是 immutable evidence，current delivery 是单独的 binding/state 或可重建 selection。

## Decisions

D-003, D-010, D-014, D-017

## Research Inputs

- `research/runtime-session-internal-model.md`
- `research/aggregate-ownership.md`
- `research/database-physical-design.md`

## Scope

- 将 anchor 改为 insert-once / idempotent create。
- 删除 anchor upsert 改写坐标语义。
- 删除 `latest_updated_anchor_for_agent` 作为业务 current selection API。
- 验证并决定 current delivery 物理形态：AgentRun child binding、materialized read model、或由 anchor + live state + applied frame resolver 推导。
- 所有 current delivery 写入通过单一边界。

## Out Of Scope

- 不处理 mailbox owner；交给 WI-04。
- 不重建 RuntimeSession trace store；交给 WI-02。
- 不做 frontend product identity cleanup；交给 WI-09。

## Dependencies

依赖 WI-00 inventory。WI-05 和 WI-08 依赖本项给出的 delivery/current selection 语义。

## Implementation Notes

- 若 current delivery 需要参与决策，命名为 binding/state，并具备 reconcile 规则。
- 若 current delivery 只是 read model，必须声明重建输入和丢失后的行为。
- `LifecycleAgent` 身份记录不应混入 live runtime pointer。

## Acceptance

- anchor 不再被更新为不同 runtime/frame/turn 坐标。
- current delivery 有唯一选择策略。
- delivery selection 不依赖可丢失 projection 做业务判断。
- AgentRun delete / runtime cleanup 的 FK/cascade 与 current binding 一致。

## Validation

- anchor create idempotency 测试。
- runtime session 轮换后 current delivery selection 测试。
- `rg "latest_updated_anchor_for_agent|upsert.*anchor"` 无业务选择残留。

## Implementation Record 2026-07-04 / Worker C1

### Delivery Binding / Anchor Boundary

- `RuntimeSessionExecutionAnchor` 保持 launch evidence 语义；fork materialization 的事务写入改为 insert-once，重复写入只在 launch coordinates 完全一致时幂等成功。
- current delivery 的事实源是 `agent_run_delivery_bindings` 的 `(run_id, agent_id)` binding/state；delivery selection 通过 binding 查 runtime session，再用 anchor 校验 run/agent/frame/node 坐标。
- `LifecycleAgent` 身份聚合不再携带 current delivery 字段；历史 `current_delivery_*` schema 迁移和删除仍由 0044 记录。

### Code Touchpoints

- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs`：删除 fork materialization 中 anchor coordinate rewrite，改为 `create_anchor_once_tx`。
- `crates/agentdash-workspace-module/src/workspace_module/surface.rs` 和 `crates/agentdash-workspace-module/src/workspace_module/tools.rs`：测试 fake repository 与 `RuntimeSessionExecutionAnchorRepository::create_once` 对齐，并移除 latest-updated anchor selector。

### Validation Notes

- `cargo fmt --check` 通过。
- `cargo check -p agentdash-domain -p agentdash-application-ports -p agentdash-infrastructure -p agentdash-application-lifecycle -p agentdash-application-agentrun -p agentdash-api` 通过。
- `cargo test -p agentdash-infrastructure agent_run_lineage_row_maps_json_and_refs --lib` 通过，确认 fork materialization 所在 infrastructure crate 的 test build 可用。
- `cargo test -p agentdash-application-agentrun current_delivery --lib` 和 `cargo test -p agentdash-application-agentrun execution_anchor_create_once --lib` 通过，覆盖 current delivery binding selection、anchor mismatch 拒绝和 create-once idempotent/conflict 语义。
- `cargo test -p agentdash-application-lifecycle current_delivery --lib` 通过，确认 lifecycle view 从 delivery binding 读取 current delivery status。
- `cargo test -p agentdash-infrastructure delivery_binding --lib` 通过，确认 delivery binding repository roundtrip。
