# Relay Runtime Wire Placement Transport

## Goal

将 Relay 从第二套 session runtime/薄 prompt backend 收敛为 AgentDash-owned Runtime Wire 的透明 placement transport。

## Depends On

- `01-runtime-contract`
- `04-integration-driver-host`

## Parent Design

- `../../design.md` 第 6.4、15 节
- `../../research/protocol-runtime.md`

## Requirements

- 传输typed command/receipt/event/descriptor，不嵌`serde_json::Value` Backbone。
- 保持canonical coordinates、service provenance、binding generation与operation correlation。
- 实现frame sequence、ack/cursor/replay、backpressure、disconnect/timeout。
- 具体Agent adapter在local driver host终止；cloud不再重跑第二套SessionRuntime。
- transport profile参与effective profile intersection，只能保持或削弱service guarantee。
- 删除RelayPromptRequest、内存session sink owner猜测与缺失terminal producer路径。

## Acceptance Criteria

- [ ] Relay不拥有Agent service identity、tool/context业务语义或connector capability。
- [ ] authoritative event在lag/reconnect下可重放且duplicate幂等。
- [ ] active turn断连exactly-one Lost，不产生Completed。
- [ ] service provenance跨placement保持不变。
- [ ] local/remote路径共享同一Managed Runtime state machine。
- [ ] untyped Backbone value转发与薄prompt协议删除。
