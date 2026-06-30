# Relay prompt typed payload 收束

## Goal

实现 design backlog Slice 1 / D12：将 Relay prompt payload 从 raw `prompt_blocks` JSON / ACP ContentBlock 往返，收束为 canonical `UserInputBlock` typed payload。Relay prompt 与 RuntimeSession / AgentRun launch 使用同一套输入表示。

## Source

- Design review: `.trellis/tasks/06-30-design-backlog-review/design-review.md#d12-relay-prompt-typed-payload`
- Implementation slice: `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md#slice-1-relay-typed-prompt-payload`
- Research: `.trellis/tasks/06-30-design-backlog-review/research/01-runtime-surfaces.md`

## Requirements

- `CommandPromptPayload` 使用 `input: Vec<UserInputBlock>`，不再使用 raw `prompt_blocks` JSON。
- `RelayPromptRequest` 使用 `input: Vec<UserInputBlock>`，与 `RelaySteerRequest.input` 对齐。
- Cloud relay connector 不再把 canonical user input 转成 ACP ContentBlock JSON 后发给 local。
- Local prompt handler 不再解析 ACP ContentBlock JSON 回 canonical input。
- ACP / model provider conversion 只保留在真实 ACP/model adapter edge，不留在 AgentDash relay prompt 中间层。
- 不保留兼容 fallback 或双字段 shape；项目预研期直接改到正确 contract。
- 更新相关 serialization / cloud-local prompt handler tests。
- 如果 generated contracts 受影响，运行对应 generate/check。

## Acceptance Criteria

- [x] `rg "prompt_blocks"` 在 relay/app transport/local prompt runtime path 中无业务命中。
- [x] Relay prompt wire payload 能 serde roundtrip typed `UserInputBlock`。
- [x] Cloud relay prompt sends typed input.
- [x] Local prompt handler constructs `UserPromptInput { input: Some(...) }` directly from typed payload.
- [x] Existing steer typed input path remains unchanged.
- [x] Targeted Rust checks/tests pass, without running broad full workspace compile.
