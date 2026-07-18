# Agent Runtime 最终收敛实施状态

> 本文件只记录实施协调与可验证进度，不替代 `prd.md`、`design.md`、
> `transition-architecture.md` 或 `implement.md` 中的架构事实。

## Global objective

完整落实本任务继承的 07-10 原始目标：保留所有 Agent 共用的 Managed Runtime 外层，
建立 Complete Agent Service 替换边界，将 Dash Agent 与无隐藏持久状态的 AgentCore
物理分层，并完成状态权威、Fork/Companion/Compaction、Tool/Hook、协议投影、schema
与 crate 拓扑的唯一正确终态。任务完成以 AC1–AC21、W1–W9 与 S0–S6 全部具备代码和
测试证据为准，不以局部编译通过或计划文档完成代替。

## Context recovery protocol

任何上下文压缩、主会话恢复或实施中断后，主会话必须按顺序：

1. 完整阅读 `prd.md`、`design.md`、`transition-architecture.md`、`implement.md`；
2. 阅读本文件，确认当前 checkpoint、bundle handoff、未完成 gate 与最后验证命令；
3. 运行 `task.py current --source`、`git status --short`、`git log -5 --oneline`；
4. 使用内嵌 `list_agents` 核对实际 subagent 状态，不能仅凭本文件猜测；
5. 对比当前 HEAD、工作区 diff 和 handoff，修正本文件后再继续派发；
6. 若文档、Git 与 subagent 结果冲突，以代码/Git/可复现测试为实际进度，并记录差异。

## Stable checkpoint policy

- S1–S6 均在实现、独立 check、真实 tracer bullet 和本文件更新后由主会话提交；
- target-ready/activation-ready 的中间修改不冒充 stable checkpoint；
- S5 只接收完整通过 architecture 与 behavior 双重检查的 hard-cut tip；
- commit 使用项目规定的中文 Conventional Commit，并在正文列出边界、测试和遗留项。

## Current state

| Field | Value |
| --- | --- |
| Task status | `in_progress` |
| Branch | `codex/agent-runtime-final-convergence-plan` |
| Planning base | `263b990e` |
| Current wave | Wave 4 — Activation-ready freeze |
| Current checkpoint | S4 Product Lane — committed；S5 inputs in progress |
| Production path | Current Runtime → Driver Host → Native/Codex driver |
| Active implementation bundles | Platform Runtime、Dash / Native、External Agents、Product / Protocol activation refresh |
| Shared hotspot owner | main dispatcher |

## Checkpoint ledger

| Checkpoint | Status | Commit | Evidence |
| --- | --- | --- | --- |
| S0 Baseline | committed | `32ecfd2c` | 5 AgentRun fork + 1 Native fork；Runtime 129 tests；ordinary send/reconnect；migration guard |
| S1 Contract Freeze | committed | `09bff131` | final Service API 15 tests + clippy；Runtime admission 3；Host target 5；dependency/negative gates |
| S2 Target Domains Ready | committed | `7b9f0ab4` | Platform/Runtime/Host/Dash/Core/Native target checks；W2 activation component signed；5+1 fork、ordinary send、reconnect tracers |
| S3 Complete Agent Lane | committed | `179bd9c3` | Native/Codex/Remote Complete Agent target；Wire 8、Codex 14、Remote 11、Relay 5；ordinary send/reconnect |
| S4 Product Lane Ready | committed | `09fbaaa0` | Fork/Companion durable target；Runtime canonical projection；gap snapshot reload；API/frontend parity；task-local generated artifact；independent fixed-and-pass |
| S5 Atomic Hard Cut | pending | — | — |
| S6 Final Conformance | pending | — | — |

## Workstream ledger

