# Codex Thread Title Protocol Research

## Codex Protocol Surface

- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs` defines `Thread.preview` as the first user message preview and `Thread.name` as the optional user-facing thread title.
- `ThreadStartResponse`, `ThreadResumeResponse`, `ThreadForkResponse`, `ThreadMetadataUpdateResponse`, and `ThreadReadResponse` all return a full `Thread`.
- `references/codex/codex-rs/app-server-protocol/src/protocol/common.rs` defines:
  - `thread/name/set` request.
  - `thread/started` notification.
  - `thread/name/updated` notification.

## Codex Server Behavior

- `thread_summary.rs` derives `ConversationSummary.preview` from the first user message in rollout history.
- `thread_processor.rs::preview_from_rollout_items` repeats that preview extraction for thread responses.
- `thread_processor.rs::thread_set_name_response_inner` writes `Thread.name` as thread store metadata.
- `thread_processor.rs::attach_thread_name` attaches stored `name` to a response only when the title is non-empty and differs from `preview`.
- `external_agent_config_processor.rs` maps imported external-agent `custom-title` into `Thread.name`.

## AgentDash Current Gap

- `CodexBridgeConnector` reads only `response.thread.id` from `ThreadStartResponse` and `ThreadForkResponse`.
- `CodexBridgeConnector::handle_server_notification` does not handle `thread/started` or `thread/name/updated`.
- `PlatformEvent::SessionMetaUpdate` is a generic key/value extension channel; using it to represent Codex protocol messages would hide protocol semantics and bypass typed event handling.

## Design Implication

AgentDash should treat Codex `Thread.name` as a source-provided title fact. The connector maps Codex protocol messages into a typed Backbone platform event, and the application layer projects that event into `SessionMeta` under `title_source = source`. Local title derivation remains a fallback for sessions without a source title.
