# W4: Companion Child Result To Parent Mailbox

Status: planned

## Goal

Child `companion_respond` resolve child-owned gate 后，将结果作为 parent AgentRun mailbox message 投递，而不是仅向 parent runtime 注入 notification。

## Dependencies

- W0 source/schema baseline 完成。
- W1 mailbox intake command shape 完成。
- W3 companion sub dispatch 完成或至少确定 dispatch/gate correlation shape。

## Deliverables

- [ ] `complete_child_result_to_parent` resolve child-owned gate 后创建 parent mailbox message，source 使用 `companion_result`。
- [ ] parent message dedup key 使用 `companion_result:{gate_id}` 或等价稳定 key。
- [ ] gate payload 记录 parent mailbox message ref / command receipt ref / delivery outcome。
- [ ] parent/child runtime notification 降为 UI hint，不再作为唯一交付证据。

## Acceptance

- [ ] parent idle 时 result 触发 parent AgentRun 继续处理。
- [ ] parent running 时 result 按 mailbox boundary queue。
- [ ] parent paused / failed 时 result 保持 pending，用户可 manual resume。
- [ ] duplicate child respond 不创建重复 parent input。
- [ ] gate resolve 与 mailbox delivery 任一失败时有可恢复状态。

## Suggested Validation

- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-application-agentrun mailbox`

## Parallel Guidance

W4 不建议与 W5/W6 同时编辑同一批 Companion gate/tool 文件。最优方案是 W4 作为 Companion gate delivery adapter 的第一条落地路径，合并后再派发 W5/W6。

