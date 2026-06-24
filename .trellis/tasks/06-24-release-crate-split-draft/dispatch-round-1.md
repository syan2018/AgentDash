# Dispatch Round 1

## Started

Branch: `codex/release-crate-split-refactor`

| Worker | Agent id | Work item | Ownership |
| --- | --- | --- | --- |
| `ports-impl` | `019efaae-7f16-76f0-9a3f-20b6019606de` | `work-items/01-ports-boundary-expansion.md` | `crates/agentdash-application-ports/**` |
| `gateway-impl` | `019efaae-c910-7643-9e15-9121f5bbb2f0` | `work-items/02-runtime-gateway-setup-boundary.md` | RuntimeGateway setup boundary |
| `surface-impl` | `019efaaf-071a-7e31-ae70-655f72ac1698` | `work-items/03-agentrun-surface-facade.md` | AgentRun current/resource/effective surface |
| `api-impl` | `019efaaf-43f6-7ad0-9e43-8392f9669058` | `work-items/06-api-consumer-facade-cleanup.md` | API facade consumers |
| `session-impl` | `019efaaf-7c7f-7151-bfea-ff73a1dc900b` | `work-items/04-runtime-session-substrate-boundary.md` | RuntimeSession substrate boundary |
| `lifecycle-impl` | `019efaaf-be83-7990-b6c8-6651593372ff` | `work-items/05-agentrun-lifecycle-boundary.md` | Lifecycle materialization/projection boundary |

## Shared Instructions

- Start prompt begins with `Active task: .trellis/tasks/06-24-release-crate-split-draft`.
- Prefer command-driven mechanical migration and minimal gates.
- Preserve parallel edits and report owner conflicts.
- Delete obsolete path/test pairs when they only preserve stale behavior.
- Handoff must include changed files, commands run, failing commands, unresolved imports, and next owner.

## Checkpoint Plan

After first-wave workers finish, dispatch:

- `check-boundary-ports`
- `check-import-graph`
- `check-dead-paths`
- `check-wave-readiness`
