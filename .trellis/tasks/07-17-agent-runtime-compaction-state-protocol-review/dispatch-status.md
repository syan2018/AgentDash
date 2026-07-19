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
2. 阅读本文件以及
   `research/final-protocol-types-codegen-cutover.md`、
   `research/relay-runtime-wire-placement-activation.md`、
   `research/product-canonical-presentation-cutover.md`、
   `research/s5-production-composition-and-deletion.md`，确认当前 checkpoint、bundle
   handoff、未完成 gate 与最后验证命令；
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
| Current wave | Wave 5 — S5 staging integration |
| Current checkpoint | S4 Product Lane — committed；Wave 4 inputs frozen；S5 staging ready |
| Production path | Current Runtime → Driver Host → Native/Codex driver |
| Active implementation bundles | 用户进度审计期间暂停；Platform production、Product P3、Relay production 三个 dirty worktree 已冻结 |
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
| W8 | in progress | Hard Cut | unique migration/composition/deletion owner；Product PG 与 Platform Tool slices checked/integrated，presentation/Product/Relay/deletion remains |
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
- Wave 4 External Agents activation component is frozen at
  `ffaf54a7` in `F:\Projects\AgentDash-s5-external-activation`. Independent recheck signed
  `component_ready: pass`: Codex/Remote no longer own the legacy driver/journal/context-activation
  path, Codex process initialization has a typed ready boundary, and the exact 20-consumer
  deletion manifest, Wire/Relay gates and no-lockfile patch all passed.
- Wave 4 Platform Runtime activation component is frozen at
  `ed1a7d95aa9c4d10feda5cbed29cdb3c4bad02a7` in
  `F:\Projects\AgentDash-s5-platform-activation`. Independent recheck signed
  `component_ready: pass`: the durable Host repository owns service/offer/binding/source/effect/
  lease facts; redispatch atomically archives the prior attempt state, receipt, surface receipt and
  inspection; attempt history is continuous and append-only; the W8 manifest freezes the matching
  schema and transaction constraints.
- Platform Runtime owner then produced the real legacy-cut activation tip `b078b3ba`, including
  final public roots, a new Managed Runtime transaction aggregate, immutable placement, the sole
  Runtime Wire revision 4 and 33 exact cross-owner consumer records. Independent full review
  rejected this tip before S5: the fact-graph validator can mismatch pending commands and duplicate
  projection operations; source reconcile and Runtime projection/change are still separate
  commits; callback idempotency outcomes are process-local; and first ordered-source sync can pair
  a partial-page cursor with a latest snapshot. Those findings were routed back to the owner before
  the final tip below.
- Platform Runtime final activation tip is
  `30d9a55597e36fc5af0591c420346c3217c1dbae`. Independent review returned
  `component_ready: pass` after the complete fact graph, source observation causal changes/outbox,
  single Runtime CAS, durable callback outcome, Host-atomic callback route/tombstone, trusted-cursor
  rules and concrete source-section delta mapping all passed their mutation and restart tests.
  Four Platform crates passed 78 locked tests and strict clippy; the committed tip retains the
  frozen base lock and leaves migration/composition/generated outputs to W8.
- Wave 4 Dash / Native activation tip
  `b8b2ba0e0d21691afd45b18f0d9bb95e9ffd6734` passed Core purity, injectable Dash/Native durable
  stores, exact Tool/Hook callbacks, consumer reduction and direct tests, but independent review
  found a remaining component blocker: `DashCompleteAgentStore` persists source/surface mutation
  before its effect receipt. The owner is revising create/fork/apply/revoke into one durable
  CAS/atomic commit contract and adding receipt-loss crash-gap coverage before recheck.
- Dash/Native owner subsequently produced `884913d8` + evidence `4822327a`: create/fork/apply/
  revoke now atomically commit effect evidence with source repository/metadata mutations, execute
  uses an Accepted reservation and recovery CAS, and Native legacy driver/context/projection/hook/
  mapping/presentation/tool files are physically deleted. Recheck confirmed the durable gap is
  closed but found one remaining live-process gap: a committed-but-lost surface response could
  leave the same service instance's callback materialization stale. That finding and its evidence
  inconsistencies were routed back to the owner before the final tip below.
- Dash/Native final activation tip is
  `6c38dd3de7527859f21e21b28a6b7cb37c7e0f5c` with production code through
  `ce46985701e40db72a40a6b2e68d69f831f005a6`. Independent review returned
  `fixed-and-pass / component_ready: pass`: same-instance surface response loss now reconciles live
  callbacks from durable metadata; Core/Dash/Native locked tests pass against the recorded
  temporary component lock; the committed tip retains the frozen base lock; Native deletion and
  W8/session-parity ownership manifests are consistent.
- Wave 4 Product / Protocol activation tip `92f562f5` passed target Fork/Companion/feed behavior,
  but independent review found three W7 owner blockers: the prepared Product graph transaction does
  not yet carry the real Lifecycle/Frame/Lineage rows; initial context remains a parallel contract
  without a proved canonical service-API mapping; and the six caller manifests are not precise
  enough to execute the hard cut. The owner is freezing the aggregate transaction payload,
  lossless mapping tests and per-symbol activation inventory before recheck.
- Product/Protocol fixes are committed as `7f79e21f`, `66ef2a67` and `a08e871b`. The domain
  component now carries a complete immutable Product graph transaction payload, CAS/visibility
  behavior, dev-only canonical context parity and an executable per-symbol caller inventory.
  Production caller activation remains an explicit S5 sequence dependency: W8 must first provide
  the frozen AppState repository bindings and canonical Managed Runtime TypeScript outputs, then
  Product/Protocol receives temporary ownership of the S5 staging caller files to perform the real
  source switch before W8 completes composition/deletion. No parallel DTO or compatibility shim is
  permitted.
- Product/Protocol shared-foundation input is finally signed at
  `67d9eef5f078dcb10077bbdb2eab1a05d2a33674`. Independent review confirmed the real two-parameter
  Companion coordinator signature, 78 per-file/per-symbol caller records, exact API/frontend/
  generated records, canonical context parity, complete Product graph transaction payload and the
  precise W8-only lock delta. Its status remains intentionally
  `production_caller_activation_pending_w8_prerequisites`; it is an accepted S5 input, not a claim
  that production callers have already switched.

## S5 entry state

- S2 has no remaining blocker. The current production checkpoint tracers passed:
  AgentRun fork 5、Native fork 1、ordinary first send 1 and reconnect 1.
- S3 has no remaining blocker. Main-branch integration reran Wire 8、Codex target 14、Remote target
  11、ordinary first send 1 and reconnect 1 with the convergence-owned lockfile.
- S4 has no remaining Product blocker. Main integration verified Product target 35、API artifact 3、
  Runtime projection 2、Runtime reconcile 11、frontend feed 5、typecheck、current fork 18、
  ordinary first-send 1、current reconnect 1、contracts check and migration guard.
- Wave 4 combined inputs are frozen under `activation/s5-combined/`. The mechanical verification
  passes for all four exact tips, clean worktrees, common base, zero component lockfile diffs, task
  artifacts and the sole same-file overlap
  `crates/agentdash-application-agentrun/Cargo.toml`.
- Platform Runtime, Dash/Native and External Agents are `component_ready: pass`.
  Product/Protocol is `shared_foundation_input: pass` with an explicit S5 sequence dependency:
  W8 first creates the final repository/AppState/generated prerequisites, then the original Product
  owner performs the real caller switch in the same staging worktree.
- W8 owns only migration, PostgreSQL adapters, workspace/lockfile, production composition,
  canonical generation and zero-consumer physical deletion. Domain behavior findings return to
  Platform, Dash/Native, External or Product owners.
- There is no remaining Wave 4 blocker. S5 is not a stable checkpoint until component integration,
  Product caller activation, final deletion/lock generation and both independent cutover checks
  pass on one staging tip.
## S5 staging progress

- The hard-cut staging branch integrated the four frozen component ranges and the Wave 4 task
  checkpoint without conflict. Its clean integration tip is
  `91cbdc077d97a6c280dabaf6acad1bad7ede1300`; the sole expected Cargo manifest overlap resolved to
  the reviewed union.
- W8 audit correctly stopped before writing migration or adapters. Existing PostgreSQL repositories
  implement the legacy Runtime/Driver Host facts and cannot be reused as final repositories; only
  PgPool, transaction/error/JSON helpers and the embedded-PostgreSQL harness are reusable.
- The audit found missing production orchestration seams that belong to domain owners, not W8:
  Platform Runtime must provide the production Managed Runtime Gateway and Host create/resume/fork
  dispatch; Product/Protocol must provide production Fork Runtime, Fresh Companion Runtime, Product
  graph and Runtime projection adapters. Platform and Product owners are implementing these seams
  in their reviewed activation worktrees before W8 continues.
- The planned unique migration is `0084` and will create final Product, Runtime, Host and Dash
  partitions. W8 remains paused until both owner fixes pass independent review; no migration,
  AppState binding, canonical generated artifact or production route has been changed yet.
