# Release application crate split 执行计划

## Execution Mode

- 单一 Trellis 任务：`.trellis/tasks/06-24-release-crate-split-draft`
- 单一承载分支：`codex/release-crate-split-refactor`
- 工作项文件：`.trellis/tasks/06-24-release-crate-split-draft/work-items/*.md`
- 工作项保存在本任务目录；subagent 派发时直接引用工作项文件，保证所有 worker 共享同一边界图。
- 阶段提交以边界固定为检查点；提交说明写清已固定的边界、当前验证状态和下一步收敛点。
- implement agents 优先使用命令完成机械迁移：`rg` 定位、整目录 move、批量 import rewrite、`cargo metadata`、精确 `cargo check -p`、可控 `cargo fix`。避免逐行手工 import 修补。
- implement agents 只运行 work item 最小 gate；大测试、波次 readiness 和架构一致性判断交给 checkpoint check agents。
- 发现冗余路径、重复 facade、错误链路、旧命名兼容壳，或只被业务无关 test 锚定的陈旧行为时，按目标架构删除路径和对应 test，并在 handoff 说明删除理由。

## Main Checklist

### Wave 0: Mainline Setup

- [x] 复读 parent/child 任务文件、research、review briefs、spec 与 manifests。
- [x] 创建分支 `codex/release-crate-split-refactor`。
- [x] 绑定 Trellis task branch。
- [x] 启动本任务为 active task。
- [x] 创建 `work-items/*.md`。
- [x] 刷新 `implement.jsonl` / `check.jsonl`。
- [x] 运行 baseline commands 并记录结果。

