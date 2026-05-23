# Module Boundary Split Plan

## Decision

Workflow, VFS, Relay protocol, and Agent loop will be split as separate batches. These areas sit in different layers and carry different invariants, so moving them together would make behavior review too noisy.

## Batch Order

| Batch | Scope | Why |
| --- | --- | --- |
| Workflow value objects | `workflow/value_objects.rs` into contract, lifecycle, activity, run state, tool capability, validation | Mostly domain types and pure validation; lowest behavioral risk and high readability gain. |
| VFS boundaries | core, providers, tools, mutation, materialization, surface | VFS is the resource/capability boundary for agents, so provider and tool surfaces need visible ownership. |
| Relay protocol payloads | handshake, prompt, workspace, tool, MCP, terminal, session event, capabilities | Relay is a protocol bus; payload domains should be reviewable without changing wire format. |
| Agent loop internals | turn, tool call, event mapping, cancellation, prompt, output | Agent loop carries streaming side effects, so it follows after lower-risk domain/protocol splits. |

Each batch keeps public facades or re-exports, preserves serialized names and protocol shapes, and runs focused check/test commands before completion.