- Platform Runtime closed the production Gateway/Host seam through checked tip `b1dce69d`.
  Application commands expose only Runtime-owned identities; Create/Resume/Fork/Activate persist
  typed lifecycle evidence, initial context is proven contribution-by-contribution, and Fork
  preserves `ChildKnown` before `Provisioned`. Complete Agent inspection now returns a closed
  Create/Resume/Fork/Command/Surface outcome, so post-side-effect recovery only inspects the same
  effect and never replays lifecycle commands. The final checker returned `fixed-and-pass` after
  five crate test/clippy/negative gates and applied-receipt checkpoint recovery.
- Product domain hard cut is checked through `6aee3b19`: the Application saga now exposes only
  Runtime `Fork` and `Activate`, retains Accepted receipts for same-operation inspection, uses
  typed Runtime identities and a prepare-only Product graph adapter. The dependent Gateway
  adapters are being completed in
  `F:\Projects\AgentDash-s5-product-runtime-adapt` against the checked combined contract.
- Dash/Native typed inspection is checked through adapter commits `33b7a24d` and `a150ec7f`.
  Head/CompletedTurn remain exact Fork cutoffs; Item is rejected before side effects because the
  current child cannot reconstruct a live active ledger. External typed inspection is checked
  through `4833ffe5` and `ad611c79`; Codex verifies completed native turns and a versioned
  source-authoritative digest, Remote validates all receipt coordinates, and Wire/Relay use the
  sole canonical revision 4.
- The hard-cut staging branch has integrated all checked owner inputs at clean tip `1f9e4ac4`.
  W8 has resumed its unique migration/repository/generator/composition/deletion scope while the
  Product owner finishes the three final Runtime Gateway adapters in an isolated branch. Neither
  branch is an S5 checkpoint until the Product adapter commit is checked, integrated, and the
  complete W8 cutover passes architecture and behavior review.
- Platform Runtime subsequently produced two bounded W8 prerequisites:
  `ec5450c9` adds reversible codecs and validated loads for the final Runtime/Host/Callback fact
  graphs, and `f210c05d` makes the public Service API, Runtime Contract and Wire revision 4
  vocabulary mechanically exportable to TypeScript without `bigint`. The independent checker
  returned `fixed-and-pass` at `646708de`: it added Host malformed-load rejection for an invalid
  callback delivery/deadline and passed Runtime 42、Contract 8、Host 47 + integration 1、Wire 7、
  Service API 17 plus strict clippy across all five crates. W8 must integrate all three commits.
- The canonical generation boundary is now fixed as one Runtime schema/TypeScript closure, one
  Service API schema/TypeScript closure, and Wire framing which imports both canonical type sets.
  W7's manifest-owned `agent-runtime-validators.ts` remains an output; no second public Runtime
  projection schema or hand-written persistence/frontend DTO is allowed.
- The first Product Gateway adapter commit `d33fe83e` was rejected by its independent check. The
  Product owner replacement `efdc5bf7` preserves typed `ChildKnown`/`Provisioned` evidence, pins
  Fork/Fresh source bindings, separates clean `Failed` from uncertain `Lost`, guards projection
  thread identity and exposes immutable typed saga accessors required by PostgreSQL. The original
  checker returned `fixed-and-pass` at `eeddb1bd` after additionally requiring Fork Lost to match
  the pending operation and Activate Lost to preserve the already pinned child progress. Real
  Product compile/test remains an explicit post-SPI-deletion and post-lock-generation W8 gate.
- W8 is actively building the sole `0084` forward migration, Product/Dash PostgreSQL adapters and
  canonical generators in the staging worktree. Its intermediate worktree is intentionally not a
  checkpoint: Platform codec/TS and Product adapter inputs must be independently checked and
  integrated before composition, deletion, lock generation and the two final S5 checks.
- W8 has now integrated the checked Platform codec/TS sequence and the checked Product adapter
  sequence. The clean staging tip is `f064dd5d`; canonical generation is committed at `0fdb6c4f`
  with one Runtime closure, one Service API closure and Wire imports of both, including hard
  failures for missing owned declarations and `bigint`. The next W8 boundary is the new final
  Runtime/Host/Callback PostgreSQL repository set, followed by composition, legacy deletion and
  the sole final lock generation. This clean internal tip is reviewable but is not an S5
  checkpoint.
- The final Product/Dash/Runtime/Host/Callback PostgreSQL repository set is committed at
  `7b882bd5`; an isolated scratch crate type-checked all three final Runtime/Host/Callback trait
  implementations without reusing legacy repository traits. The final in-process composition
  kernel is committed at `f8ce1ee4`, wiring the three PostgreSQL authorities, process service
  registry, Host, Callback Broker and Managed Runtime Gateway. These remain internal W8 commits,
  not S5.
- A production registration owner gap was returned to Dash/Native. The owner created the
  dependency-light Complete Agent contribution at `856eb0c9`, and then fixed independent review
  findings at `3e26d0e2`: Integration now submits only declared/claimed build facts, while Host
  must independently verify offer provenance; Remote registration exposes immutable local/remote
  instance-generation mapping instead of hiding it in a factory closure. The owner fix still needs
  the original checker recheck before S5 acceptance.
- A callback routing gap was returned to Platform Runtime and fixed at checked commit `75deff34`.
  Host now resolves and validates Runtime thread, binding/generation, source and bound/applied
  surface evidence before invoking Tool/Hook handlers. Product handlers receive typed resolved
  context and no longer need Host repository access. Host 48 + integration 1 and strict clippy
  passed.
- Product caller activation is active in an isolated worktree. The final Managed Runtime feed is
  moving out of the platform `Session` namespace, and legacy journal/NDJSON reducers are deleted
  only after canonical snapshot/change, gap reload, reconnect, activity/compaction, availability
  and view-model coverage exists. Thread-name refresh uses the existing committed Project event.
  Workspace-module presentation and shell/PTY terminal live state were correctly classified as
  separate Product durable channels, not Managed Runtime conversation fields; Product owns their
  typed ports/API/frontend while W8 will own final PostgreSQL/composition.
- Product caller activation was committed as `7570137b` and integrated into the W8 staging branch
  as `2fcfb671`, but its independent review found owner-level blockers before S5 acceptance:
  Managed Runtime routes bypass the guarded Product gateway; Workspace/Terminal frontend endpoints
  have no production API/repository/composition path; their TypeScript DTOs are a handwritten
  second contract; Workspace snapshot pending intents/ack failures are not replayed or retried;
  Terminal platform availability/control changes incorrectly consume the source-owned sequence;
  and an initial Product feed baseline failure never schedules recovery. The original Product
  owner is fixing the Product gateway/routes/canonical root, durable frontend consumption and
  sequence/retry semantics while W8 retains PostgreSQL, Local/Relay and composition ownership.
  The exact `7570137b` review result is `needs-owner-fix`; frontend typecheck and 41 focused tests
  passed, but these tests prove only the isolated type-only lane and not a production data path.
- The Product owner fix `48493d92` closes the seven original caller findings in shape and behavior:
  guarded Runtime/Product routes, typed Workspace/Terminal routes, final source-binding fences,
  durable pending/ack consumption, separate Product/source terminal changes, initial-feed recovery
  and removal of the handwritten frontend DTO. Its exact recheck passed 44 focused frontend tests
  and Rust formatting, but found two further Product-owner blockers before acceptance. The Product
  generator must import Runtime-owned evidence from the sole Runtime closure instead of
  `export_all`-duplicating it, and Terminal reconcile must terminalize
  `Unknown/OwnerFenceUnprovable` as a Product-owned Lost change without consuming a source cursor,
  with resolution-specific owner/fence evidence validation. Both findings are back with the
  original Product owner; W8 must wait for the checked replacement tip.
- The Product replacement tip is now independently accepted at
  `7593abb600e99ffcf941d48e20d3c1b7db9720a7` (parent
  `48493d92c7fdf51927e5971ea544f9d0ee3c3e79`). The final recheck returned `pass`: all seven
  original findings, canonical Runtime imports, Product-owned reconcile Lost semantics,
  resolution-specific owner evidence and strict page cursor continuity passed 45 focused frontend
  tests plus targeted ESLint、rustfmt and diff checks. W8 may now integrate both commits and owns
  the remaining concrete PostgreSQL/UoW/producers/composition、generated artifact、legacy endpoint
  deletion and final lock gates.
- W8 froze its next clean internal boundary at `312976a5`: the workspace-wide Platform SPI rename,
  the initial final `0084` Product extension, Product projection PostgreSQL foundation and Local/
  Relay legacy Runtime-wire removal are committed without `Cargo.lock` or frozen evidence changes.
  The accepted Product owner commits were then integrated without conflict as `f3803dbb` and
  `58c537b7`. W8 is now adapting the concrete PostgreSQL/composition implementation to the final
  Product protocol before deleting the remaining legacy crates/routes and generating the sole
  final lock and contracts.
- W8 then committed clean internal boundary `c3cc58b9`. Product identity now belongs to Domain;
  Workspace、Terminal and Product protocols no longer depend on the legacy Runtime target;
  AgentRun removed its facade/journal/frame/runtime-session public roots; Infrastructure
  composition uses the final Managed Runtime Product gateway; and Product/Dash PostgreSQL compile
  defects are closed. `cargo check -p agentdash-application-agentrun` and
  `cargo check -p agentdash-infrastructure` pass. The frozen lock remains restored; the next W8
  boundary owns API/Application/Lifecycle/VFS caller removal, final AppState service/Product/
  terminal-reconcile injection, physical legacy-crate deletion and extension-gateway rename.
