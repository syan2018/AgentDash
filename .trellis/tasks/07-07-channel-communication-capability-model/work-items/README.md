# ChannelService Work Items

Task: `.trellis/tasks/07-07-channel-communication-capability-model`

## Status Legend

`planned` -> `dispatched` -> `implementing` -> `checking` -> `ready_for_integration` -> `done`

`blocked` can be used from any state. When unblocked, return to `dispatched` or `checking`.

## Tracker

| ID | File | Status | Worker / Channel | Last Update |
| --- | --- | --- | --- | --- |
| WI-01 | `WI-01-domain-document-model.md` | planned | - | initialized |
| WI-02 | `WI-02-owner-document-mutation.md` | planned | - | initialized |
| WI-03 | `WI-03-lifecyclerun-registry-persistence.md` | planned | - | initialized |
| WI-04 | `WI-04-channel-owner-store-binding-resolver.md` | planned | - | initialized |
| WI-05 | `WI-05-channel-service-core.md` | planned | - | initialized |
| WI-06 | `WI-06-capability-channel-projection.md` | planned | - | initialized |
| WI-07 | `WI-07-mailbox-gate-materialization.md` | planned | - | initialized |
| WI-08 | `WI-08-runtime-wake-convergence.md` | planned | - | initialized |
| WI-09 | `WI-09-provider-neutral-im-contract.md` | planned | - | initialized |
| WI-10 | `WI-10-integration-static-cleanup.md` | planned | - | initialized |

## Dispatcher Notes

- Update this table before and after each worker run.
- Keep worker handles stable: `wi01-impl`, `wi01-check`, etc.
- Record channel name if it differs from `channel-service-dispatch`.
- Do not mark `done` until WI-10 full integration confirms the item still passes.
