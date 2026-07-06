# Integration Map

## Existing Runtime Spine

- Relay already carries extension runtime action and protocol channel through `command.extension_action_invoke` / `command.extension_channel_invoke` and matching responses in `crates/agentdash-relay/src/protocol.rs`.
- Backend service relay payload structs already exist in `crates/agentdash-relay/src/protocol/extension_runtime.rs`; the next integration step is registering them as first-class `RelayMessage` variants and routing pending responses.
- Local extension command handling is centralized in `crates/agentdash-local/src/handlers/extension.rs`. The backendService path should extend this handler family so artifact cache, backend id, workspace root and metadata behavior stay aligned with action/channel invocation.
- Extension artifact cache already downloads and unpacks package archives by `artifact_id + archive_digest` in `crates/agentdash-local/src/extensions/artifact_cache.rs`.
- Workspace Module operation projection already uses `operation_catalog` as the Agent operation source and filters `panel_only`; backendService currently projects as unavailable readiness in `crates/agentdash-workspace-module/src/workspace_module/mod.rs` and returns a diagnostic in `surface.rs`.

## Integration Direction

- Treat backendService invoke as the third extension runtime transport beside action and channel.
- Cloud/API must send invoke intent and package artifact metadata; local runtime performs private service access.
- Workspace Module should consume a backendService invoker abstraction rather than reaching into relay details directly.
- Panel fetch route and Agent operation should converge on the same backendService invoke contract; panel-only still stays hidden from Agent describe/invoke.
- Readiness and diagnostics should be current-state behavior from local runtime, not a compatibility mode.

## Merge Watchpoints

- Keep `backend_id`, `project_id`, `extension_key/id`, `service_key`, `route`, `trace_id` and `invocation_id` in metadata.
- Keep generated TypeScript contracts synchronized when Rust DTOs change.
- Avoid adding automatic localhost discovery; manifest `backend_services[]` plus explicit `fetch_routes[]` remain the lifecycle and routing entry.