- Hook/Session caller hard cut is committed internally at `8d22d6cf`.
  `agentdash-application-hooks` is physically absent from the staging workspace; Workflow、
  Lifecycle and Application compile, Runtime Hook 1/1、Host callback Hook 2/2 and migration-history
  guard pass, and no lockfile was committed. Hook execution now has the final owner:
  `AgentHostCallbacks::invoke_hook` through `CompleteAgentCallbackBroker` and
  `CompleteAgentComposition::host_callbacks()`. This boundary exposed two non-optional follow-ups
  before S5: Workflow AgentCall must be reconnected to the typed Managed Runtime path rather than
  remain unsupported, and test-support's legacy `RuntimeEvent*` trace validator must be deleted if
  unused or rewritten against final Runtime snapshot/change semantics.
- The test-support follow-up is closed at `4c1e15dc`: the zero-consumer legacy
  `RuntimeTraceValidator` and its `RuntimeEvent*` tests are physically deleted while session-parity
  fixtures and the cross-adapter conformance support remain. Test-support check/test and migration
  guard pass with the frozen lock restored. Workflow AgentCall remains with the Product/Application
  owner in an isolated worktree; its final boundary is Workflow-owned orchestration identity and
  dispatch port → Lifecycle/Application idempotent Product target materialization → AgentRun-owned
  provisioning/binding/mailbox → typed Managed Runtime SubmitInput, with Runtime terminal and
  Lifecycle output artifacts driving completion.
- A read-only legacy audit of the earlier `58c537b7` base confirmed why the remaining cut must be
  caller-first rather than export-first. Only `runtime_session_delivery` was actually
  zero-consumer there; the other five legacy Application Runtime modules still had real consumers.
  The final replacements are Domain `AgentRunTarget`, Product binding/projection gateways,
  `ManagedAgentRuntimeGateway`, and the independent Workspace/Terminal protocols. The same base
  still had 147 Runtime journal、12 Driver-contribution and 9 ContextActivation matches, plus
  active consumers of `agent-types`、`agent-protocol`、executor、application-hooks and the old
  runtime-gateway name. W8 is following the audited order: Complete Agent composition → Product
  callers → journal → Tool/Hook → extension gateway → protocol/types crates → Platform SPI →
  final generated contracts and lock. The final S5 check must rerun these counts on the cutover
  tip rather than treating the base audit as completion evidence.
- Final Host/Product API composition is committed internally at `a535ae01`. AppState now wires the
  Complete Agent registration/Host、Managed Runtime Product gateway and Relay Terminal projection/
  reconcile path; `lifecycle_agents.rs` was reduced from the legacy Runtime/journal/mailbox/session
  surface to seven final Product query/ack routes; `project_agents.rs` no longer owns the old
  run-start/message saga; execution profile reads Host-persisted service instances. AgentRun and
  Infrastructure checks pass, and all API errors outside Product-owner `routes/workflows.rs` are
  zero. The frozen lock was restored and migration history guard passes. The remaining twelve
  Workflow route errors are assigned to the isolated AgentCall owner; W8 continues with extension
  gateway rename and protocol/types/executor/SPI physical deletion.
- The isolated Workflow AgentCall owner component is committed at
  `1bcb137f03f1b81532effc88c785f82821c380d1` and is under independent review. Workflow now owns
  typed AgentCall identity/request、Prepared/Dispatched history and a dispatch port; AgentRun owns
  a five-phase materialize → Create → Activate → binding commit → SubmitInput saga with stable
  phase identities、digest conflict、revision CAS and receipt inspection; the executor only claims
  and starts the node after accepted dispatch. Workflow 54 tests and four affected crate checks
  pass with the frozen lock restored. AgentRun's target test compile remains blocked by a
  base-owned legacy test-support import and must run immediately after W8 integration. W8 must not
  integrate this component until its independent checker signs the exact tip.
- The exact `1bcb137f` AgentCall review returned `needs-owner-fix`. ContinueCurrent still followed
  the CreateNew phase plan; current Agent authority was not durably resolved across predecessor
  nodes; Product graph/schema transaction semantics were insufficient for W8 to implement
  mechanically; Dispatched、Claimed and Started used separate commits with a Claiming crash hole;
  and `LifecycleRunRepository::update` lacked revision/CAS while overwriting the aggregate.
  Production composition correctly did not fall back to the in-memory saga, but therefore had no
  active AgentCall route. The original Product/Application owner is fixing separate phase plans,
  durable current authority、Product graph/schema contracts、an atomic dispatch/claim/start
  transition and Lifecycle mutation CAS before recheck.
- The AgentCall owner fix is committed at
  `44bc1c497c5762c7e00fcb61f96dd9a888128543` and is under exact-tip recheck by the original
  checker. ContinueCurrent now resolves one durable predecessor/current Agent authority and uses
  only SubmitInput; CreateNew retains the five Product phases. Dispatched、Claimed and Started are
  one domain transition and one LifecycleRun revision CAS, with stable request/claim replay after
  an accepted Product effect followed by CAS failure. The component also freezes the durable
  saga/effect/graph repository contract and rejects production construction without durable
  dependencies. W8 retains the LifecycleRun PostgreSQL revision/CAS, saga/effect tables, graph
  transaction adapter, production factory composition and recovery worker.
- The exact `44bc1c49` recheck closed ContinueCurrent authority/phase selection, the AgentCall
  claim crash hole and the Lifecycle revision/CAS entry point, but returned `needs-owner-fix`
  before W8 integration. `WorkflowAgentCallBindingCommit` must carry request ID and payload digest
  explicitly. More importantly, Function success cannot be converted to failure after a terminal
  CAS conflict, and HumanGate open/resolve cannot create orphan gates or leave a resolved gate with
  a Running node. The Product/Workflow owner is extending the same stable prepare/effect
  receipt/reload-and-reapply protocol to Function and HumanGate, with typed schema contracts and
  concurrency/crash tests. W8 must preserve independently mutated channel registry state when its
  PostgreSQL Lifecycle CAS updates the executor aggregate and revision.
- The Workflow owner completed the second fix at
  `df37d64c97745faee8018f07572ab39478c72d21`, now under final exact-tip review. Function nodes
  persist one stable request/effect and terminal receipt before reapplying the Lifecycle CAS, so a
  successful side effect is neither re-executed nor converted to failure after conflict/restart.
  HumanGate open/resolve uses stable gate/effect receipts, removes the raw fallback and recovers
  concurrent open, post-resolution CAS loss and duplicate decisions. Binding commits now carry
  request ID and payload digest explicitly. Workflow 64 tests, targeted formatting, diff and
  migration history gates pass with a clean lock.
- The final AgentCall/Workflow check returned `fixed-and-pass` at
  `1c6e6ab519ffa49402395bccb40ce8d055e85c27`. The bounded checker fix scopes terminal HumanGate
  receipt lookup to one orchestration and recursively resolves nested path/attempt, preventing
  cross-orchestration same-path collisions while preserving duplicate-decision replay. The two
  focused tests and the resulting Workflow 66/66 suite pass; no owner finding remains. W8 may
  integrate `44bc1c49` + `df37d64c` + `1c6e6ab5` and implement their PostgreSQL effect/CAS/
  composition contracts.
- W8 then exposed a new implementation-level Function unknown-outcome gap: a PostgreSQL row lock
  held across raw HTTP/Bash execution cannot make the external side effect atomic with the runner
  receipt. A crash after the effect but before receipt commit would make the old
  `Unknown -> execute` mapping repeat the side effect. W8 has stopped treating this as complete.
  The Workflow owner is adding distinct not-applied, accepted/in-flight and lost observations;
  only not-applied may dispatch, while an uninspectable ambiguous raw effect becomes a typed
  Blocked node with evidence rather than retry or false failure. W8 will persist the dispatch claim
  before external I/O and prove a new runner instance never invokes the raw effect twice.
- The Workflow unknown-outcome contract is committed at
  `facb7d73a8513488fdbeb21e00db9ceb8bdf15f1` and is under independent combined-tip recheck.
  Only `NotApplied` may execute; `Accepted/InFlight` only inspect; terminal observations replay the
  existing receipt; `Lost` CASes the node to Blocked with effect identity/digest/reason/evidence
  and never enters Failed or `failed_nodes`. Workflow 67/67 and six Function-focused tests pass.
  W8's raw runner now persists accepted owner/token/lease before I/O, respects live leases, marks
  expired uninspectable effects Lost, fences terminal commit and quarantines late receipts.
- The combined Workflow tip `2abd5a8c985d607e0ca2682e74ce3033706c9805`
  (`1c6e6ab5` + `facb7d73`) passed independent review with no remaining owner finding.
  Function 6/6、Lost 1/1、HumanGate 2/2 and Workflow 69/69 pass. W8 must additionally provide a
  durable wake/reconcile driver so Accepted/InFlight nodes are re-inspected and eventually become
  terminal or Lost/Blocked without ever authorizing a second external dispatch.
- The W8 API compile exposed a separate Lifecycle read-model owner gap. The disconnected legacy
  `run_view_builder` derives current attempts/traces from the deleted
  `agent_run_runtime` binding ports, while the final Product projection query is target-scoped.
  Rebuilding that join in `routes/workflows.rs` would create an API-owned second business
  projection. The Application Lifecycle owner is replacing it with one final query/builder over
  Lifecycle facts, final Product Runtime binding and canonical Product projection; API will only
  perform DTO mapping. W8 continues to delete the old Hook/tool-catalog route inputs and will
  compose the checked final query.