| Work | Status | Owning bundle | Notes |
| --- | --- | --- | --- |
| W1 | completed | Platform Runtime | frozen at `09bff131` |
| W2 | target + component ready | Dash / Native | independent target and activation-component checks passed；combined activation remains Wave 4 |
| W3 | target ready | Platform Runtime | independent recheck passed；S5 repository/schema activation remains |
| W4 | target ready | Platform Runtime | independent recheck passed；S5 production binding remains |
| W5 | target ready | Dash / Native | Native Complete Agent target conformance passed；production activation remains |
| W6 | target + component ready | External Agents | independent W6/S3 check passed；production registry/canonical activation remains S5 |
| W7 | target + component ready | Product / Protocol | durable Fork/Companion、canonical Runtime feed、API/UI parity、九 consumer 与 generated activation input；production caller cutover remains S5 |
| W8 | pending | Hard Cut | unique migration/composition/deletion owner |
| W9 | pending | Final Conformance | — |

## Wave 0 baseline

### Required inventory

- [x] Record current entrypoints, production composition and legacy consumer inventory.
- [x] Record current crate graph and schema/migration head.
- [x] Confirm working tree ownership before bundle dispatch.

### Required tracer bullets

- [x] `cargo test -p agentdash-application-agentrun fork_` — 5 passed.
- [x] `cargo test -p agentdash-integration-native-agent native_fork_imports_the_requested_checkpoint_and_preserves_its_digest` — 1 passed.
- [x] `cargo test -p agentdash-agent-runtime` — 129 passed across Runtime,
  Surface, Compaction, Hook, Tool Broker and interface suites.
- [x] `cargo test -p agentdash-application-agentrun --test runtime_facade first_send_provisions_once_and_retry_replays_the_original_thread_start` — 1 passed.
- [x] `cargo test -p agentdash-application-agentrun get_paging_initial_live_reconnect_and_refresh_match_main_fixture` — 1 passed.
- [x] `pnpm migration:guard` — passed.

### Baseline inventory

- Production registration currently flows through
  `agentdash-api/src/integrations.rs`、`agentdash-api/src/app_state.rs`、
  `agentdash-local/src/runtime.rs`、`agentdash-local/src/agent_runtime_host.rs` and
  first-party `agent_runtime_drivers()` contributions.
- Current seam remains `AgentRuntimeDriverContribution` /
  `AgentRuntimeDriverFactory` / `AgentRuntimeDriver`.
- Migration head is `0083_remove_agent_frame_workspace_module_projections.sql`.
- Legacy inventory before refactor:
  `AgentRuntimeDriver` in 21 source files、`RuntimeJournalFact` in 33、
  `RuntimeSession` in 66、`agentdash-agent-types` in 17、
  `agentdash-agent-protocol` in 25、`agentdash-executor` in 10、
  `agentdash-application-hooks` in 5.
- Current crate graph confirms `agentdash-agent` still depends on
  `agentdash-agent-types`; Runtime Contract still depends on
  `agentdash-agent-protocol`; Host still depends on `agentdash-integration-api`.
- S0 workspace ownership was clean before `task.py start`; current uncommitted files are
  task activation metadata and this progress ledger only.

## Active bundle handoffs

- Platform Runtime implement: W1/W3/W4；owns Runtime Contract、new Service API、
  Runtime/Host/Surface/Tool/Hook target lane. W1 contract frozen after final checker pass:
  Service API 15 tests、clippy、Runtime admission 3 tests、Host target 5 tests and
  dependency/negative gates passed. W3/W4 target modules passed their fix-and-recheck:
  SnapshotOnly/ObservationOnly sync、typed active-turn changes、monotonic terminal effects、
  Hook deadline intersection and the unified five-kind `AgentSurfaceCapabilityFacet` are covered.
  Service API 15、Runtime 79 and Host 107 tests passed with target dependency/production-route
  gates; modules remain unstaged for S2.
  Shared hotspots remain with main.
- Dash / Native implement: W2/W5；owns Dash Agent/Core and Native adapter target lane.
  The final independent recheck passed the Dash-owned repository/service/worker boundary,
  `Native -> Dash -> Core`, command vocabulary, per-revision change cursor/digest, exact fork,
  manual/automatic compaction and B/C failure/Lost recovery. AgentCore 2、Dash 72 and Native 73
  tests passed, including 12 Native Complete Agent conformance tests. Production remains on the
  current driver route.
