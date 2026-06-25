# Checkpoint Wave 5A

Date: 2026-06-25

## Scope

Round 5A 固定了 crates-first physical split 的目标 Cargo graph，并把 RuntimeSession、VFS、AgentRun、Lifecycle 的实现文件从 `agentdash-application` 搬到独立 target crates。此 checkpoint 允许 target crate 编译红灯，原因是本轮价值在于让 Cargo graph 暴露剩余 wrong owner / forbidden edge，而不是继续在 umbrella crate 内隐藏引用关系。

## Physical Moves

- 新增 workspace crates：
  - `agentdash-application-runtime-session`
  - `agentdash-application-vfs`
  - `agentdash-application-agentrun`
  - `agentdash-application-lifecycle`
- `agentdash-application-runtime-session` now owns moved `session/**` substrate and its test persistence support file.
- `agentdash-application-vfs` now owns generic `vfs/**` core.
- `agentdash-application-agentrun` now owns moved `agent_run/**`.
- `agentdash-application-lifecycle` now owns moved `lifecycle/**` and Lifecycle-owned orchestration runtime launch/reducer pieces.
- `agentdash-application` is now the composition/facade owner for extracted crates and retained owner adapters such as session VFS owner providers.
- API/local/MCP imports no longer consume old `agentdash_application::{session,agent_run,lifecycle,vfs}` implementation paths.

## Worker Results

| Worker | Result |
| --- | --- |
| `cargo-manifest-owner` | Added target crate manifests and repair deps without adding forbidden peer implementation dependencies. `cargo metadata` and forbidden-edge metadata check passed. |
| `runtime-session-crate-split` | Moved RuntimeSession substrate. Remaining failures are old application-local imports and direct AgentRun/Lifecycle facts that must become ports/composition. |
| `vfs-crate-split` | Moved generic VFS core. Owner providers remained outside generic VFS where already split, but remaining `runtime`, `skill_asset`, `runtime_tools` imports still need source repair. |
| `agentrun-crate-split` | Moved AgentRun modules. Remaining failures are old `crate::session`, `crate::lifecycle`, application service, repository and VFS facade imports. |
| `lifecycle-crate-split` | Moved Lifecycle modules and orchestration runtime pieces. Remaining failures are old `crate::session`, `crate::agent_run`, workflow compiler, repository and VFS owner imports. |
| `application-facade-owner` | Removed local implementation module declarations and re-exported extracted crate surfaces for the current umbrella transition. |
| `api-wiring-owner` | Rewired API/local imports to extracted crates or retained application facade imports. External old moved-module import gates are clean. |
| `ports-gap-owner` | No new port contracts added. Existing ports remain pure and sufficient for currently visible compiler failures. |
| `dead-path-cleaner` | No safe standalone stale path/test deletion found. Remaining old aliases are application facade repair work, not dead-path cleanup. |

## Validation

Passed:

```powershell
cargo fmt --check
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-ports --message-format short
cargo check -p agentdash-application-runtime-gateway --message-format short
rg -n "agentdash_application::(session|agent_run|lifecycle|vfs)::" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
rg -n "agentdash_application_(agentrun|lifecycle|runtime_session|runtime_gateway|vfs)" crates/agentdash-application-ports -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(agentrun|lifecycle|runtime_session|runtime_gateway)" crates/agentdash-application-vfs -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(agentrun|lifecycle|runtime_gateway)" crates/agentdash-application-runtime-session -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(lifecycle|runtime_session)" crates/agentdash-application-agentrun -g '*.rs'
rg -n "agentdash_application::|agentdash_application_(agentrun|runtime_session)" crates/agentdash-application-lifecycle -g '*.rs'
```

Expected red:

```powershell
cargo check -p agentdash-application-vfs --message-format short
cargo check -p agentdash-application-runtime-session --message-format short
cargo check -p agentdash-application-agentrun --message-format short
cargo check -p agentdash-application-lifecycle --message-format short
```

## Blockers Assigned To Round 5B

| Owner | Current blocker | Repair rule |
| --- | --- | --- |
| `vfs-repair` | `agentdash-application-vfs` still imports `crate::runtime`, `crate::skill_asset`, `crate::runtime_tools`. | Move neutral types to domain/spi/ports or make them explicit VFS-owned helpers; keep owner-specific adapters outside generic VFS. |
| `runtime-session-repair` | RuntimeSession still imports `crate::runtime`, `crate::vfs`, `crate::context`, `crate::hooks`, `crate::backend_execution_placement` and some AgentRun facts. | Replace with ports/spi/domain/generic VFS imports; composition-only facts stay in application. |
| `agentrun-repair` | AgentRun still has old `crate::session`, `crate::lifecycle`, `crate::repository_set`, `crate::capability`, `crate::context` and VFS root export assumptions. | Use existing ports/facades, move concrete composition back to application, and depend only on generic VFS where allowed. |
| `lifecycle-repair` | Lifecycle still has old `crate::session`, `crate::agent_run`, `crate::vfs`, `crate::repository_set`, workflow compiler and owner-provider imports. | Use materialization/delivery/projection ports and keep compiler/application composition outside Lifecycle crate if it is not Lifecycle-owned runtime. |
| `application-facade-repair` | Umbrella aliases still bridge extracted crates for application internal adapters. | Replace internal alias consumers with explicit extracted crates or composition facades, then delete old-path re-export shells. |
| `api-repair` | API/local compile is blocked before API by application/extracted crate source failures. | After target crate source owners unblock, verify direct extracted-crate deps and facade entrypoints. |

## Next Dispatch

Round 5B should run source repair workers in parallel with disjoint ownership:

- `vfs-repair`
- `runtime-session-repair`
- `agentrun-repair`
- `lifecycle-repair`
- `application-facade-repair`
- `api-contract-check`

Check agents should be launched after the first repair wave to verify forbidden edges and stale aliases, not to demand broad workspace green too early.
