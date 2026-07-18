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
| Current wave | Wave 1 — Foundation bundles ready to dispatch |
| Current checkpoint | S1 Contract Freeze — passed；checkpoint commit pending |
| Production path | Current Runtime → Driver Host → Native/Codex driver |
| Active implementation bundles | Platform Runtime、Dash / Native |
| Shared hotspot owner | main dispatcher |

## Checkpoint ledger

| Checkpoint | Status | Commit | Evidence |
| --- | --- | --- | --- |
| S0 Baseline | committed | `32ecfd2c` | 5 AgentRun fork + 1 Native fork；Runtime 129 tests；ordinary send/reconnect；migration guard |
| S1 Contract Freeze | passed | pending | final Service API 15 tests + clippy；Runtime admission 3；Host target 5；dependency/negative gates |
| S2 Target Domains Ready | pending | — | — |
| S3 Complete Agent Lane | pending | — | — |
| S4 Product Lane Ready | pending | — | — |
| S5 Atomic Hard Cut | pending | — | — |
| S6 Final Conformance | pending | — | — |

## Workstream ledger

| Work | Status | Owning bundle | Notes |
| --- | --- | --- | --- |
| W1 | contract frozen | Platform Runtime | typed Service API and additive crate boundary independently checked |
| W2 | in progress | Dash / Native | Wave 1 |
| W3 | in progress | Platform Runtime | Wave 1 |
| W4 | in progress | Platform Runtime | Wave 1 |
| W5 | in progress | Dash / Native | contract milestone may unblock；waits for Platform checked revision before final check |
| W6 | pending | External Agents | — |
| W7 | pending | Product / Protocol | — |
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
  dependency/negative gates passed. W3/W4 target modules remain unstaged for S2.
  Shared hotspots remain with main.
- Dash / Native implement: W2/W5；owns Dash Agent/Core and Native adapter target lane.
  AgentCore 2 tests and Dash ordered-history/fold/fork/compaction 7 tests passed.
  It may consume current contract milestone, but final W5 check waits for Platform checked
  revision.

## Known blockers

None at task activation.
