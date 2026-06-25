# Dispatch Round 5

## Start Condition

Round 5 starts after commit `d84a0859` recorded `checkpoint-wave-4.md` and `physical-dependency-contract.md`.

Round 5 is a crates-first physical split round. It intentionally stops waiting for all module imports to be clean inside the monolithic application crate. The goal is to make Cargo express the target physical dependency contract early, then let compiler errors reveal the remaining wrong edges.

## Contract

Primary contract file:

- `physical-dependency-contract.md`

All implement and check agents must classify work against that file before local compile status. A red compile result is acceptable when the failure is assigned to a target crate owner and forbidden edge. A green result that keeps a forbidden edge is not acceptable.

## Mode

- Create every remaining target crate in the same wave: `agentdash-application-vfs`, `agentdash-application-runtime-session`, `agentdash-application-agentrun`, `agentdash-application-lifecycle`.
- Move files first, then repair compile errors by owner.
- Compile may be red after the first integration checkpoint.
- Do not add compatibility modules for old application paths.
- Delete stale facade/test/path pairs that only preserve old chains.
- Use command-driven mechanical moves and import rewrites instead of hand-editing import churn.
- Main session coordinates and commits; implement/check work defaults to Trellis subagents.

## Shared Ownership Locks

| Shared surface | Sole owner | Rule |
| --- | --- | --- |
| Root `Cargo.toml`, new crate `Cargo.toml`, workspace membership | `cargo-manifest-owner` | Other workers report dependency needs; they do not edit manifests. |
| `crates/agentdash-application/src/lib.rs` and umbrella facade modules | `application-facade-owner` | Other workers do not preserve old module paths through compatibility shells. |
| `crates/agentdash-application-ports/**` | `ports-gap-owner` | Other workers report missing DTO/trait/error; they do not widen ports ad hoc. |
| API/local/MCP imports and bootstrap wiring | `api-wiring-owner` | Crate split workers do not edit route code except to report required wiring. |
| `physical-dependency-contract.md`, dispatch docs, checkpoint docs | main session | Workers report findings; main session updates task state. |

## Implement Wave A: Crates-First Move

Spawn these implement agents together after the manifest owner has created skeleton crates, or let the manifest owner create skeleton crates as the first worker and keep the rest queued in the same channel.

| Worker | Work item | Primary ownership | Required output |
| --- | --- | --- | --- |
| `cargo-manifest-owner` | `work-items/09-physical-crate-extraction-runtime.md`, `work-items/10-physical-crate-extraction-control-plane-vfs.md` | workspace `Cargo.toml`, new crate manifests, baseline `lib.rs` skeletons | Target crates exist, `cargo metadata` either passes or reports explicit dependency blocker. |
| `runtime-session-crate-split` | `work-items/04-runtime-session-substrate-boundary.md`, `work-items/09-physical-crate-extraction-runtime.md` | `crates/agentdash-application-runtime-session/**`, moved RuntimeSession substrate from `application/src/session/**` | RuntimeSession files live in the new crate; direct AgentRun/Lifecycle imports are listed as blockers or port needs. |
| `vfs-crate-split` | `work-items/07-vfs-resource-surface-boundary.md`, `work-items/10-physical-crate-extraction-control-plane-vfs.md` | `crates/agentdash-application-vfs/**`, moved generic `application/src/vfs/**` core | Generic VFS core lives in the new crate; owner providers stay outside. |
| `agentrun-crate-split` | `work-items/03-agentrun-surface-facade.md`, `work-items/10-physical-crate-extraction-control-plane-vfs.md` | `crates/agentdash-application-agentrun/**`, moved `application/src/agent_run/**` | AgentRun files live in the new crate; direct Lifecycle/RuntimeSession imports are listed as blockers or port needs. |
| `lifecycle-crate-split` | `work-items/05-agentrun-lifecycle-boundary.md`, `work-items/10-physical-crate-extraction-control-plane-vfs.md` | `crates/agentdash-application-lifecycle/**`, moved `application/src/lifecycle/**` and Lifecycle-owned orchestration runtime pieces | Lifecycle files live in the new crate; direct AgentRun/RuntimeSession imports are listed as blockers or port needs. |
| `application-facade-owner` | `work-items/08-public-visibility-cleanup.md` | application root facade, owner adapter placement, old module removal | `agentdash-application` becomes composition/facade crate, not business implementation owner. |
| `api-wiring-owner` | `work-items/06-api-consumer-facade-cleanup.md` | `agentdash-api`, `agentdash-local`, `agentdash-mcp` imports/bootstrap | API/local/MCP consume extracted crates or application facade deliberately. |
| `ports-gap-owner` | `work-items/01-ports-boundary-expansion.md` | `agentdash-application-ports/**` | Minimal DTO/trait/error gaps are added only when required by forbidden edge repair. |
| `dead-path-cleaner` | `work-items/08-public-visibility-cleanup.md` | stale re-exports, old tests, deleted module paths | Old-path compatibility shells and obsolete tests are removed. |

## Implement Wave B: Compiler-Driven Repair

Run after Wave A produces a fixed target crate graph, even if compile is red.