- W8's durable Workflow/AgentCall hard cut is committed internally at
  `679a6802e7d01f950e882ad1ebbd2d5982af753b`. Migration `0084` now contains Lifecycle revision,
  Workflow Function/HumanGate effect state and AgentCall saga/effect/graph ledgers; PostgreSQL
  implements Lifecycle CAS while preserving channel registry, atomic gate/effect receipts,
  Product saga/graph replay and the fenced Function lease/lost/late-receipt protocol. Production
  uses only `new_durable`, final AgentCall dispatch and a recovery scan. Two real embedded
  PostgreSQL suites prove receipt-loss → Lost → Blocked with raw invocation one, repository replay,
  CAS/channel registry, concurrent gates and graph drift. Infrastructure, canonical generators,
  contract freshness, migration guard, rustfmt and diff pass with the frozen lock. API remains
  intentionally blocked only on the checked Lifecycle final-query component.
- W8's physical AgentRun deletion audit found that the previously accepted thread-name observer
  still depends on `CommittedDurablePresentation/BackboneEvent`, while the final Complete Agent and
  Managed Runtime contracts do not yet carry a typed thread name. Deleting it would silently lose
  set/clear and Project `title_changed` invalidation. The old file is therefore restored but remains
  disconnected from production, and a bounded cross-owner component is active in
  `F:\Projects\AgentDash-s5-thread-name`: source-authoritative Complete Agent snapshot → normalized
  Runtime projection/change → Product committed-change observer with binding/thread currentness.
  W8 must not restore the old journal observer or delete the feature before this component passes
  independent review.
- A second W8 deletion review restored every Product API/module that lacked a proven final
  replacement: Canvas、extension runtime、Terminal、VFS surfaces、Workspace module、Story、
  Routine、Companion、Lifecycle/runtime traces and AgentRun workspace remain. Only old Runtime
  surface/control/bootstrap、the disconnected legacy AgentRun path/ports/executor and three old
  Infrastructure implementations remain in the physical deletion set. Restored extension callers
  now use the renamed extension gateway, and Project notification/stream/AppState boundaries stay
  intact until the typed thread-name owner component replaces the old observer.
- The corrected extension/crate boundary is committed internally at
  `dfb4f903751020a5d6518dad87c5b5b895966395`. The extension gateway is a real hard rename with
  old session/MCP/tool adapter modules removed; the old executor、AgentRun runtime
  surface/control/bootstrap、legacy AgentRun path/ports、test-support control-effect and three
  disconnected Infrastructure implementations form the evidence-backed deletion set. Product
  APIs without replacements remain restored. Extension Gateway + AgentRun lib tests pass 120/120;
  Application owner tests and Application/Lifecycle/Workflow/Infrastructure checks pass; API
  errors remain only in the AgentCall-owned Workflow route. The lock was not committed. W8's next
  inventory explicitly tracks restored disconnected Product/Lifecycle/Workspace source,
  Infrastructure cfg(test)/integration consumers and the held thread-name observer; each must be
  connected to its final owner or deleted with zero-consumer evidence before S5.
- The thread-name authority component is committed at
  `700bacb9f593d971ce341e46d10376372119b016` and is under exact-tip independent review.
  Complete Agent snapshot now distinguishes unsupported from source-authoritative set/clear;
  Codex maps native read/change authority, Dash reports typed absence, Remote/Wire preserve the
  payload, Managed Runtime owns the normalized snapshot/change, and Product observes only
  committed Runtime changes to invalidate the exact Project AgentRun list. The old Backbone
  observer is deleted. W8 still owns PostgreSQL snapshot persistence, production outbox observer
  composition, canonical generation and the Product observer tests after the legacy test-support
  import is removed.
- The independent thread-name check returned `fixed-and-pass` at
  `fd149570e93997a0ef6d73bcd0643a2b8909e843`. Runtime now accepts only
  `AgentAuthoritative + Exact` initial/set/clear name evidence and rejects lower authority or
  fidelity before any mutation/change/outbox. The Product observer additionally fences the
  internal source projection revision, non-zero source sequence and authoritative/exact
  change/snapshot evidence. W8 may integrate `700bacb9` + `fd149570` and owns the committed
  Runtime change consumer composition, final fact persistence, canonical generation and
  Codex set/clear restart tracer.
- The read-only final deletion audit at `ddbc94b4` found that Executor is already absent from the
  package graph, while Agent Types still has five normal direct dependents and Agent Protocol has
  twelve; Protocol Codegen is kept alive by `contracts:check`. Real blockers are VFS/Task tool
  surfaces, Product/Control-Plane DTOs, Relay prompt framing, Lifecycle/Extension/Workspace
  RuntimeThread migration and generated Codex/Product contracts. Platform SPI itself remains for
  non-Agent auth、function、MCP、mount、routine、extension and VFS ports, but its Agent
  re-exports/session persistence/old hook/prompt/runtime facades must migrate out. Journal,
  ContextActivation and old Driver Rust producers are zero-consumer, but Product routes/modules
  currently omitted from module trees cannot be mistaken for deletable features. The final order
  is caller/owner migration first, then old SPI/protocol/codegen/types directories, delete-store
  SQL cleanup, workspace manifests and one final lock.
- W8 removed the evidence-backed dead Runtime persistence lane at
  `d494298471ecaec6e94bd1eb69dbb751701d0227`: five undeclared legacy Runtime worker/driver/
  PostgreSQL modules and two exclusive tests are absent; Infrastructure no longer consumes old
  `agent_run_runtime`, Agent Protocol, Remote Runtime or LLM Provider dependencies, and
  Application Ports no longer carries a zero-consumer Agent Types dependency. The shared
  `state_change_store` remains because Story/StateChange still consumes it. Infrastructure,
  final repository replay, metadata and both repository guards pass with the frozen lock.
- W8 integrated the checked thread-name component and completed its production delivery at
  `ddbc94b4ed4ce3362c3f21312c656a270602daec`. A durable ordered lease worker consumes committed
  Runtime outbox rows, fences per-thread gaps/stale tokens, resolves the final Product binding and
  invokes `AgentRunThreadNameProjectionObserver`; only Project invalidation is emitted. Migration
  `0084` stores delivery operation state but no scalar title/name truth. Isolated embedded
  PostgreSQL proves source-authoritative set/clear replay, normalized projection drift rejection,
  ordered delivery and stale-token fencing; observer 3/3, activation, generators and guards pass.
  The frozen lock remains unchanged until AgentCall integration and final legacy deletion.
- The Application Lifecycle owner completed the final run-view query boundary at
  `a2958fee8b500db47a1599471f1aea42f607d5d1`. One query combines Lifecycle facts, final Product
  binding and canonical Managed Runtime projection while preserving typed `Absent`、`Current` and
  `Stale` execution trace states, recursive orchestration attempts and target/thread/source fences.
  W8 has integrated the owner commit as `e571754f` and temporarily composed the query while its
  exact-tip independent check runs. The final cut is explicitly the new Lifecycle Agent execution
  contract with lossless runtime state、stale reason、current attempt and attempt history; mapping
  this information back into the old `RuntimeSessionRefDto` / `runtime_trace_refs` shape is not an
  acceptable endpoint and no compatibility DTO is retained.
- The Lifecycle DTO cut exposed three source files omitted from the API module tree while their
  Product callers remain real: frontend Lifecycle、Task and Story surfaces still call
  SubjectExecution and ProjectActiveAgents, and AgentRun Workspace remains a real Product feature.
  They cannot be deleted merely because the current route registration is disconnected. The
  Application Lifecycle owner is extending the same final query boundary with Subject/Project
  aggregation; W8 will restore the checked routes, migrate Workspace to its final Product/Workspace
  ports, then delete the zero-consumer legacy `application-ports::lifecycle_read_model`. This keeps
  one business projection owner and prevents a compile-clean cut from silently breaking actual
  Product paths.
- The exact `a2958fee` Lifecycle query review returned `needs-owner-fix`. The initial `Stale` view
  loses the actually observed target/thread/source/snapshot fence evidence, and the initial attempt
  view loses the runtime-node structure required by SubjectExecution and latest-node consumers.
  API cannot repair either loss without becoming a second query owner. The original
  Product/Lifecycle owner is therefore extending Product projection mismatch evidence and the
  Application query result itself, together with final SubjectExecution/ProjectActive aggregation.
  W8 retains the single combined `Cargo.lock`, AppState/route composition, canonical generation and
  old-port deletion after the replacement tip passes the same independent checker.
- The Product/Lifecycle replacement is committed at
  `5675b9de7fbe17aef44a730e689c193be831a2b7` and is under exact-tip recheck. It adds a typed Product
  runtime snapshot observation, lossless stale expected/observed fence evidence, complete recursive
  Runtime node views and one LifecycleRun/SubjectExecution/ProjectActive query owner. Five focused
  query tests and the AgentRun library check pass; W8 must wait for the independent verdict before
  final route/contract integration.
