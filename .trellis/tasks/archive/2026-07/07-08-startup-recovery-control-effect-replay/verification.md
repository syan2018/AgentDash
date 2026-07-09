# Verification Notes

## 2026-07-08

### Actual Startup Smoke

Command:

```powershell
node ./scripts/dev-runtime.js --profile web --skip-local --skip-frontend --server-port 3011
```

Result:

- First run after the initial phased replay implementation still reproduced `thread 'main' has overflowed its stack` during `agentdash-server serve`.
- Direct `agentdash-server serve` with trace logging showed the remaining risky path: terminal side-effect replay delivered a companion parent mailbox wake to RuntimeSession `91533d57-999a-443a-8e4b-f68bedf9e652`, whose execution state was already `Interrupted`, and the mailbox scheduler attempted `LaunchOrContinueTurn`.
- Companion parent mailbox wake was tightened so terminal target delivery returns `skipped_terminal_target` without creating a mailbox message.
- Re-running the same dev-runtime smoke then succeeded:
  - migrate completed at schema version 60
  - `agentdash-server :3011/api/health -> 200`
  - 20 second post-ready observation window produced no stack overflow
  - dev-runtime stopped cleanly and killed embedded PostgreSQL

### Automated Checks

- `cargo fmt`
- `git diff --check`
- `python ./.trellis/scripts/task.py validate .trellis/tasks/07-08-startup-recovery-control-effect-replay`
- `cargo check -p agentdash-api`
- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-application companion_mailbox_guard_skips_terminal_target`
- `cargo test -p agentdash-application-agentrun agent_run::mailbox`
- `cargo test -p agentdash-application-agentrun agent_run::control_effects`

Additional checks run by the Trellis check agent before the final companion guard tightening:

- `cargo test -p agentdash-application-runtime-session transcript_restore`
- `cargo test -p agentdash-application-runtime-session append_guard`
- `cargo test -p agentdash-application-runtime-session process_turn_terminal`
- `cargo test -p agentdash-application-runtime-session control_effect_outbox_tracks_attempt_status_and_delete`
- `cargo test -p agentdash-application-runtime-session control_effect_outbox_claim_filters_effect_kind`
- `cargo test -p agentdash-application-workflow gate_wait_policy`
- `cargo test -p agentdash-api`

### Known Non-Task Lint Debt

`cargo clippy ... -- -D warnings` is still blocked by pre-existing lint debt outside this task's behavior boundary:

- `agentdash-agent-protocol/src/backbone/platform.rs`: `large_enum_variant`
- `agentdash-domain/src/channel/mod.rs`: `collapsible_if`
- `agentdash-domain/src/workflow/dispatch.rs`: `large_enum_variant`
- selected runtime-session functions trip existing `too_many_arguments`

No migration was added, so `pnpm run migration:guard` was not required.
