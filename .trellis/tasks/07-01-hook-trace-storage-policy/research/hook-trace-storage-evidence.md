# Hook trace storage evidence

## Durable event inflation

The lifecycle event screenshot shows 1087 total events and 598 `platform` events. Platform events alone are 55.0% of the event log.

## Trace production paths

- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:235` builds `HookTraceEntry` in `record_trace` and calls `hook_runtime.append_trace`.
- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:509` records `UserPromptSubmit` `noop` when no injection or pending turn-start message exists.
- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:598` records `BeforeTool` `allow` when no deny/ask/rewrite exists.
- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:630` records `AfterTool` as `effects_applied` when no refresh is requested.
- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:676` records `AfterTurn` `noop` unconditionally.
- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:719` records `BeforeStop` `stop` for natural stop.
- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs:795` records `BeforeProviderRequest` `observed` unconditionally.

## Broadcast and persistence path

- `crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs:399` appends trace to a 200-entry memory ring and sends it through `trace_broadcast`.
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:936` receives hook traces and converts each one into `PlatformEvent::HookTrace`.
- `crates/agentdash-application-runtime-session/src/session/eventing.rs:1481` defines ephemeral event kinds. `PlatformEvent::HookTrace` is not currently ephemeral, so it defaults to durable append.
- `crates/agentdash-application-runtime-session/src/session/hub/hook_dispatch.rs:102` builds HookTrace for hub-triggered session hooks and persists it as fallback when no live executor session exists.

## Existing frontend filtering

- `packages/app-web/src/features/session/model/systemEventPolicy.ts:53` already classifies `stop`, `terminal_observed`, `observed`, `refresh_requested`, `allow`, `effects_applied`, `noop`, `notified`, `context_injected`, and `steering_injected` as silent hook decisions.
- `packages/app-web/src/features/session/model/systemEventPolicy.ts:91` renders silent HookTrace only when it carries meaningful block/completion/injection/diagnostic data.
- `packages/app-web/src/features/session/model/useSessionFeed.test.ts:649` verifies observed hook trace does not split tool bursts.

## Existing spec intent

- `.trellis/spec/backend/hooks/execution-hook-runtime.md:182` defines Hook Event Stream behavior.
- `.trellis/spec/backend/hooks/execution-hook-runtime.md:185` says pure noise traces such as `noop/allow/effects_applied` do not have to enter the event stream.
- `.trellis/spec/backend/hooks/execution-hook-runtime.md:186` says traces with `matched_rule_keys / diagnostics / completion / block_reason` must be emitted.
