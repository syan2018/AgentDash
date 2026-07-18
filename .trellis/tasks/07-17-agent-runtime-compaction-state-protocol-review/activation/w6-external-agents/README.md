# W6 Codex / Remote Complete Agent activation component

## Identity

- Frozen base: `fc26d3ff`
- Runtime Wire target revision: `4`
- Ownership: Codex adapter, Remote Complete Agent proxy/endpoint, adapter-facing Wire usage
- Production composition owner: W8 hard cut

This component removes the adapter-owned driver, `RuntimeJournalFact`, and context-activation
production paths. Codex and Remote now expose Host-ready typed Complete Agent registrations.
Codex includes a production App Server JSON-RPC transport; its bounded notification tail is only
live observation evidence, while `thread/read` and Codex ThreadStore remain source authority.
Remote keeps the reviewed reverse callback, source change, generation fence, disconnect, deadline,
and effect-idempotency behavior over Runtime Wire revision 4. Relay remains a placement stream.

The current production composition still calls the driver-era Integration API. This component must
therefore enter S5 only together with the exact cross-owner cutover operations in `manifest.json`.
Applying only part of that manifest would create either an uncompilable route or dual registration;
neither is an allowed intermediate production state.

## Owned activation result

- `CodexCompleteAgentRegistration` yields
  `(AgentServiceInstanceId, Arc<dyn CompleteAgentService>)` for
  `CompleteAgentHost::register_service`.
- `CodexProcessTransport` owns App Server process/RPC correlation and reports observation gaps
  instead of inventing durable change authority.
- `RemoteCompleteAgentRegistration` preserves the caller-provided service instance identity and
  binds a `RemoteCompleteAgentService` to a concrete Runtime Wire target and placement.
- Codex no longer compiles or exports `CodexRuntimeIntegration`, driver factories, journal mapping,
  hook-driver bridges, or context activation.
- Remote no longer compiles or exports the runtime driver factory/contribution, driver endpoint,
  HostPort journal/context brokers, or driver-era resolver.

## Verification

```powershell
cargo test -p agentdash-integration-codex
cargo test -p agentdash-integration-remote-runtime
cargo clippy -p agentdash-integration-codex -p agentdash-integration-remote-runtime --all-targets -- -D warnings
rg -n "AgentRuntimeDriver|RuntimeJournalFact|ContextActivation|codex_runtime_contribution|remote_runtime_contribution" crates/agentdash-integration-codex crates/agentdash-integration-remote-runtime
git diff --check
```

The `rg` command must return no result.
