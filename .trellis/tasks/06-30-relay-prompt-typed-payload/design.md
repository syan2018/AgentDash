# Technical Design

## Current Split

The canonical input representation already exists:

- RuntimeSession `UserPromptInput.input: Option<Vec<UserInputBlock>>`.
- AgentRun launch boundary mirrors that shape.
- Relay steer already carries typed `Vec<UserInputBlock>`.

Relay prompt is the split path:

- `RelayPromptRequest.prompt_blocks: Option<serde_json::Value>`.
- `CommandPromptPayload.prompt_blocks: Option<serde_json::Value>`.
- Cloud converts `UserInputBlock` into ACP `ContentBlock` JSON.
- Local parses ACP `ContentBlock` JSON back into `UserInputBlock`.

## Target Contract

Relay prompt uses typed user input:

```rust
pub struct RelayPromptRequest {
    pub input: Vec<UserInputBlock>,
    // existing fields preserved
}

pub struct CommandPromptPayload {
    pub input: Vec<UserInputBlock>,
    // existing fields preserved
}
```

`PromptPayload::Text` should be converted once to text `UserInputBlock` before creating the relay request. `PromptPayload::Input` passes through as-is.

## Files / Areas

Expected areas:

- `crates/agentdash-application-ports/src/backend_transport.rs`
- `crates/agentdash-relay/src/protocol/prompt.rs`
- `crates/agentdash-application/src/relay_connector.rs`
- `crates/agentdash-local/src/handlers/prompt.rs`
- protocol / local handler / relay connector tests near those modules
- generated TypeScript only if relay prompt contracts are exported

## Constraints

- Do not change AgentRun command availability, AgentRuntimeDelegate, or admission boundaries.
- Do not introduce compatibility fallback fields.
- Do not move ACP conversion helpers except to delete relay-specific conversions; true ACP/model edge helpers can remain.
- Avoid broad Rust compile; use targeted tests/checks.