- The replacement recheck returned `fixed-and-pass` at
  `e9d08621a0a83cc933d2bd2259244f19fcaea814`. The bounded checker fix preserves an observed
  snapshot's missing source binding instead of backfilling expected Product evidence, and adds
  deterministic association/attempt tie-breakers plus binding-change and missing-source tests.
  AgentRun check、format、diff and scoped dependency/legacy gates pass. The isolated branch's full
  Lifecycle test/check remains blocked only by the known W8-base `with_function_runner` and removed
  test-support imports, so the combined W8 tip must run the focused seven tests and full affected
  crate/API/frontend gates without those exemptions.
- A current-tip read-only deletion inventory replaced the older `ddbc94b4` assumptions. Agent Types
  has five direct dependencies but only Protocol、Application VFS and Platform SPI are mounted true
  consumers; Agent Protocol has twelve direct dependencies, with API、AgentRun、Application Ports、
  Contracts、Platform SPI、Relay and Protocol Codegen still real. Platform SPI must remain for
  non-Agent platform ports but must lose its Agent types/reexports、runtime surface、prompt、
  session-persistence and old Hook/protocol coupling. No exact `RuntimeSession*` symbol is
  vendor-owned; the current 766-line/115-file inventory is therefore a real semantic hard-cut
  backlog except historical migrations、fixtures and Dash history terminology. Protocol Codegen
  is still invoked by `contracts:check`, so Codex vendor generation must move to the Codex
  integration owner before deleting Protocol/Codegen.
- The same audit found that 0084 declared old RuntimeSession tables retired without dropping them,
  and `PostgresAgentRunDeleteStore` still targeted retired Runtime/Context tables. W8 has already
  corrected the in-flight migration with explicit drops and removed the zero-production-consumer
  delete port/store/export/test wiring; Infrastructure check and a fresh embedded-PostgreSQL
  readiness test pass. This remains an internal dirty W8 boundary until Lifecycle integration is
  accepted and the combined change is committed.
- W8 integrated the checked Lifecycle owner/fix sequence and committed clean internal boundary
  `d13e923cedeee6fc3f276ae01cdcd1f22f5930e3`. LifecycleRun、SubjectExecution and
  ProjectActiveAgents now use one Application query; `lifecycle_views` and `story_runs` are restored
  as real mounted routes; canonical Rust/TypeScript and Lifecycle/Task/Story consumers preserve
  complete attempts and stale fence evidence; the old Lifecycle read-model port and RuntimeSession
  projection shape are deleted. Lifecycle query 7/7、frontend focused 5/5、Lifecycle/AgentRun/API
  checks、contracts check、Function crash-window、real PostgreSQL replay/conflict and migration
  guard pass with the lock restored. Full frontend typecheck now isolates only Product-owned
  bigint/thread-name consumer drift.
- Two coarse isolated components now run from `d13e923c`. Product/Protocol owns final Product
  consumers in `F:\Projects\AgentDash-s5-product-final-consumers`: canonical bigint sequences,
  thread-name fixtures, AgentRun Workspace, Product/API/frontend RuntimeThread naming and Product
  DTO ownership. W8's `platform_tool_surface` grandchild owns
  `F:\Projects\AgentDash-s5-platform-tool-surface`: Application VFS/Task tools, Platform SPI Agent
  surface removal and the production `CompleteAgentCallbackBroker -> Runtime Tool Broker` handler
  path. Neither component may touch the other's files、Cargo.lock、0084 or root workspace, and each
  requires independent review before W8 integration.
- The Platform Tool component established the final owner chain instead of moving the old
  `AgentTool` facade: Managed Runtime owns typed catalog/policy/authorization/executor contracts;
  Host remains the sole durable callback generation/idempotency/replay owner; Infrastructure
  resolves Product evidence and adapts `CompleteAgentToolHandler`; VFS/Task provide concrete
  executors. The initial catalog self-authorization was rejected as tautological and replaced by a
  Runtime authorization port over Host-resolved context plus typed Product grant evidence.
  Required/unknown/no-grant broker tests 3/3 and Infrastructure/Host/API checks pass.
- A real cross-owner gap now blocks the non-empty production VFS catalog. Final Product binding
  proves Runtime thread/source but carries no applied Project/VFS grants, while the only historical
  VFS resolver depends on a deleted legacy AgentRun surface port. Restoring that port is forbidden.
  Product/Protocol therefore owns a new dependency-clean
  `AgentRunAppliedResourceSurfaceQueryPort`: `AgentRunTarget` to project/workspace/VFS grants,
  permissions, revision/digest/provenance/currentness. AgentRun Workspace and Infrastructure Tool
  authorization will consume the same Product fact; missing/stale/no-grant is a typed deny.
  Platform continues independent broker/Host behavior and waits for the checked Product seam before
  composing real VFS executors.
- Product's applied-resource boundary now uses dependency-light AgentRun-owned mount/grant/path
  vocabulary rather than an AgentRun → Application VFS dependency. Its immutable snapshot、
  materialize-before-activation、CAS and crash/replay behavior remain under owner implementation.
  Exact replay must compare complete immutable evidence rather than trust caller-supplied digest
  strings, and revisions must reject overflow. The checked component must hand W8 a mechanical
  PostgreSQL producer/repository contract; an uncomposed read port is not completion.
- The lean final AgentRun Workspace query exposed a real Product command-path break rather than a
  removable DTO. Managed Runtime snapshot/change routes are mounted, but existing frontend
  submit、cancel、compact and Product mailbox calls currently have no mounted API write handlers;
  setting the workspace conversation/mailbox to `undefined` would merely hide the resulting 404s.
  Product/Protocol therefore also owns the final guarded AgentRun command facade、mounted API、
  Managed Runtime availability consumer and independent Product mailbox contract/behavior in the
  same component. W8 owns their PostgreSQL repositories and production composition after the
  component passes independent review. No old RuntimeSession conversation DTO is restored.
- Platform Tool initially removed unmounted legacy tool sources without complete replacement
  evidence; every such deletion was restored before commit. The final component must use a
  non-empty production broker catalog, stateless multi-AgentRun executors and a typed
  runtime-neutral authorization grant carrying the applied resource surface、operations、path
  scopes and provenance. A construction-time fixed VFS、empty AppState catalog or string-only
  scope/evidence is not an activatable boundary. Each later legacy deletion requires either a
  final executor with behavior parity or exact zero-consumer/product-path evidence.
- Canonical Runtime/Service/Wire generation still maps Rust `u64` revisions、change/source
  sequences、deadlines and timestamps to TypeScript `number` and incorrectly calls that
  JSON-safe. S5 now treats lossless integer wire semantics as a contract blocker. The preferred
  owner fix is canonical unsigned-decimal strings with Rust `u64` internals、branded generated
  wire types and one frontend bigint decoder; any retained number form would instead require an
  enforced `Number.MAX_SAFE_INTEGER` bound in Rust serde、database constraints and rejection
  tests. TypeScript declarations alone are not evidence.
- The AppliedResourceSurface independent check returned `fixed-and-pass` at
  `17e014d10d288b1e69af31384433647fff51f185`. Canonical VFS segment/path matching now rejects
  traversal and prefix expansion; the Product query returns the complete immutable snapshot with
  an expected-revision fence; missing data is `SurfaceNotApplied`; persisted values match the
  signed PostgreSQL bigint range; persistent DTOs have typed serde; and the fixture repository is
  test-only. The handoff uses primary-key full-row equality plus current-pointer CAS rather than an
  incomplete digest-subset unique key. Focused 21/21、AgentRun 98/98 + doc 2/2 and crate check
  pass with a clean lock. W8 still owns the PostgreSQL adapter、0084、materialize-before-activation
  composition and the remaining indirect AgentRun dependency cleanup.
- The checked AppliedResourceSurface sequence is integrated after canonical-u64 in the S5 staging
  branch at clean tip `e99b3022`. This freezes the Product immutable grant/snapshot evidence before
  Platform Tool integration; its PostgreSQL current-pointer CAS、activation pin and production
  composition remain W8 work and have not been fabricated in the domain component.
- Product integrated that checked surface fix as `afe2a7cb` while preserving its final-consumer
  work. Main review rejected the draft mailbox API as a stable boundary: `client_command_id` was
  not a durable replay/conflict identity, deletion checked the target after mutation, and a
  max-row timestamp could not provide monotonic mailbox revision/change/gap semantics. The Product
  owner is moving command/snapshot/change behavior back to an Application-owned durable mailbox
  boundary; the UI may consume a separate Product mailbox feed but must keep Runtime control state
  on the single live Runtime feed.
- The Platform Tool component resumed from its preserved worktree after the checked Product
  surface seam became available. It must consume the full typed snapshot, activate a non-empty
  production catalog, map stateless multi-AgentRun VFS/Task grants, and prove pre-execution deny,
  Host callback replay/generation/deadline fences and no cross-target leakage before independent
  review.
- Product froze the Application-owned Runtime command and mailbox contracts at internal commit
  `a5a81f5873693737edcd377b1db921275eab11de`, now under independent check. Runtime commands load
  a previously claimed full envelope before consulting the latest snapshot, so response loss and
  active-turn/revision changes replay the same Runtime operation; a different request digest is a
  conflict. Product mailbox exposes one atomic command UoW and one transactionally consistent
  snapshot/change/gap read boundary. Focused Product tests 76/76 and the AgentRun check pass.
  Product removed all attempted Infrastructure/migration ownership; W8 receives only the
  mechanical `research/product-final-consumer-pg-handoff.md` contract and still owns `0084`, PG
  adapters and production composition.
