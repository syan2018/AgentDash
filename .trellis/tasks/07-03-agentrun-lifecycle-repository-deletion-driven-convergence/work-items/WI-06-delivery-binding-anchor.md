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
