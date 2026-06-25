# Work Item 00: Dispatch Map

## Objective

把本任务作为单一主轴派发。该文件定义 worker lane、文件所有权、阶段提交和冲突处理，后续 subagent 直接引用具体 work item 文件。

## Branch

`codex/release-crate-split-refactor`

## Worker Lanes

| Lane | Workers | Primary ownership | Start condition |
| --- | --- | --- | --- |
| Manifest | `cargo-manifest-owner` | workspace manifest, target crate manifests, crate skeleton `lib.rs` | Round 5 start |
| RuntimeSession | `runtime-session-crate-split`, `runtime-session-repair` | `agentdash-application-runtime-session/**`, moved session substrate | after crate skeleton exists |
| VFS | `vfs-crate-split`, `vfs-repair` | `agentdash-application-vfs/**`, moved generic VFS core | after crate skeleton exists |
| AgentRun | `agentrun-crate-split`, `agentrun-repair` | `agentdash-application-agentrun/**`, moved AgentRun modules | after crate skeleton exists |
| Lifecycle | `lifecycle-crate-split`, `lifecycle-repair` | `agentdash-application-lifecycle/**`, moved Lifecycle modules and Lifecycle-owned orchestration runtime | after crate skeleton exists |
| Facade/API | `application-facade-owner`, `api-wiring-owner`, `api-repair` | `agentdash-application` facade, API/local/MCP imports/bootstrap | after first physical moves |
| Ports/cleanup | `ports-gap-owner`, `ports-repair`, `dead-path-cleaner`, `stale-test-repair` | ports DTO/trait/error gaps, stale path/test deletion | during repair wave |
| Check | `import-graph-check`, `runtime-crates-check`, `control-plane-crates-check`, `vfs-core-check`, `api-contract-check`, `dead-export-check`, `workspace-check-owner` | contract gates and owner-assigned blockers | checkpoint waves |

## Dispatch Rules

- Each worker owns its work item file and reports progress there or in final handoff.
- Shared files are locked to single owners: manifests, application facade, ports and API/local/MCP wiring.
- Physical file moves into new crates happen immediately in Round 5A; compile errors map to target crate owners.
- A worker may leave compile errors if the error list is the expected handoff to another work item.
- Every handoff includes changed files, commands run, failing commands, unresolved imports, and next owner.
- Implement workers use command-driven mechanical migration first: `rg`, batch move, batch import rewrite, `cargo metadata`, precise `cargo check -p`, controlled `cargo fix`.
- Implement workers run minimal gates for their work item; broad tests are delegated to checkpoint check agents.
- If a stale path or test exists only to preserve a legacy chain that conflicts with the target graph, delete the path and the test together and explain why.
- Workers are not alone in the codebase; they must preserve unrelated edits and report owner conflicts instead of reverting.
- Do not create compatibility modules to preserve old `agentdash_application::{session,agent_run,lifecycle,vfs}` paths after physical moves.
- A red checkpoint is acceptable when every failure is assigned to a crate owner and forbidden edge from `physical-dependency-contract.md`.

## Checkpoint Checks

After first-wave implementation, dispatch check agents before moving to physical crate extraction:

| Check worker | Focus |
| --- | --- |
| `check-boundary-ports` | ports purity: DTO/trait/error only; no `AppState`, `RepositorySet`, route DTO, builder, concrete adapter. |
| `check-import-graph` | static rg gates and remaining implementation import owners. |
| `check-dead-paths` | stale helpers, duplicate facades, legacy compatibility paths, obsolete tests. |
| `check-wave-readiness` | readiness for RuntimeGateway/RuntimeSession extraction. |

Round 5 checkpoint checks:

| Check worker | Focus |
| --- | --- |
| `import-graph-check` | static forbidden-edge gates from `physical-dependency-contract.md`. |
| `runtime-crates-check` | RuntimeGateway/RuntimeSession crates free of monolithic application and owner implementation deps. |
| `control-plane-crates-check` | AgentRun/Lifecycle mutual dependencies are ports/facades or owner-assigned blockers. |
| `vfs-core-check` | generic VFS core free of session/lifecycle/canvas owner internals. |
| `api-contract-check` | API/local/MCP imports and composition root wiring. |
| `dead-export-check` | stale application facade exports and obsolete tests. |
| `workspace-check-owner` | final cargo metadata, target crate checks, workspace blockers. |

## Checkpoint Commit Pattern

Use project commit style:

```text
type(scope): 可保留英文专业用词的中文提交信息

- 更新点一
- 更新点二
- 当前验证状态或红灯原因
```

## Baseline Commands

```powershell
cargo metadata --no-deps --format-version 1
rg -n "session_construction" crates
rg -n "use crate::(mcp_preset|workspace)::" crates/agentdash-application/src/runtime_gateway -g '*.rs'
```

## Round 5 Contract Commands

```powershell
cargo metadata --no-deps --format-version 1
rg -n "agentdash_application::(session|agent_run|lifecycle|vfs)::" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
rg -n "agentdash_application_(agentrun|lifecycle|runtime_session|runtime_gateway|vfs)" crates/agentdash-application-ports -g '*.rs'
```

## Final Gate

```powershell
cargo metadata --no-deps --format-version 1
cargo check --workspace
```