- The Platform Tool target component is frozen clean at
  `42e61eeb46f6188bb17107c687b591eedb75b475` and is under independent check. It keeps Runtime
  catalog/policy/authorization/executor, Host callback handling, typed Infrastructure
  authorization and stateless VFS/Task executors while leaving all PG/migration/composition/API
  activation to W8. Its committed activation seam pins Product resource snapshot revision and
  Host binding generation alongside the Product binding digest, preventing an old callback from
  reading a newer expanded grant. Broker 5/5、VFS 2/2、Task 2/2、Infrastructure authorization
  4/4 and Host replay/fence tests pass; five affected crate checks and Runtime/Host strict clippy
  pass.
- Canonical Runtime/Service/Wire u64 checkpoint 1 is committed clean at
  `95da7696d62cab773ba9102c87507427b92765ad`. Public Rust semantic `u64` values now serialize as
  canonical unsigned-decimal strings with branded raw TypeScript/schema forms, and the Managed
  Runtime frontend uses explicit root decoders plus bigint fold/cursor/URL encoding. Three Rust
  crate suites、three generator freshness checks and 41 focused frontend tests pass. The owner is
  continuing Remote frame allocation exhaustion and final Runtime/Host/Callback PostgreSQL
  `NUMERIC(20,0)` persistence before independent review.
- The Product Runtime command/mailbox checkpoint `a5a81f58` failed independent review with
  `needs-owner-fix`. Stable Runtime command identity and response-loss replay are directionally
  correct, but the new repository ports still classify business outcomes by string prefixes;
  mailbox snapshot/change lacks canonical digest and commit evidence; target/binding fences are
  incomplete; and the fixture does not exercise real mutation、receipt、head、change atomicity,
  rollback、external-row reconcile、strict paging/gap or deterministic digest ordering. The
  Product owner must replace these with typed finite errors and behavior-level conformance before
  W8 implements PostgreSQL. The checker ran AgentRun 105/105 and left its exact-tip worktree clean.
- The Product owner fix is committed at
  `b6c26ba170f9e2ea3ef3da60c68f8b1b29aa9f57` and is under exact-tip recheck. Repository outcomes
  are typed; command availability retains kind/reason and a snapshot-revision fence; mailbox
  snapshot/change/receipt carry one canonical digest and typed commit evidence; binding/data/
  anchor target fences run before mutation. The replacement fixture executes real
  Promote/Delete/Move/Resume UoW behavior、rollback、replay/conflict、external reconcile、
  sequence/paging/gap/retention and deterministic priority/order/ID plus canonical JSON digest.
  AgentRun 113/113 and check pass. W8 still owns the PostgreSQL implementation、0084 and production
  composition after the original checker signs the fix.
- The exact `b6c26ba` recheck closed the original seven findings but returned
  `needs-owner-fix` for three bounded contract inconsistencies: change pages did not reject
  revision regression or prove strict gap evidence; typed claim storage failure was mislabeled as
  a binding failure; and replay mutated a supposedly durable terminal receipt's `duplicate` bit.
  Main authorized the checker to self-fix these locally by separating immutable durable receipt
  from response-level replay metadata, tightening continuity/gap validation and preserving typed
  storage errors. The same bounded fix also removes redundant mailbox `command_kind` state in
  favor of `command.kind()`.
- The Product command/mailbox checker completed the bounded fix and returned `fixed-and-pass` at
  `f67a5ec613c4af86f3011e6400790f9c41f25e01`. Change pages now carry one same-transaction head/
  commit boundary and reject revision regression、future cursor、false/incomplete gap and head
  mismatch; claim storage keeps a typed persistence source; immutable durable receipt is replayed
  verbatim while outer outcome reports `replayed`; command kind has one source. Focused command
  8/8、mailbox 12/12、AgentRun 118/118 + docs 2/2 and all-target check pass. The reviewed three
  Product commits are integrated into S5 staging at clean tip `63a5c902`; W8 now has a mechanical
  PG/UoW contract but no production adapter has been invented yet.
- W8's PostgreSQL implementation exposed one additional Product contract gap before commit:
  moving a mailbox message relative to an anchor in a different priority lane is a typed invalid
  command, but the frozen repository error vocabulary only offered `Storage`. Main rejected
  classifying domain validation as persistence failure. W8 continues the remaining PG slice while
  this minimal typed invalid-move outcome is routed back to the Product owner, then adapts the
  final contract.
- The bounded Product fix passed at `018aa31632197b0518391bbf4c167e231d9475f6` with typed
  `InvalidMove(SelfAnchor | CrossPriorityLane)` carrying target/message/anchor evidence and
  rejecting before every mutation/head/change/receipt fact. Exact replay and digest conflict keep
  precedence. Focused 15/15、AgentRun 121/121 + docs 2/2 and all-target check pass. Main integrated
  it into W8 staging as `fbce90eb`; W8 resumed and is replacing the temporary branch with typed
  errors plus real PostgreSQL zero-mutation assertions.
- Before that pause, W8's isolated embedded PostgreSQL
  `final_product_persistence_contract_runs_on_real_postgres` tracer passed the complete migration:
  surface concurrent replay/conflict/current CAS and overflow、max-u64 command claim restart、
  mailbox four commands/terminal replay/rollback/external reconcile/partial paging/retention gap/
  revision regression and max cursor. The final W8 commit still waits for the typed Move adaptation
  and post-fix rerun.
- The Platform Tool independent check confirmed an owner-level functional blocker at
  `42e61eeb`: the final broker catalog only carries `mounts_list`、`task_read` and `task_write`,
  while the active Product VFS surface also exposes `fs_read`、`fs_glob`、`fs_grep`、
  `fs_apply_patch` and `shell_exec`. Hard-cutting the current component would silently remove
  Read/Search/Write/Execute behavior, and path/operation grants would have no real executor
  consumer. The final `NEEDS-OWNER-FIX` verdict also rejects the new
  Application/Application-VFS → Runtime concrete/Service API dependencies, proves that
  Task-scoped grants cannot constrain sibling tasks, and requires complete VFS/Task
  revision/digest/provenance audit evidence. Runtime broker 5、Host callback 8 and VFS target 2
  tests passed; the checked worktree is clean. A Platform owner fix now owns the full VFS catalog,
  dependency inversion, concrete Task scope and evidence path; W8 remains only the later
  persistence/composition owner.
- The Platform owner replacement is committed clean at
  `0b55f92aaa31b22f6b702cf0f3af1a392017195a`. Runtime/Infrastructure now expose the real
  eight-tool catalog; Application and VFS no longer depend on Runtime concrete or Service API;
  typed Project/Task scope, full Agent/VFS/Task evidence and pinned snapshot/generation are
  enforced. Runtime broker 5、Host callback 8、Infrastructure 8、VFS runtime 3 + fs 50 and Task 3
  tests pass. Main did not send this tip to independent check yet: its VFS target service still
  invoked the legacy `DynAgentTool` facade internally, which would block the required
  `agent-types`/Platform SPI AgentTool deletion. The owner is now extracting a VFS-owned direct
  typed execution service while retaining identical overlay/materialization/terminal behavior;
  old wrappers must become exact zero-consumer deletion inputs, not a hidden target dependency.
- The canonical public-u64 component is complete and clean at
  `593a1af72c06010708f77a98ab694d94164c8a26` with commit range
  `d13e923c..593a1af7`. Runtime Contract、Service API and Runtime Wire use owner-branded canonical
  decimal strings with explicit bigint codecs; Remote frame allocation has typed exhaustion; and
  final Runtime/Host/Callback PostgreSQL coordinates use constrained `NUMERIC(20,0)`. Real
  embedded PostgreSQL proves max-u64 load/CAS and overflow rejection; contracts、migration、
  Remote and focused frontend gates pass. An independent checker is now validating the exact tip.
  Product-owned scalar migration/decoder/fixtures remain a separate Product responsibility.
- The canonical-u64 checker found one bounded codec-coverage blocker at `593a1af7`: Runtime raw
  brands are generated for command expected revision、operation accepted revision、changes
  request cursor and conflict actual revision, but the frontend owner closure only exported
  snapshot/change codecs. One interaction response still cast a raw receipt directly, leaving a
  branded decimal string where the semantic frontend contract requires bigint. Main authorized
  the checker to self-fix the complete mechanical Runtime/Service/Wire root inventory with explicit
  owner codecs and max/illegal-root tests; generic field-name walkers and opaque JSON mutation
  remain forbidden.
- The canonical-u64 independent check returned `fixed-and-pass` at
  `187c78114f922f60ac8bc302d9ef9b0fb03eed89`. It closes every public Runtime root codec and
  interaction receipt, and changes all three owner schemas/codecs to an exact
  `0..=18446744073709551615` language verified by 100,000 randomized differential cases.
  Runtime/Service/Wire/Remote 67 tests、frontend 57、real PostgreSQL max/CAS、three generator
  freshness gates and migration guard pass. The reviewed five-commit sequence is integrated into
  the S5 staging branch at clean tip `d095a91c`; `Cargo.lock` remains frozen. Full frontend
  typecheck now exposes only Product-owned bigint consumers and fixtures.
