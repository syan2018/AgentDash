# Implement Plan

## Step 1 - Context And Contracts

- Read current runner CLI, config, claim, service, status modules.
- Update cross-layer `desktop-local-runtime.md` with setup helper contract once design is accepted.
- Apply embedded default policy:
  - generic runner release artifact has no concrete server URL embedded
  - cloud/customer/environment-specific artifact may embed `AGENTDASH_RUNNER_DEFAULT_SERVER_URL`
  - embedded defaults contain only non-secret packaging hints

Validation:

```powershell
rg "enum Command|RunnerCliOverrides|persist_credentials|ServiceAction|RunnerStatusSnapshot" crates/agentdash-local/src -n
```

## Step 2 - Embedded Defaults

- Add `RunnerEmbeddedDefaults` to `runner_config`.
- Read build-time values through `option_env!`.
- Extend config resolution source tracking with `embedded`.
- Add tests for precedence:
  - CLI > env > file > embedded > default.
  - no secret-bearing embedded fields exist.

Validation:

```powershell
cargo test -p agentdash-local runner_config -- --nocapture
```

## Step 3 - Setup CLI Surface

- Add `Command::Setup(SetupArgs)`.
- Reuse/extend `ConfigArgs` for setup inputs.
- Add flags:
  - `--install-service`
  - `--start`
  - `--dry-run`
  - `--json`
  - `--non-interactive`
- Add parser tests.

Validation:

```powershell
cargo test -p agentdash-local cli_shape -- --nocapture
```

## Step 4 - Interactive Input Planner

- Add a pure setup planner that determines missing fields and prompt order.
- Add interactive prompt implementation around stdin/stdout.
- Keep token values out of logs and summaries.
- In non-interactive mode, return actionable missing-field errors.

Validation:

```powershell
cargo test -p agentdash-local setup_prompt -- --nocapture
```

## Step 5 - Config Write And Claim Orchestration

- Add config write helper for non-credential runner fields and registration token.
- Run claim when credentials are missing.
- Persist credentials using existing `persist_credentials`.
- Produce setup summary from resolved config + claim result.
- Ensure `--dry-run` does not write config or call claim.

Validation:

```powershell
cargo test -p agentdash-local setup_config setup_claim -- --nocapture
```

## Step 6 - Service Install/Start Orchestration

- When `--install-service`, call existing service install path.
- When `--start`, call existing service start path after install or against existing service.
- Merge service result into setup summary.
- Keep service execution injectable for tests.

Validation:

```powershell
cargo test -p agentdash-local setup_service runner_service -- --nocapture
```

## Step 7 - Doctor Command

- Add `Command::Doctor(DoctorArgs)`.
- Check:
  - config readable and parseable
  - server URL present
  - credentials present / registration token present
  - service installed/running
  - status snapshot exists and freshness
  - log path parent exists/writable
  - optional server `/api/health` reachable
- Support human and JSON output.

Validation:

```powershell
cargo test -p agentdash-local doctor -- --nocapture
```

## Step 8 - Docs And Release Checklist

- Update `.trellis/spec/cross-layer/desktop-local-runtime.md`.
- Update `.trellis/tasks/06-26-distribution-release-validation/implement.md` runner acceptance checklist.
- Add copy-paste Linux and Windows setup examples.
- Record build-time defaults example for customer/environment-specific runner artifacts.

Validation:

```powershell
rg "agentdash-local setup|agentdash-local doctor" .trellis docs -n
```

## Step 9 - Final Verification

Run focused checks:

```powershell
cargo fmt --check
cargo test -p agentdash-local -- --nocapture
cargo check -p agentdash-local
git diff --check
```

Manual acceptance to hand off:

- Linux VM:
  - run setup with registration token
  - verify systemd service installed/running
  - verify cloud runner online
- Windows admin PowerShell:
  - run setup with registration token
  - verify SCM service installed/running
  - verify cloud runner online

## Rollback Points

- If interactive prompt introduces cross-platform terminal issues, keep non-interactive `setup` and gate prompt behind follow-up.
- If embedded defaults create packaging ambiguity, ship `setup` with CLI/env/config only and leave embedded defaults behind a build-time feature.
- If service start verification is flaky, complete setup after service start command succeeds and leave online verification to `doctor`.

## Ready-To-Start Checklist

- [x] PRD has testable setup, doctor, dry-run, JSON and embedded-default acceptance criteria.
- [x] Design resolves embedded-default packaging policy.
- [x] Implement plan is split into independently verifiable runner steps.
- [x] `implement.jsonl` and `check.jsonl` point to real specs/research artifacts.
- [x] User reviews planning artifacts and approves `task.py start`.
