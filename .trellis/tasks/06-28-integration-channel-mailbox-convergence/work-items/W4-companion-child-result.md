# W4: Companion Child Result To Parent Mailbox

Status: implemented

## Goal

Child `companion_respond` resolve child-owned gate 后，将结果作为 parent AgentRun mailbox message 投递，而不是仅向 parent runtime 注入 notification。

## Dependencies

- W0 source identity model 完成。
- W1 mailbox intake command shape 完成。
- W3 companion sub dispatch 完成或至少确定 dispatch/gate correlation shape。

## Deliverables

- [x] `complete_child_result_to_parent` resolve child-owned gate 后创建 parent mailbox message，source identity 使用 `namespace=companion`、`kind=result`、`source_ref=gate_id`。
- [x] parent message dedup key 使用 `companion_result:{gate_id}` 或等价稳定 key。
- [x] gate payload 记录 parent mailbox message ref / command receipt ref / delivery outcome。
- [x] parent/child runtime notification 降为 UI hint，不再作为唯一交付证据。

## Acceptance

- [x] parent idle 时 result 触发 parent AgentRun 继续处理。
- [x] parent running 时 result 按 mailbox boundary queue。
- [x] parent paused / failed 时 result 保持 pending，用户可 manual resume。
- [x] duplicate child respond 不创建重复 parent input。
- [x] gate resolve 与 mailbox delivery 任一失败时有可恢复状态。

## Implementation Notes

- `CompanionGateControlService` 通过 `CompanionParentMailboxDelivery` adapter 投递 child result，业务 gate service 不直接依赖 AgentRun mailbox service。
- `AgentRunCompanionParentMailboxDelivery` 在 companion tool 层构造 `companion/result` source identity，并把 parent result 文本写入 parent mailbox。
- Gate payload 的 `parent_mailbox_delivery` 子对象记录 delivery status、mailbox message、command receipt、outcome 和 accepted refs；runtime event 只作为 parent/child UI hint。

## Suggested Validation

- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-application-agentrun mailbox`

## Validation Results

- `cargo fmt --check` passed.
- `cargo test -p agentdash-application companion` passed.
- `cargo test -p agentdash-application-agentrun mailbox` passed.
- `cargo check -p agentdash-api` passed.

## Parallel Guidance

W4 不建议与 W5/W6 同时编辑同一批 Companion gate/tool 文件。最优方案是 W4 作为 Companion gate delivery adapter 的第一条落地路径，合并后再派发 W5/W6。
