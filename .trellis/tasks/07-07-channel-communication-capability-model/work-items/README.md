# ChannelService Work Items

Task: `.trellis/tasks/07-07-channel-communication-capability-model`

## Status Legend

`planned` -> `dispatched` -> `implementing` -> `checking` -> `ready_for_integration` -> `done`

`blocked` can be used from any state. When unblocked, return to `dispatched` or `checking`.

## Tracker

| ID | File | Status | Native Agent | Last Update |
| --- | --- | --- | --- | --- |
| WI-01 | `WI-01-domain-document-model.md` | done | native check `Ohm` | dispatcher integration review passed |
| WI-02 | `WI-02-owner-document-mutation.md` | done | native check `Ohm` | dispatcher integration review passed |
| WI-03 | `WI-03-lifecyclerun-registry-persistence.md` | done | native check `Ohm` | dispatcher integration review passed |
| WI-04 | `WI-04-channel-owner-store-binding-resolver.md` | done | native check `Ohm` | dispatcher integration review passed |
| WI-05 | `WI-05-channel-service-core.md` | done | native check `Ohm` | semantic update/remove APIs added; dispatcher review passed |
| WI-06 | `WI-06-capability-channel-projection.md` | done | native check `Ohm` | dispatcher integration review passed |
| WI-07 | `WI-07-mailbox-gate-materialization.md` | done | native check `Ohm` | mapper ownership cleaned up; dispatcher review passed |
| WI-08 | `WI-08-runtime-wake-convergence.md` | done | native check `Ohm` | dispatcher integration review passed |
| WI-09 | `WI-09-provider-neutral-im-contract.md` | done | native check `Ohm` | dispatcher integration review passed |
| WI-10 | `WI-10-integration-static-cleanup.md` | done | native check `Ohm` | full-scope check and dispatcher review passed |

## Current Dispatch State

- Main session is dispatcher/host only from this point forward.
- Native spawned check worker `Ohm` (`019f3db3-ae9a-7440-8549-9b66e4ceff10`) completed the full-scope review/fix pass.
- WI-01 through WI-10 passed dispatcher integration review.
- If dispatcher finds a new gap during integration review, spawn native `trellis-implement` workers for the affected WI with disjoint file ownership, then re-run a check worker.
- Full WI-10 checks and dispatcher integration review passed; checkpoint commits are next.

## Dispatcher Notes

- Update this table before and after each worker run.
- Record the native agent nickname and agent id for every spawned worker.
- Use stable work-item labels in prompts: `WI-01 implement`, `WI-01 check`, etc.
- Record any external audit surface only when it is actually used for this run.
- Move an item to `done` after WI-10 full integration confirms the item still passes.