- A read-only canonical-presentation/protocol deletion audit is captured in
  `research/final-protocol-types-codegen-cutover.md`. It confirms the current Service/Runtime item
  body is not an activation target for the old presentation consumers: most Codex item families
  are reduced to type/status Extension evidence, the frontend rebuilds generic JSON cards, and
  failed/interrupted/lost compaction can be misclassified as completed. The next owner bundle must
  first freeze complete platform-neutral body、typed update、terminal and interaction evidence,
  then migrate Codex private codegen/projector、Native/Remote、Runtime persistence/change and
  Product/UI consumers. The same audit found Relay has no production `RuntimeWirePlacement`;
  typed open/frame/ack/closed/offer placement must exist before old Prompt/SessionEvent deletion,
  otherwise Remote Complete Agent would lose its production transport.
- The first canonical-presentation activation component now runs in
  `F:\Projects\AgentDash-s5-presentation-protocol` from checked staging base `63a5c902`. Its coarse
  owner covers complete Service/Runtime body、update、terminal and interaction vocabulary plus
  Codex private codegen/projector、Native projector and Remote/Wire conformance. It does not touch
  Runtime PostgreSQL/Product frontend、Platform Tool/SPI、Relay production placement、root
  scripts/lock or legacy deletion. Those consumers only activate after this source contract passes
  independent review.
- The Platform Tool owner completed the direct-execution replacement at
  `409ba778600b4389ec8dd3ab49adae436e7f9358` on top of `0b55f92a`. The target lane now exposes
  the exact eight-tool catalog and VFS-owned typed executors without constructing、holding or
  calling `DynAgentTool` / `AgentTool`; current-lane wrappers remain only as an exact W8 deletion
  input. Application-VFS runtime 5、filesystem 52、direct shell/terminal/PTY 13 and Infrastructure
  authorization 8 tests pass, both target negative scans are zero and the lock is restored. An
  independent checker is reviewing the full `42e61eeb + 0b55f92a + 409ba778` component before W8
  integration; no production composition or legacy deletion has been activated yet.
- W8 completed and committed the final Product PostgreSQL/`0084` persistence slice at
  `d4a979f1c71834a20599792d40c2a3933ad47d1b`. The sole migration now covers immutable
  `AppliedResourceSurface` snapshot/current state、full resolved Runtime command claims and the
  Product mailbox head/change/terminal-receipt UoW. A real embedded PostgreSQL contract proves
  concurrent surface replay/conflict/current CAS、signed/max integer fences、restart command
  claim、all four mailbox commands、typed self/cross-lane invalid move with zero mutation、
  transaction rollback、external/concurrent reconcile、strict paging/gap/retention and max cursor.
  Product focused suites pass 21/21、8/8、15/15 and Infrastructure check/migration guard pass with
  a clean lock. This is an internal S5 staging boundary only: Platform Tool production composition
  and Product production caller activation remain required before the S5 architecture/behavior
  checks and stable checkpoint.
- The independent Product PG check returned `pass` on exact tip `d4a979f1` without fixes.
  Embedded PostgreSQL 1/1、Product command 8/8、mailbox 15/15、AppliedResourceSurface 21/21、
  migration-focused 1/1、both affected all-target checks、owner Clippy、migration guard and lock
  cleanliness pass. Full dependency Clippy remains blocked only by two pre-existing
  `agentdash-agent-protocol` lints in the separately owned legacy-deletion lane.
- The independent Platform Tool check returned `needs-owner-fix` after a bounded clean checker
  fix at `ed0a0190960560bb19c673e94e9f1ceb964d6159`. The fix derives shell capabilities from each
  invocation's frozen applied VFS surface、preserves Product typed deny codes and removes the last
  zero-consumer legacy VFS helper/dependency omission. Runtime broker 5/5、Host callback 8/8、
  Infrastructure tool 8/8、Application-VFS 160/160、direct VFS 6/6 and Task scope 3/3 pass.
  Two owner blockers remain before integration: seven non-empty tool parameter schemas are reduced
  to field names with empty `{}` values instead of the real parser contract, and Host callback
  deadlines are checked only before invocation so a late handler can still settle success.
  The original Platform owner is replacing the schema source with a dependency-light canonical
  parser/schema boundary and making post-reservation deadline expiry enter typed
  inspection-required/unknown recovery with zero duplicate execution.
- Canonical presentation P1/P2 are committed clean as `ac615732` + `0e5b18ad` from base
  `63a5c902`. The component contains independent Service/Runtime canonical bodies、typed
  transitions/terminal/interaction evidence、explicit Runtime projection、Codex-private
  generator/generated/fixtures/lock、typed Codex and Native projectors and Remote/Wire roundtrip.
  Owner gates pass across Service、Runtime Contract、Runtime、Codex、Native、Wire、Remote、
  generator freshness and strict owner Clippy; an independent checker is reviewing exact range
  `63a5c902..0e5b18ad`. The 33-line
  `research/presentation-protocol-source-handoff.md` is present in `0e5b18ad` and records the new
  private generator source plus root switch. The owner is strengthening it on a separate Relay
  branch with exact commands and consumer/deletion inventory; the checker will judge the original
  component from Git rather than the earlier mistaken filesystem lookup.
- The Platform Tool owner/checker completed the two owner fixes and one bounded checker fix at
  clean tip `ab278b3de935ea1d24907e61546ae1e8bbb532cf`. The final eight-tool catalog now derives
  every parameter schema from the strict serde/JsonSchema parser owner; Host uses a durable
  invocation reservation and absolute deadline so timeout or late success enters
  `InspectionRequired`; per-invocation VFS capability、typed Product deny and concrete Task scope
  are preserved. Independent result is `fixed-and-pass`: Host callbacks 11/11、Runtime broker
  5/5、Infrastructure runtime tools 9/9、Application VFS 166/166 and Task tools 4/4 pass.
- Main integrated the reviewed Platform Tool commit range into S5 staging without conflict as
  `d8ad6b08..15b1b1ed`, then removed the W8-manifested test-only
  `crates/agentdash-application/tests/runtime_tool_catalog.rs` at `84caddb4`. That file was the
  remaining compiler consumer of the deleted 17-tool `RuntimeToolProvider`/Backbone path; final
  eight-tool catalog、schema、authorization、VFS/Task execution and Host replay remain covered by
  their owner suites. `cargo check -p agentdash-application --tests` and the Infrastructure final
  catalog 2/2 pass with the frozen lock restored. `84caddb4` is an internal S5 boundary, not the
  stable checkpoint.
- The first presentation checker findings remain owner-level: Runtime must project real typed
  `ItemTransitioned` deltas and prove source-change/snapshot fold parity; Codex transport must
  strict-decode private typed notifications and server requests rather than keep method + opaque
  `Value`. The source owner is fixing both on the presentation branch. A Product cutover audit
  additionally found that presentation digests must recursively canonicalize JSON object key
  order before hashing, and generated Runtime validators must recursively decode/encode and
  discriminant-check presentation/item-transition `u64` fields. Both blockers are assigned to the
  same P1 contract owner before P3 activation.
- A new read-only current-tip audit is running from staging `84caddb4` to produce the exact
  remaining P4/P5 protocol/types/codegen、Platform SPI、Relay、composition、schema and workspace
  deletion order. It must distinguish real Product paths from zero-consumer leftovers and cannot
  authorize deletion solely from module-tree absence.
- The Product presentation cutover audit completed with no user decision required. P3 must fold
  `ManagedRuntimeItem`/typed interaction directly into the Product history UI, handle
  `thread_name_changed` and `item_transitioned`, preserve all four terminal outcomes and stop
  rebuilding `BackboneEvent`/`AgentDashThreadItem` or generic dynamic-tool JSON. Workspace、
  Terminal、Canvas and Product mailbox remain independent Product feeds.
- P3 also owns the final typed Runtime command route and caller cut: interaction responses use
  `interaction_id + client_command_id + expected_revision`; legacy item-ID approval and separate
  context-compact paths are deleted after caller zero. AppState must compose the existing durable
  Product Runtime command/mailbox facades. Lifecycle VFS `session_records` remains a legal
  history-derived read model only after its `PersistedSessionEvent`/journal/compaction-archive
  inputs are replaced by canonical Runtime history.
- W8 retains only PG/composition/generation/deletion for this slice: canonical items/interactions
  stay nested JSONB under the Runtime fact owner; load/commit recursively validate digest、
  status-terminal、interaction、identity and revision evidence; domain canonical SHA-256 is used
  instead of `md5(jsonb::text)`; same-transaction failpoints must prove facts/projection/change/
  outbox cannot partially commit. Product P3 implementation waits for the canonical presentation
  source contract to pass independent recheck.
- The canonical presentation owner closed all prior source findings at clean tip
  `9d56eb27a8add1d5bfb98f082d8104d8fdffb096`. Runtime now emits an independent typed
  `ItemTransitioned` delta for all nine updates and four terminals through commit/outbox/snapshot
  fold; Codex transport admits private generated notifications and six typed server-request
  families and breaks on unknown/invalid frames; Service/Runtime digests use recursively key-sorted
  canonical JSON SHA-256; generated Runtime validators recursively decode/encode presentation
  `u64` and validate body/update/transition discriminants. Owner checks passed across four Rust
  crates、Runtime 48/48、Codex 13 + 21、generators、Clippy、single-file TypeScript、ESLint and
  codec 21/21. The original independent presentation checker is rechecking this exact clean tip
  before staging integration.
