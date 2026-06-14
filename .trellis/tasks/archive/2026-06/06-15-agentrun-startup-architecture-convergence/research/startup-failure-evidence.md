# Startup Failure Evidence

## Incident Timestamp

The observed log timestamp `2026-06-14T16:01:13Z` is `2026-06-15 00:01:13 +0800` locally.

The frontend WebSocket error at `2026-06-14T16:01:46Z` aligns with the backend process terminating after `tokio-rt-worker` stack overflow. The WebSocket `10054` is a consequence of the Rust backend exit, not the primary frontend fault.

## Database Snapshot

Latest failed runtime session:

- session id: `58d2bba6-b98b-4fdf-99c4-e1d053eb3dbb`
- title: `新会话 · 06/15 00:01`
- created: `2026-06-15 00:01:46.484+08`
- status projection: `last_delivery_status=idle`
- no `last_turn_id`
- no `executor_session_id`

Command receipts:

- `project_agent_start` receipt `75719ab5-0843-41f2-ae02-a96808234063`
  - status: `pending`
  - created: `2026-06-15 00:01:46.478573+08`
  - no run/agent/frame/runtime/turn accepted refs
- first `agent_run_message` receipt `40b233d6-6e50-4ded-9736-3714bc825961`
  - status: `pending`
  - created: `2026-06-15 00:01:46.498063+08`
  - no accepted refs

Control plane partially materialized:

- LifecycleRun `8a8a72fc-701a-4256-b63e-7f10b58fa201` status `ready`
- LifecycleAgent `9ea6a66d-7a73-49a2-9bd1-60421e212cf7` status `active`
- current frame `59fa4ce5-6804-429e-94fb-5a85bb4d2e20`
- agent bootstrap status `pending`
- latest frame contains empty capability/context/vfs/mcp/canvas/module surfaces

Mailbox:

- message `165bc17b-2883-4a2e-8dfb-08f67e89d299`
- delivery `launch_or_continue_turn`
- barrier `immediate_if_idle`
- status `consuming`
- preview `你好你好，听得见么？`

This means the backend crashed after initial control-plane creation and mailbox claim, before final launch surface/turn accepted commit.

## Relevant Commits

`d7a11421 refactor(agentrun): 收敛 mailbox-first 消息投递`

- Introduced the ProjectAgent start path where outer start creates control plane, submits the first user message to mailbox, then waits for inner launch to complete before marking the outer receipt accepted.
- Best explains the observed half-created outer/inner receipt state.

`4d5ab4d2 refactor(runtime): 收束本机运行时装配与扩展契约`

- Changed runtime tool composer/provider injection.
- High-risk because runtime tools are built during `TurnPreparer`, before connector accepted and before final frame/turn commit.

`5f6a34bd refactor(mcp): 收束 MCP 执行面与边界模型`

- Initially suspicious because recursive schema handling can cause stack overflow.
- Lower priority for this incident because the local DB had no MCP presets and no project extension installations.

## Code Paths

- `crates/agentdash-api/src/routes/project_agents.rs`
- `crates/agentdash-application/src/workflow/project_agent_run_start.rs`
- `crates/agentdash-application/src/session/agent_run_mailbox.rs`
- `crates/agentdash-application/src/workflow/agent_message.rs`
- `crates/agentdash-application/src/session/launch/orchestrator.rs`
- `crates/agentdash-application/src/session/launch/preparation.rs`
- `crates/agentdash-application/src/session/launch/deps.rs`
- `crates/agentdash-api/src/bootstrap/session.rs`
- `crates/agentdash-application/src/session/mailbox_delegate.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`

## Architecture Implication

The failure is not just a stack overflow bug. It exposes that ProjectAgent start, AgentRun Mailbox, SessionLaunch, runtime tools, and runtime delegate stages have overlapping ownership of one startup lifecycle.

The corrected model should remove nested startup ownership rather than hardening each half-state as if it were a desired runtime state.
