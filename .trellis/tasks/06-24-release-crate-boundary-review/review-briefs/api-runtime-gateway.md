# Brief: API And RuntimeGateway Consumers

Active task: .trellis/tasks/06-24-release-crate-boundary-review

You are `review-api-gateway`. Review API and RuntimeGateway consumers that currently couple to session or AgentFrame.

Write your final report to `.trellis/tasks/06-24-release-crate-boundary-review/research/03-api-runtime-gateway-consumers.md`.

Scope:

- Inspect `crates/agentdash-api/src/app_state.rs`, `bootstrap/runtime_gateway.rs`, `session_construction.rs`, `runtime_bridge.rs`, `routes/canvases.rs`, `routes/extension_runtime.rs`, `routes/vfs_surfaces/**`, `routes/terminals.rs`, `routes/sessions.rs`, `routes/lifecycle_views.rs`.
- Inspect `crates/agentdash-application/src/runtime_gateway/**`.
- Trace `RuntimeSessionMcpAccess`, `mcp.list_tools`, `mcp.call_tool`, `resolve_session_frame_vfs`, `get_current_runtime_backend_anchor`, current frame resolver and RuntimeSessionExecutionAnchor usage.
- For each consumer, state the target application facade and regression tests needed.

Use repository evidence and file paths. Do not modify source code.