- The original presentation checker returned `fixed-and-pass` at
  `831ccac8f556750c3f275e17cafadb2d8e89f169`. Its bounded fix makes Runtime validators
  explicitly reject unknown terminal/content/plan/search/file-change/output-stream
  discriminants and adds ItemTransitioned change/outbox parity. Seven-crate tests、Runtime 48、
  Contract 18、generator freshness、affected Clippy、focused TypeScript/ESLint and codecs 27/27
  pass. The complete reviewed source chain is integrated into S5 staging without conflict at
  clean internal tip `f5fa039ffd1677a073c8a3e97387a854ae56446e`.
- The current-tip P4/P5 audit confirmed `RuntimeJournalFact` is already zero-match and 0084 plus
  migration readiness gates already retire the old journal/session tables. P5 remains blocked by
  real activation gaps instead: the final eight-tool broker/catalog/authorizer/VFS/Task services
  have no production composition; their Product binding query lacks a PostgreSQL implementation
  and persisted applied-resource/binding fences; first-party production registration still omits
  Native Complete Agent; and Remote has no API/Local/Relay `RuntimeWirePlacement`. Deletion must
  wait for these production paths, not for more journal renaming.
- Platform Tool/Hook production activation now runs in isolated worktree
  `F:\Projects\AgentDash-s5-platform-production` on branch
  `codex/agent-runtime-s5-platform-production` from clean base `84caddb4`. Its coarse ownership
  includes VFS/Task final service separation、Product binding/resource/generation PostgreSQL pins、
  `RuntimeToolProductBindingQueryPort`、final catalog/authorizer/Broker/Host handler and real
  AppState Tool/Hook callback composition. It temporarily owns 0084 and AppState only in that
  branch, must restore `Cargo.lock`, and may not touch Product P3、Native registration、Relay or
  canonical presentation. Independent review is required before staging integration.
- Product P3 now runs from checked source tip `f5fa039f` in
  `F:\Projects\AgentDash-s5-product-presentation` on branch
  `codex/agent-runtime-s5-product-presentation`. It owns the Rust/API/Lifecycle/frontend canonical
  history consumer cut described in `product-canonical-presentation-cutover.md`, but not PG、
  AppState、Platform、Relay or final lock. Its handoff must leave Workspace/Terminal/Canvas/mailbox
  as independent feeds and prove all presentation/interaction/terminal/bigint tracers.
- Relay production activation now runs from the same checked source tip in
  `F:\Projects\AgentDash-s5-relay-production` on branch
  `codex/agent-runtime-s5-relay-production`. It owns Wire placement vocabulary、Cloud/Local real
  WebSocket routing、Host independent verification/dynamic registration、Remote bidirectional
  service/callback transport and bounded queues. Old Prompt/SessionEvent variants may be deleted
  only after real ws_handler/ws_client/Host PG tracers and non-Agent lane regressions pass.
- User progress audit paused all three active implement agents before further mutation. The audit
  found 43 registered worktrees while the stable branch had not received a checkpoint since
  `9c86876e`, despite the clean S5 staging branch containing 24 integrated internal commits through
  `f5fa039f`. Main must now commit this task-local progress record, remove only clean handed-off
  worktrees while preserving branches/commits, keep every dirty worktree untouched, and resume with
  shorter visible checkpoint intervals. `f5fa039f` remains an internal staging tip rather than a
  false S5 stable checkpoint because production composition is not yet complete.

## 2026-07-19 Final convergence closeout

- 当前分支已统一回到单目录、单分支执行；本轮未创建 worktree。
- canonical protocol、presentation carrier、Lifecycle canonical history provider 与 VFS
  surface 已形成 `3eb78e80`、`6e05a0f5`、`cd775331`、`e176ae10`、`be874b73`
  checkpoints。
- 当前 S4 的剩余范围已收敛为 Companion、Routine、Workspace/Canvas/Terminal、
  Lifecycle VFS、Wait、Capability 与 canonical UI 的 production caller/tracer。
- 已清理未提交的 Companion PG repository / graph helper 实验改动，工作树恢复干净。
- Product 控制面 oracle 修正为 `58c537b7`（`c3cc58b9^`）。`c3cc58b9` 已经切断
  AgentRun exports，不能作为完整 composition baseline。后续先从 oracle 恢复 Product 源码、routes、
  composition 与 tests，再只在旧依赖点机械适配 Runtime Contract、Tool Broker、
  Product repositories、AppliedResourceSurface 与 canonical projection。
- `final-convergence-closeout.md` 已固定 C0–C6 全分支收尾路径；
  `hard-cut-final-checklist.md` 已对齐 Product parity 与 replacement evidence。
- 当前真实阶段：C0 完成，下一步进入 C1 Product Integrity；正式 deletion manifest 在
  C1–C4 tracer 闭合后形成。

## 2026-07-19 Product 控制面恢复进度

- 删除链审计确认完整 Product 控制面最后基线为 `58c537b7`。`c3cc58b9` 先切断
  AgentRun exports，`8d22d6cf` 再移除 Application Product modules，`a535ae01` 再移除
  Router/AppState production composition，后续物理删除不能作为零消费者证据。
- `93f1181f` 已建立 Product-owned `AgentRunProductRuntimeProvisioningPort`：Application
  只提交 AgentRun target、Runtime thread、AgentFrame/执行配置引用与 surface facts；
  Complete Agent selection、Host target registration、surface admission 与 source binding
  留在 final Runtime/Host owner。
- `9eaf25a8` 已固定 Application/Product 不属于 Hard Cut。Companion、Frame、Routine、
  Workflow、Workspace、Canvas、Terminal、Wait、Lifecycle 只迁移 Runtime 接入 seam，
  业务规则、route、worker、权限、gate、mailbox 与用户可见行为必须保持。
- 当前 C1/C2 并行进行：
  - Product owner 以 `58c537b7` 恢复 Companion/Frame/Routine、Project Agent start、
    routes、AppState 与行为测试，并把旧 seam 映射到 final owners；
  - Complete Agent owner闭合 Dash/Codex/Remote registration、Host target provisioning 与
    Runtime create path；
  - Read-model owner闭合 Lifecycle AppliedResourceSurface materialization、canonical
    history 与 AgentRun workspace consumer。
- C2 审计确认 MCP 必须进入 final Runtime Tool Broker：Surface compiler 与 Broker 共享
  typed dynamic catalog，Host callback 执行同一 catalog 中的 MCP tool；server metadata
  不能伪装为 context。Complete Agent owner已直接吸收该纵向闭环。
- C2 审计确认 Product Hook plan compiler/policy handler当前没有production composition；
  `EmptyPlanCompiler`与无条件Allow不构成能力证据。Product owner负责恢复真实Product
  policy/effects，Complete Agent只映射明确属于Agent callback/native的hook site。
- 下一 checkpoint 只在 Product module/router/composition 进入真实构建图，且
  Project Agent create → provisioning → Managed Runtime → Complete Agent 的 focused tracer
  成立后形成。

## 2026-07-20 Runtime-only final convergence

- 最终范围已在 `848df4d5` 收窄：本任务只重构 Agent Runtime 内核、Complete Agent
  接入及其 final seam。Application/Product 只恢复既有业务并适配 seam；其 module、
  route、worker、权限、Companion/Frame/Workspace 等业务不进入 Hard Cut。
- `2e653ab1`、`5bbfcbd2` 已闭合 RuntimeThread Product read model、Lifecycle canonical
  history/VFS 与单 outbox 多观察者；Lifecycle library tests 16/16 通过。
- `3849f2f2` 已接入 Product Hook 与动态工具回调；`6991f79a` 已闭合显式 Rebind、
  trusted replacement selection、Host generation recovery 与 stale effect fencing。
- `ad05facf` 已恢复 Product Application/API production composition：Canvas、Companion
  Gate、Extension、Workspace Module、Routine、Terminal、AgentRun workspace、Hook、
  Surface、launch/input 与 scheduler 均进入真实构建图；API library check 通过。
- `0230fe27` 与 `a592c63b` 已闭合 Remote replacement → Lost RuntimeThread →
  Product recovery observer 链，并冻结首次恢复的线程集合以保证 admission 重放不会在
  Host generation 前移后丢失 Product CAS/Activate。API admission 5/5、Host tracer
  1/1 通过。
- `a4297f37` 已闭合 Product Rebind durable recovery：PostgreSQL recoverable scan、
  AppState background worker 与四个崩溃点的 phase-aware replay 已进入 production；
  Rebind replay tests 4/4 通过。
- `246ba75b` 已同步 `conversation_history + Rebind` canonical generated fixture/schema；
  exact generator test 与 migration guard 通过。
- 当前唯一在制 Product seam 是 Companion production caller：
  - Full 使用最近 completed turn 的 exact cutoff，进入唯一 durable fork saga；
  - fresh 使用预分配 child，按 typed InitialContextPackage → Activate → 独立 first input
    推进 durable saga；
  - 成功后沿用既有 channel/gate/adoption/result/mailbox 业务，不重复实现 Product
    delivery。
- 当前阶段仍是 C2/C3，不进入 C5。Companion caller、Routine/Workflow、Workspace/
  Canvas/Terminal、Wait、Tool/Hook 与 canonical UI 的 production tracers 闭合后，
  才冻结旧 Runtime replacement manifest 并执行最终 Runtime-only deletion。