| Worker | Focus |
| --- | --- |
| `runtime-session-repair` | Remove forbidden `runtime-session -> agentrun/lifecycle/application` edges through ports or composition root wiring. |
| `vfs-repair` | Ensure `agentdash-application-vfs` has no owner provider or application umbrella dependency. |
| `agentrun-repair` | Remove forbidden `agentrun -> lifecycle/runtime-session/application` edges; move relation to ports or facade inputs. |
| `lifecycle-repair` | Remove forbidden `lifecycle -> agentrun/runtime-session/application` edges; keep workflow runtime with Lifecycle unless a compile blocker proves otherwise. |
| `api-repair` | Repair API/local/MCP imports and AppState composition after crate moves. |
| `ports-repair` | Add minimal missing port contracts surfaced by compiler errors. |
| `stale-test-repair` | Delete tests that only assert old module paths or stale direct implementation chains. |
| `import-graph-check` | Run static forbidden-edge gates and assign each hit to owner. |

## Implement Wave C: Integration

Run after target crate checks are near green or all remaining blockers are owner-assigned.

| Worker | Focus |
| --- | --- |
| `runtime-crates-check` | `runtime-gateway` and `runtime-session` Cargo dependencies and targeted checks. |
| `control-plane-crates-check` | AgentRun/Lifecycle dependencies, mutual edges and targeted checks. |
| `vfs-core-check` | VFS crate purity and owner exclusion. |
| `api-contract-check` | API/local/MCP composition and contract imports. |
| `dead-export-check` | application facade and old re-export cleanup. |
| `workspace-check-owner` | `cargo metadata`, target crate checks, final `cargo check --workspace` blocker summary. |

## Prompt Prefix

Every worker prompt starts with:

```text
Active task: .trellis/tasks/06-24-release-crate-split-draft
Branch: codex/release-crate-split-refactor
Round: 5 crates-first physical split
Contract: .trellis/tasks/06-24-release-crate-split-draft/physical-dependency-contract.md
Work item: <path>
```

## Worker Bias

Implement workers:

- Prefer bulk `Move-Item`, `rg`, scripted import rewrite and compiler-driven repair.
- Do not preserve old module paths through compatibility shells.
- Do not run broad tests; run `cargo metadata` or the smallest relevant `cargo check -p` if useful.
- Leave red compile errors if they are assigned to the next owner and caused by the intended physical split.
- Report changed files, commands run, failed commands, unresolved imports, forbidden edges and next owner.

Check workers:

- Check `physical-dependency-contract.md` first.
- Classify each issue as `delete`, `move`, `port`, `composition`, or `keep read-model`.
- Recommend deleting obsolete tests when they only preserve stale behavior.
- Do not require implement workers to make broad workspace tests green before the target graph is fixed.

## Checkpoint Criteria

Round 5 can checkpoint with red compile if all of these are true:

- All target crates exist in the workspace.
- `agentdash-application-runtime-gateway` remains extracted.
- Moved files are owned by their target crate or intentionally retained in application facade/owner adapter space.
- Every `cargo metadata` or `cargo check` failure is assigned to a crate owner and forbidden edge.
- Check agents have reviewed VFS core purity, RuntimeSession forbidden edges, AgentRun/Lifecycle mutual edges and API wiring.

## Round 5A Checkpoint

Checkpoint file:

- `checkpoint-wave-5a.md`

Current status on 2026-06-25:

- All target crates exist and are workspace members.
- `agentdash-application-runtime-gateway` and `agentdash-application-ports` still pass crate checks.
- `cargo fmt --check` and `cargo metadata --no-deps --format-version 1` pass.
- Static forbidden-edge gates listed below are clean.
- Target implementation crate checks are red by source/path ownership, not manifest cycles or forbidden Cargo dependencies.
- Round 5B should start with source repair owners: `vfs-repair`, `runtime-session-repair`, `agentrun-repair`, `lifecycle-repair`, `application-facade-repair`, plus `api-contract-check`.

## Round 5B Pass 1 Checkpoint

Checkpoint file:

- `checkpoint-wave-5b-pass1.md`

Current status on 2026-06-25:

- `agentdash-application-vfs` and `agentdash-application-runtime-session` are green at crate level.
- Lifecycle source repair narrowed remaining work to workflow compiler ownership.
- AgentRun source repair removed first-layer forbidden implementation imports but exposed the larger frame-construction composition split.
- Next wave should prioritize `lifecycle-workflow-compiler-port`, `agentrun-session-port-repair`, `agentrun-frame-composition-repair`, `agentrun-capability-context-repair`, and `stale-test-repair`.

## Round 5B Pass 2 Checkpoint

Checkpoint file:

- `checkpoint-wave-5b-pass2.md`

Current status on 2026-06-25:

- Target implementation crates are green at crate level.
- Forbidden physical edges are clean in static gates.
- Round 5B target crate repair is complete.
- Round 5B pass 3 should focus on application composition/facade and API/local/MCP integration, not target crate ownership.

## Initial Static Gates

```powershell
cargo metadata --no-deps --format-version 1
rg -n "agentdash_application::(session|agent_run|lifecycle|vfs)::" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
rg -n "agentdash_application_(agentrun|lifecycle|runtime_session|runtime_gateway|vfs)" crates/agentdash-application-ports -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(agentrun|lifecycle|runtime_session|runtime_gateway)" crates/agentdash-application-vfs -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(agentrun|lifecycle|runtime_gateway)" crates/agentdash-application-runtime-session -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(lifecycle|runtime_session)" crates/agentdash-application-agentrun -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(agentrun|runtime_session)" crates/agentdash-application-lifecycle -g '*.rs'
```