Baseline result on 2026-06-25:

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-24-release-crate-split-draft` passed: `implement.jsonl` 28 entries, `check.jsonl` 12 entries.
- `cargo metadata --no-deps --format-version 1` passed.
- `rg -n "session_construction" crates` returned no source hits.
- `rg -n "use crate::(mcp_preset|workspace)::" crates/agentdash-application/src/runtime_gateway -g '*.rs'` found the expected setup blocker:
  - `crates/agentdash-application/src/runtime_gateway/setup_actions.rs:9`
  - `crates/agentdash-application/src/runtime_gateway/setup_actions.rs:10`

### Wave 1: Ports Only

- [x] Add `agentdash-application-ports::agent_run_surface`。
- [x] Add `agentdash-application-ports::runtime_session_delivery`。
- [x] Add `agentdash-application-ports::runtime_surface_adoption`。
- [x] Add `agentdash-application-ports::frame_launch_envelope`。
- [x] Add `agentdash-application-ports::lifecycle_surface_projection`。
- [x] Add `agentdash-application-ports::lifecycle_materialization`。
- [x] Add `agentdash-application-ports::agent_frame_materialization`。
- [x] Add `agentdash-application-ports::runtime_gateway_setup`。
- [x] Add `agentdash-application-ports::vfs_surface_runtime`。
- [x] Keep existing `runtime_gateway_mcp_surface` as reduced Gateway MCP DTO。

Gate:

```powershell
cargo check -p agentdash-application-ports
```

### Wave 2: Import Cleanup And Facade Contraction

- [x] RuntimeGateway setup actions consume `runtime_gateway_setup` ports.
- [x] API current-surface helper consumes AppState-owned query facade instead of reconstructing concrete query per call.
- [x] AgentRun resource/current surface DTOs move to stable facade/ports and preserve launch/current frame distinction.
- [x] Terminal/runtime placement uses application facade so route code only maps HTTP request/response.
- [ ] VFS preview/runtime source construction moves behind VFS/AgentRun facades.
- [ ] Lifecycle dispatch consumes RuntimeSession creation port.
- [ ] Lifecycle AgentCall materialization consumes AgentRun frame materialization/update port.
- [ ] AgentRun consumes Lifecycle projection ports instead of projector implementation imports.
- [ ] AgentRun consumes RuntimeSession delivery/adoption ports instead of session services.
- [ ] SessionHub/runtime builder consume launch/adoption/mailbox/effective-capability ports.
- [ ] Public exports contract to intended facades.

Gate:

```powershell
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local -p agentdash-mcp
```

Checkpoint result on 2026-06-25:

- Compile/test gates passed for ports, application, API, local/MCP, RuntimeGateway setup targeted tests, `cargo fmt --check`, `git diff --check`, and task manifest validation.
- Ports purity check passed with no `AppState`, `RepositorySet`, route DTO, builder or concrete adapter leakage.
- RuntimeGateway-only extraction may proceed because MCP/setup dependencies are port-mediated and the old setup import gate is clean.
- RuntimeSession extraction is still blocked by direct AgentRun/Lifecycle imports in launch/adoption/mailbox/effective-capability paths.
- AgentRun/Lifecycle extraction is still blocked by lifecycle projector/current-frame resolver imports and by Lifecycle using AgentRun frame materialization helper instead of a port.
- VFS extraction is still blocked by API VFS route-local assembly and owner-specific provider dependencies.
- Full checkpoint details live in `checkpoint-wave-1.md`.

### Wave 3: Runtime Crate Extraction

- [x] Add workspace crate `agentdash-application-runtime-gateway`.
- [x] Move RuntimeGateway modules and rewire dependencies.
- [ ] Add workspace crate `agentdash-application-runtime-session`.
- [ ] Move RuntimeSession substrate modules and rewire dependencies.
- [x] Rewire API/local/MCP composition roots for RuntimeGateway.

Checkpoint result on 2026-06-25:

- `agentdash-application-runtime-gateway` is a clean extracted crate and does not depend on monolithic `agentdash-application`.
- API/local/MCP no longer import `agentdash_application::runtime_gateway`.
- RuntimeSession extraction is explicitly deferred because production adoption, launch envelope, accepted launch commit, mailbox/effective capability and hook target resolution still need ports.
- Full checkpoint details live in `checkpoint-wave-2.md`.

Gate:

```powershell
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-runtime-gateway
cargo check -p agentdash-application-runtime-session
cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp
```

### Wave 4: Control Plane Extraction

- [ ] Add workspace crate `agentdash-application-agentrun`.
- [ ] Move AgentRun modules and keep frame internals private to crate.
- [ ] Add workspace crate `agentdash-application-lifecycle`.
- [ ] Move Lifecycle + orchestration runtime/reducer/materialization modules.
- [ ] Rewire umbrella `agentdash-application` according to remaining consumer needs.

Gate:

```powershell
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-agentrun
cargo check -p agentdash-application-lifecycle
cargo check --workspace
```

### Wave 5: VFS Core Extraction

- [ ] Add workspace crate `agentdash-application-vfs` if owner-specific provider deps are directional.
- [ ] Move generic VFS core and fs/mount/shell tools.
- [ ] Keep owner providers with owners/adapters where dependencies require it.
- [ ] Rewire API and AgentRun resource surface to stable VFS facades.

Gate:

```powershell
cargo check -p agentdash-application-vfs
cargo check --workspace
```

## Subagent Dispatch Plan

Use one Trellis channel for the active task. Spawn at most six live implement workers at a time. Check agents run at wave checkpoints and do not ask implement agents to preserve old behavior for compatibility.

Round 1 after Wave 0:

| Worker | File | Edit ownership |
| --- | --- | --- |
| `ports-impl` | `work-items/01-ports-boundary-expansion.md` | `crates/agentdash-application-ports/**`, dependent import compile fixes only when required. |
| `gateway-impl` | `work-items/02-runtime-gateway-setup-boundary.md` | `crates/agentdash-application/src/runtime_gateway/**`, setup call adapters, tests. |
| `api-impl` | `work-items/06-api-consumer-facade-cleanup.md` | `crates/agentdash-api/src/agent_run_runtime_surface.rs`, selected route helpers, AppState service handles. |
| `surface-impl` | `work-items/03-agentrun-surface-facade.md` | `crates/agentdash-application/src/agent_run/**` current/resource surface and runtime placement. |
| `session-impl` | `work-items/04-runtime-session-substrate-boundary.md` | `crates/agentdash-application/src/session/**` and RuntimeSession port implementations. |
| `lifecycle-impl` | `work-items/05-agentrun-lifecycle-boundary.md` | `crates/agentdash-application/src/lifecycle/**`, `workflow/orchestration/**`, materialization ports. |

Round 1 checkpoint checks:

| Worker | File / focus | Check ownership |
| --- | --- | --- |
| `check-boundary-ports` | `work-items/01-ports-boundary-expansion.md` | Verify ports purity and flag concrete service / AppState / route DTO leakage. |
| `check-import-graph` | `design.md` static gates | Run rg gates and assign remaining direct imports to owners. |
| `check-dead-paths` | research + current diff | Find obsolete helpers, duplicate facades, stale compatibility paths and tests that should be deleted. |
| `check-wave-readiness` | `implement.md` Wave 2/Wave 3 gates | Decide whether runtime crate extraction can start or which owner blocks it. |

Round 2 after first checkpoint commit:

| Worker | File | Edit ownership |
| --- | --- | --- |
| `vfs-impl` | `work-items/07-vfs-resource-surface-boundary.md` | `crates/agentdash-application/src/vfs/**`, VFS route facade consumers. |
| `visibility-impl` | `work-items/08-public-visibility-cleanup.md` | module `mod.rs` / `lib.rs` facade contraction after imports move. |
| `runtime-crates-impl` | `work-items/09-physical-crate-extraction-runtime.md` | Cargo manifests and RuntimeGateway/RuntimeSession crate moves. |
| `control-crates-impl` | `work-items/10-physical-crate-extraction-control-plane-vfs.md` | AgentRun/Lifecycle/VFS crate moves after runtime crates settle. |

Round 2 checkpoint checks:

| Worker | Focus | Check ownership |
| --- | --- | --- |
| `check-runtime-crates` | RuntimeGateway / RuntimeSession crates | Ensure extracted crates do not depend on monolithic application or owner implementations. |
| `check-control-plane-crates` | AgentRun / Lifecycle crates | Ensure mutual links are ports/facades, not implementation imports. |
| `check-vfs-core` | generic VFS crate | Ensure generic VFS core is free of session/lifecycle/canvas owner internals. |
| `check-final-contract` | final gates | Run cargo metadata, static gates, target crate checks and summarize workspace blockers. |

Round 2 actual checkpoint:

| Worker | Agent id | Result |
| --- | --- | --- |
| `check-runtime-gateway-crate` | `019efafd-3959-7b32-9da1-fc9d9e860c28` | Gateway extraction passed; only temporary umbrella re-export remains. |
| `check-session-port-wiring` | `019efafd-4db5-7971-9453-125757ec0ba8` | Session/Lifecycle direct import gate passed; RuntimeSession extraction still blocked by production port wiring. |
| `check-control-plane-port-wiring` | `019efafd-6277-7b53-9d58-d4633c50658c` | AgentFrameBuilder/current-frame resolver gates passed; AgentRun/Lifecycle extraction still blocked by remaining dispatch/helper couplings. |
| `check-api-vfs-facade` | `019efafd-76c5-75a1-b79f-b43289097343` | API VFS facade cleanup passed; VFS physical extraction still blocked by owner-specific providers. |

Round 3 planned dispatch:

- Dispatch file: `dispatch-round-3.md`
- Mode: port-wiring convergence only; no RuntimeSession, AgentRun, Lifecycle or VFS physical crate moves.
- Implement lanes: session adoption port, session launch/commit port, control dispatch facade, frame construction helper port, VFS owner-adapter prep.
- Check lanes: session adoption, session launch/commit, control dispatch boundary, VFS owner adapters, Gateway regression.

Round 3 checkpoint result on 2026-06-25:

- Runtime surface adoption passed and stale `AgentRunActiveRuntimeSurfaceAdopter` was deleted.
- Old Session launch/commit adapter names are gone from Session/API bootstrap.
- AgentRun/workflow no longer construct `LifecycleDispatchService` directly.
- AgentRun frame construction no longer imports Lifecycle helper implementation paths.
- Generic VFS registry no longer owns Session/Lifecycle/Canvas provider registration.
- RuntimeSession extraction remains blocked by concrete AgentRun `FrameLaunchEnvelope`, mailbox/effective-capability/surface helper imports.
- VFS physical extraction remains blocked by owner-specific providers and application-level `VfsSurfaceResolver`.
- Full checkpoint details live in `checkpoint-wave-3.md`.

Round 4 planned dispatch:

- Dispatch file: `dispatch-round-4.md`
- Mode: substrate convergence only; no RuntimeSession, AgentRun, Lifecycle or VFS physical crate moves.
- Implement lanes: neutral launch envelope, mailbox/effective-capability ports, Gateway visibility cleanup, VFS owner adapter split.
- Check lanes: RuntimeSession envelope, RuntimeSession live ports, Gateway visibility, VFS owner split, Round 4 readiness.

Each worker prompt starts with:

```text
Active task: .trellis/tasks/06-24-release-crate-split-draft
Branch: codex/release-crate-split-refactor
Work item: <path>
```

Worker handoff must include changed files, commands run, failing commands, unresolved imports and suggested next owner.

Implement worker prompt bias:

- State file ownership and conflict boundaries explicitly.
- Prefer mechanical moves/replacements and command-driven fixes over hand-edited import churn.
- Run only minimal gates listed in the work item; leave broad tests to check agents.
- Delete obsolete path/test pairs when the old behavior contradicts the target architecture, and report the deletion.
- Do not revert parallel worker edits; adapt to them or report owner conflict.

Check worker prompt bias:

- Prioritize boundary violations, stale paths, duplicate facades, incorrect chains and obsolete tests.
- Classify each finding as delete, move, port, or keep as presentation/debug read-model.
- Assign each finding to a work item owner.
- Treat tests as evidence only when they encode target architecture; recommend deleting tests that only preserve stale behavior.
- Keep output ordered by severity and wave readiness impact.

## Batch Strategy

- Use `rg` to list old import paths before editing.
- Move DTO/traits first, then update imports with mechanical replacements.
- Prefer one module owner per batch; dependent modules adapt through ports.
- For file moves into new crates, first move whole module directories and set `lib.rs` exports, then let compiler enumerate missing imports.
- After each wave, run `cargo metadata`; it is cheaper than full check and catches manifest cycles early.
- Preserve user/parallel session changes while editing scoped files.

## Validation Commands

Baseline:

```powershell
cargo metadata --no-deps --format-version 1
rg -n "session_construction" crates
rg -n "use crate::(mcp_preset|workspace)::" crates/agentdash-application/src/runtime_gateway -g '*.rs'
```

Static boundary:

```powershell
rg -n "crate::session::(plan|runtime_commands|types|hub|Session.*Service|LaunchCommand)" crates/agentdash-application/src/agent_run -g '*.rs'
rg -n "AgentFrameBuilder" crates/agentdash-application/src/lifecycle crates/agentdash-application/src/workflow/orchestration -g '*.rs'
rg -n "crate::lifecycle::.*AgentRunRuntimeAddress|crate::lifecycle::surface::surface_projector|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src/agent_run crates/agentdash-application/src/session -g '*.rs'
rg -n "AgentRunRuntimeSurfaceQuery::new|AgentRunRuntimeSurfaceQueryDeps|runtime_surface_query\\(" crates/agentdash-api/src -g '*.rs'
rg -n "agentdash_application::session::(construction|plan|types|hub)|agentdash_application::agent_run::frame|agentdash_application::vfs::ResolvedVfsSurfaceSource|agentdash_application::vfs::build_surface_summary" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
```

Compile/test:

```powershell
cargo check -p agentdash-application-ports
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local -p agentdash-mcp
cargo test -p agentdash-application runtime_gateway::mcp_access
cargo test -p agentdash-application agent_run::runtime_surface
cargo test -p agentdash-api agent_run_runtime_surface
cargo check --workspace
```

## Commit Plan

Suggested checkpoint commits:

1. `docs(crate-split): 建立 application crate split 主轴`
2. `refactor(ports): 扩展 application 边界端口`
3. `refactor(runtime-gateway): 收束 setup action 端口边界`
4. `refactor(agentrun): 固化 runtime surface facade`
5. `refactor(runtime-session): 倒置 RuntimeSession substrate 依赖`
6. `refactor(lifecycle): 收束 AgentRun/Lifecycle 物化边界`
7. `refactor(api): 收束 current surface 消费入口`
8. `refactor(crates): 抽取 RuntimeGateway 与 RuntimeSession crate`
9. `refactor(crates): 抽取 AgentRun 与 Lifecycle crate`
10. `refactor(vfs): 抽取 VFS core 边界`
