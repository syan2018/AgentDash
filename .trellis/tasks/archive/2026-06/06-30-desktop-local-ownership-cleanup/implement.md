# Implementation Plan

## Operating Rules

- Follow Trellis workflow and start this task before code edits.
- Every subagent prompt must start with
  `Active task: .trellis/tasks/06-30-desktop-local-ownership-cleanup`.
- Cleanup-first constraint: this review exists to converge architecture from first principles.
  Removing Tauri-local DTO/IO/claim implementation forks is more important than adding convenience
  API surface.
- Do not keep old Tauri profile/settings/claim helpers as compatibility paths.
- Implementation workers must not run broad Rust builds or full suites. Use scoped `rg`, format and
  targeted tests only.

## Research Split

1. Tauri desktop ownership map
   - Map DTOs, commands and helper functions in `crates/agentdash-local-tauri/src/main.rs`.
   - Identify which functions remain Tauri shell adapters and which must move.

2. `agentdash-local` owner/test pattern map
   - Map runtime paths, machine identity, runner config/claim tests and local module export style.
   - Identify a small test strategy that avoids broad compile.

3. Frontend/Tauri command payload map
   - Map TS callers for `profile_*`, `desktop_settings_*`, `runtime_start`.
   - Confirm whether Rust type moves can preserve command payload shape.

## Ordered Implementation

1. [x] Add `agentdash-local` desktop profile/settings modules and exports.
2. [x] Move profile DTO, normalization and file IO into `agentdash-local`.
3. [x] Move desktop settings DTO, defaults and file IO into `agentdash-local`.
4. [x] Move desktop access-token ensure payload/response, HTTP client and validation into
   `agentdash-local`.
5. [x] Add local API that returns `LocalRuntimeConfig` for desktop runtime start.
6. [x] Thin Tauri commands to delegate to local APIs; keep autostart OS adapter in Tauri.
7. [x] Remove old Tauri helper functions and DTO definitions.
8. [x] Add focused unit tests in `agentdash-local`.
9. [x] Update spec and run targeted checks.
10. [ ] Commit as one D11 slice.

## Implementation Notes

- `desktop_profile` and `desktop_settings` own local profile/settings DTOs, defaults,
  normalization and file IO.
- `desktop_claim` owns desktop access-token ensure payload/response, HTTP client, retry policy,
  response validation, HTTP status mapping and runtime config projection.
- `agentdash-local-tauri` delegates profile/settings/claim/start-config to `agentdash-local` and
  keeps autostart, tray/window lifecycle, Desktop API origin selection and Tauri error mapping.

## Validation Results

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-desktop-local-ownership-cleanup`
  passed.
- `git diff --check` passed.
- `cargo fmt --check --package agentdash-local --package agentdash-local-tauri` passed.
- `cargo test -p agentdash-local desktop_profile --lib` passed: 7 tests.
- `cargo test -p agentdash-local desktop_settings --lib` passed: 4 tests.
- `cargo test -p agentdash-local desktop_claim --lib` passed: 8 tests.
- `cargo check -p agentdash-local-tauri` passed.
- Static search for old Tauri profile/settings/claim owner helpers returned no matches.

## Suggested Subagent Split

- Research A: Tauri main ownership map and TS payload map.
- Research B: `agentdash-local` module/test pattern map.
- Implement A: profile/settings move.
- Implement B: claim/start-config move after profile/settings API shape lands.
- Check: verify Tauri main is shell-only and old paths are removed.

## Validation Commands

Adjust exact filters after implementation:

```powershell
python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-desktop-local-ownership-cleanup
git diff --check
cargo fmt --check --package agentdash-local --package agentdash-local-tauri
cargo test -p agentdash-local desktop_profile --lib
cargo test -p agentdash-local desktop_settings --lib
cargo test -p agentdash-local desktop_claim --lib
rg -n "struct LocalRuntimeProfile|struct DesktopAppSettings|post_local_runtime_claim|validate_claim_response|desktop_settings_write_internal|profile_path\\(" crates/agentdash-local-tauri/src/main.rs
```
