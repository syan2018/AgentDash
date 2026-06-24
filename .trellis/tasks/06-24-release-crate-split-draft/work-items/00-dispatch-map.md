# Work Item 00: Dispatch Map

## Objective

把本任务作为单一主轴派发。该文件定义 worker lane、文件所有权、阶段提交和冲突处理，后续 subagent 直接引用具体 work item 文件。

## Branch

`codex/release-crate-split-refactor`

## Worker Lanes

| Lane | Workers | Primary ownership | Start condition |
| --- | --- | --- | --- |
| A | `ports-impl` | `crates/agentdash-application-ports/**` | immediately after task start |
| B | `gateway-impl`, `api-impl` | RuntimeGateway setup providers; API current-surface helpers/routes | after ports scaffold compiles or expected symbols exist |
| C | `surface-impl`, `vfs-impl` | AgentRun surface/resource facade; VFS resource boundary | after `agent_run_surface` DTO names are fixed |
| D | `session-impl`, `lifecycle-impl` | RuntimeSession substrate; AgentRun/Lifecycle materialization boundary | after ports scaffold; coordinate overlapping targets |
| E | `visibility-impl` | `lib.rs`, module `mod.rs`, public re-exports | after consumers move to facades |
| F | `runtime-crates-impl`, `control-crates-impl` | Cargo manifests and physical crate moves | after Wave 2 static gates trend clean |

## Dispatch Rules

- Each worker owns its work item file and reports progress there or in final handoff.
- Only one worker edits `agentdash-application-ports/src/lib.rs` at a time.
- Physical file moves into new crates happen after a checkpoint commit, so compile errors map to one extraction owner.
- A worker may leave compile errors if the error list is the expected handoff to another work item.
- Every handoff includes changed files, commands run, failing commands, unresolved imports, and next owner.

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

## Final Gate

```powershell
cargo metadata --no-deps --format-version 1
cargo check --workspace
```