- S2 target code and activation artifacts are committed at `7b9f0ab4`. The W2 physical/API
  component was generated in the isolated worktree
  `F:\Projects\AgentDash-s2-dash-activation` on branch
  `codex/agent-runtime-s2-dash-activation`; it must not change the main worktree production route.
- The first activation candidate `265155ea513e576b11897d531fe0279903627e7e` passed the physical
  Agent/Core shape, dependency, test and production-route checks, but independent review rejected
  it as activation-ready: `agentdash-agent-core` and `agentdash-agent-types` still defined the same
  Core-owned types, and Infrastructure bridged them through a serde transcode. Dash/Native now
  temporarily owns the bounded S5 consumer cut needed to move all remaining consumers to their
  final owners, remove the transcode and delete `agentdash-agent-types` in the same activation set.
- The corrected W2 activation component is frozen as code tip `e1abec31` with reviewed inventory
  correction `7fbdd764`. Its two task-local patch files, SHA-256 digests, apply verification,
  remaining nine-consumer matrix and Wave 4 prerequisites are recorded under
  `activation/w2-dash-core/`. Independent recheck signed `component_ready: pass`.
- The W7 readiness audit is recorded in
  `research/current-w7-product-protocol-readiness.md`. It confirms the current graph-before-runtime
  fork, prompt-slice Companion and journal/UI feed must be replaced by the target saga,
  initial-package and Runtime snapshot/change lane before the combined hard cut.
- External Agents are frozen through main-branch commits `81a31793`、`9d339458`、`270a1485`、
  `e834498d` and `69053fc9`. Independent review closed post-dispatch Codex Unknown handling,
  unknown vendor terminal mapping, Remote endpoint callback/change production, callback
  reentrancy/deadline/effect idempotency, send-before-ledger ordering and target Wire revision
  isolation. Production Wire remains revision 3; Complete Agent target Wire uses revision 4.
- Product / Protocol target lane is frozen through `899e557b`、`c691e2bd`、`d253017f`、
  `f53033b9` and checker fix `09fbaaa0`. Its Application-owned Fork/Companion sagas persist a
  pre-dispatch marker, reuse stable effect identity through inspect/restart, pin product/Runtime/
  Agent lineage and distinguish known-child `Lost`; Fresh Companion proves package fidelity and
  materialized digest before activation, then submits the first input exactly once. Product/API/UI
  consume the Runtime Contract canonical snapshot/change/availability vocabulary, and typed cursor
  gaps execute snapshot reload before continuing from the reloaded sequence.
- W7 generated activation evidence is frozen under `activation/w7-product-protocol/` as a
  deterministic schemars output plus schema/frontend fixture hashes. Canonical generated artifacts
  and production callers remain unchanged until S5.

## Known blockers

- S2 has no remaining blocker. The current production checkpoint tracers passed:
  AgentRun fork 5、Native fork 1、ordinary first send 1 and reconnect 1.
- S3 has no remaining blocker. Main-branch integration reran Wire 8、Codex target 14、Remote target
  11、ordinary first send 1 and reconnect 1 with the convergence-owned lockfile.
- S4 has no remaining Product blocker. Main integration verified Product target 35、API artifact 3、
  Runtime projection 2、Runtime reconcile 11、frontend feed 5、typecheck、current fork 18、
  ordinary first-send 1、current reconnect 1、contracts check and migration guard.
- Complete combined `activation_ready` remains a Wave 4 gate. Platform Runtime must add a durable
  `CompleteAgentHost` repository seam; W8 will implement its PostgreSQL adapter together with the
  final migration. Dash/Native must refresh the W2 physical component to the S4 frozen revision.
  Platform and Dash owners must also clear the two remaining `test-support:guard` target repository
  findings before S5.
- Product/Application callers、Complete Agent production registration、canonical generated
  Runtime/Agent contracts、formal repositories/schema and all legacy deletion must be refreshed on
  the same frozen revision. The detailed owner/hotspot/order inventory is recorded in
  `research/current-s5-hard-cut-readiness.md`.
