# Agent Runtime Contract、Wire 与 Conformance Harness

## Goal

建立 dependency-light、AgentDash-owned 的 Runtime vocabulary、Driver SPI、Wire、capability profiles 与共享行为测试，成为 Managed Runtime、Application、Integration Driver Host 和所有 adapter 的唯一公共语言。

## Parent Design

- `../../design.md`
- `../../implement.md` 第 3 节

## Requirements

- 定义 RuntimeThread/Turn/Item/Interaction/Operation/Binding/Checkpoint/Revision newtypes 与合法关系。
- 定义 `AgentRuntimeGateway` command/snapshot/event 类型和 driver command/event/descriptor/error SPI。
- 定义 Input/Instruction/Tool/Workspace/Interaction/Hook/Context/TelemetryConfig profiles，以及 HookRequirement/HookDeliveryGuarantee。
- Level 仅作为参考类别；command admission 使用 typed predicates。
- 定义 Runtime Wire typed request/response/notification、protocol revision 与 critical frame violation。
- Rust contract 同源生成 TypeScript/JSON Schema。
- 建立 state transition、exactly-one terminal、unsupported、profile intersection 与 availability conformance helpers。
- Contract 不依赖 application、repository、transport 或任何 vendor protocol。

## Acceptance Criteria

- [ ] Canonical/source IDs 在类型系统中不可混用。
- [ ] accepted、running、terminal、Lost 与 retryable error 语义有 executable tests。
- [ ] final Item authoritative、terminal 后 delta 非法有 executable tests。
- [ ] unsupported command 在 side effect 前返回 typed error，不存在 default no-op。
- [ ] service/transport/host policy profile intersection 与 CommandAvailability tests 通过。
- [ ] Rust/TypeScript/JSON Schema 由同一 owned contract 生成且不包含 vendor DTO。
- [ ] 新 contract crate 的依赖图满足 parent design。

## Out of Scope

- 不实现 concrete runtime state machine、database、driver 或 UI。
