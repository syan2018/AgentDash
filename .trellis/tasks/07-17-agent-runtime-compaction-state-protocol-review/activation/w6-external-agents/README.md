# W6 Codex / Remote Complete Agent activation component

## Identity

- Frozen base: `fc26d3ff`
- Runtime Wire target revision: `4`
- Ownership: Codex adapter, Remote Complete Agent proxy/endpoint, adapter-facing Wire usage
- Production composition owner: W8 hard cut

This component removes the adapter-owned driver, `RuntimeJournalFact`, and context-activation
production paths. Codex and Remote now expose Host-ready typed Complete Agent registrations.
Codex includes a production App Server JSON-RPC transport. Registration is an async ready
boundary: it sends the typed 0.144.1 `initialize` request with AgentDash client identity and
experimental API capability, validates the typed server response, then sends `initialized`.
Only that completed sequence exposes the service to Host registration. Its bounded notification
tail is live observation evidence, while `thread/read` and Codex ThreadStore remain source
authority.
Remote keeps the reviewed reverse callback, source change, generation fence, disconnect, deadline,
and effect-idempotency behavior over Runtime Wire revision 4. Relay remains a placement stream.

The current production composition still calls the driver-era Integration API. This component must
therefore enter S5 only together with the exact cross-owner cutover operations in `manifest.json`.
Applying only part of that manifest would create either an uncompilable route or dual registration;
neither is an allowed intermediate production state.

## Owned activation result

- `CodexCompleteAgentRegistration` yields
  `(AgentServiceInstanceId, Arc<dyn CompleteAgentService>)` for
  `CompleteAgentHost::register_service` only after the App Server ready handshake.
- `CodexProcessTransport` owns App Server process/RPC correlation and reports observation gaps
  instead of inventing durable change authority.
- `RemoteCompleteAgentRegistration` preserves the caller-provided service instance identity and
  binds a `RemoteCompleteAgentService` to a concrete Runtime Wire target and placement.
- Codex no longer compiles or exports `CodexRuntimeIntegration`, driver factories, journal mapping,
  hook-driver bridges, or context activation.
- Remote no longer compiles or exports the runtime driver factory/contribution, driver endpoint,
  HostPort journal/context brokers, or driver-era resolver.

## Atomic shared-hotspot inputs

`manifest.json` schema version 2 freezes every required S5 shared edit with its owner, exact
removed/added symbols, prerequisite, gate and build command:

| Input | Owner | Final result |
| --- | --- | --- |
| Workspace `Cargo.toml` / `Cargo.lock` | W8 | One final crate graph and one lock regeneration |
| Runtime Wire lib / generator | Platform + W8 | Revision 4 service/change/callback envelope only |
| Wire JSON Schema / generated TypeScript | W8 | Canonical revision 4 artifacts |
| Integration API | Platform + W8 | Complete Agent registration collection |
| First-party Codex | Product + W8 | Ready Codex registration input |
| Infrastructure composition | Platform + W8 | `CompleteAgentHost` registrations and durable bindings |
| API AppState / relay module / registry / placement | Product + W8 | Revision 4 Cloud placement |
| Enterprise Remote E2E | Product + W8 | Complete Agent callback/change/reconnect tracer |
| Local Runtime Wire handler | Platform + W8 | Complete Agent endpoint resolution |
| Relay Runtime Wire | Platform + W8 | Service API provenance and revision 4 transport |
| Generic context activation | Platform/Product + W8 | Complete Agent-native context lifecycle only |

The generic removal gate covers source, persistence, workers, migration consumers, generated
contracts and tests. S5 is complete only when `ContextActivation`, `context_activation`,
`ContextActivationDispatch`, `ContextActivationRecovery` and `DriverContextActivation` all have
zero matches across `Cargo.toml`, `crates`, `packages` and `schemas`.

## Verification

```powershell
cargo test -p agentdash-integration-codex
cargo test -p agentdash-integration-remote-runtime
cargo test -p agentdash-agent-runtime-wire
cargo test -p agentdash-relay runtime_wire
cargo test -p agentdash-agent-runtime-host --test complete_agent_target
cargo clippy -p agentdash-integration-codex -p agentdash-integration-remote-runtime --all-targets --no-deps -- -D warnings
rg -n "AgentRuntimeDriver|RuntimeJournalFact|ContextActivation|codex_runtime_contribution|remote_runtime_contribution" crates/agentdash-integration-codex crates/agentdash-integration-remote-runtime
git diff --check
```

The `rg` command must return no result.
