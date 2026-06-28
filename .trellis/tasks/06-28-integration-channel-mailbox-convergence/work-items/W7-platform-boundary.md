# W7: Platform Boundary

Status: planned

## Goal

保留当前 `target=platform` missing broker diagnostic，同时明确未来 broker 接入必须产生 durable request fact，并在需要 AgentRun 继续处理时通过 mailbox 投递。

## Dependencies

- W0 source identity model 完成。
- 不依赖 W2-W6 的业务实现。

## Deliverables

- [ ] 保留当前 missing broker diagnostic。
- [ ] 在 design/spec 中明确 platform broker 后续必须产出 durable request fact。
- [ ] 明确 broker response 只有在需要 AgentRun 继续处理时才 materialize mailbox message。
- [ ] 防止 `target=platform` 后续回到 runtime-only notification delivery。

## Acceptance

- [ ] 当前 unsupported behavior 有测试覆盖。
- [ ] 后续 broker 接入有明确 mailbox/outbox 边界。

## Suggested Validation

- `cargo test -p agentdash-application companion platform`

## Parallel Guidance

W7 可以与 W2/W3 并行做文档和测试检查。若需要改 `companion/tools.rs`，应避开 W3/W5/W6 的代码窗口，避免同文件冲突。
