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

## Completed

All six implement workers completed and were closed after handoff. Main-session integration fixed the test-only `WorkflowAgentNodeFrameMaterializationContext` import and removed the API unused import warning.

Validation passed:

- `cargo metadata --no-deps --format-version 1`
- `cargo check -p agentdash-application-ports`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-local -p agentdash-mcp`
- `cargo test -p agentdash-application runtime_gateway::setup_actions`
- `cargo fmt --check`
- `git diff --check`
- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-24-release-crate-split-draft`

## Checkpoint Results

| Worker | Agent id | Result |
| --- | --- | --- |
| `check-boundary-ports` | `019efaca-c269-7612-a546-c45ecfaf4c62` | Ports crate is pure DTO / trait / error and does not block the next wave. |
| `check-import-graph` | `019efaca-d6d0-7562-a14b-3bf94228d735` | Gateway setup and Lifecycle `AgentFrameBuilder` gates are clean; AgentRun/Session/Lifecycle and API/VFS imports still block broader extraction. |
| `check-dead-paths` | `019efaca-ebbb-71d1-a04a-b16c749c42b9` | Stale SessionConstruction test fixture paths and unanchored RuntimeSession fallback tests should be deleted during RuntimeSession cleanup. |
| `check-wave-readiness` | `019efacb-00a9-7253-bf96-5a9b7c3e9c87` | RuntimeGateway-only physical extraction is allowed; RuntimeSession, AgentRun/Lifecycle and VFS extraction are not ready. |

Full checkpoint notes: `checkpoint-wave-1.md`.
