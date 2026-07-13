# W0 Main Oracle 与行为账本

## 可执行资产

- Oracle preflight：`scripts/session-parity/Test-SessionParityOracle.ps1`。
- 固定来源与hash：`scripts/session-parity/oracle-manifest.json`。
- strict normalizer/comparator：`crates/agentdash-agent-runtime-test-support/src/session_parity.rs`。
- 机器可读variant/owner/fixture账本：`fixtures/session-parity/inventory.json`。
- Required fixture matrix：`fixtures/session-parity/scenario-catalog.json`。
- 首个固定golden：`fixtures/session-parity/main/user-submit.json`。

preflight同时验证Main reference的commit、clean状态、六个关键oracle合同/producer源文件hash、W0 harness/hash，以及current HEAD仍继承task start。reference目录没有写入步骤。

## Strict comparator合同

normalizer只有两个互斥入口：

1. Main `SessionEventResponse.notification.event`；
2. current immutable carrier `presentation_event`。

它们只接受显式列出并启用`deny_unknown_fields`的typed transport wrapper，随后将wrapper一次性丢弃，protected event body直接移入`NormalizedPresentationEvent`。未知wrapper字段会硬失败；body内部即使使用与wrapper同名的字段也不会被删除。不存在递归字段删除、payload ignore list、默认值补齐、重排或类型转换。比较顺序固定为event count、逐项durability、逐项完整JSON body；因此ID、timestamp、null/omitted、number/string和数组顺序都会触发失败。

`connected`与`heartbeat`在typed Main NDJSON parser中进入独立control channel，不伪装成presentation；其可观察时序仍由`journal_fork_heartbeat_lagged_closed`场景验收。

## BackboneEvent inventory

Main `BackboneEvent`的21个variant全部进入机器账本：message/reasoning三类delta、item start/update/completed、command/file/MCP delta、turn start/completed/diff、user input、plan update/delta、usage、thread status、executor compaction、approval、error与Platform。每行均记录Main producer owner、W4–W7 current owner及scenario fixture id。

当前分支W0开始时的generated/owned event enum缺少Main的`turn_started`、`turn_completed`、`thread_status_changed`与`executor_context_compacted`，因此这些行明确归入W4/W5/W7恢复，不能因current enum不存在而标记non-production。

## PlatformEvent inventory

Main的10个variant全部进入机器账本：executor binding、source title、hook trace、session meta、provider attempt、runtime terminal diagnostic、session rewind、control-plane projection、terminal output和PTY lifecycle。

当前分支W0开始时缺少provider attempt、runtime terminal diagnostic与session rewind；这些是Main有生产owner的事件，不是non-production，分别归W5/W7恢复。其余variant也必须由W7逐个证明producer，而不是只证明类型存在。

## Driver、Tool 与 application producer账本

- Driver：Codex、Native、Remote/Relay均有owner和required fixture集合；future/enterprise contribution必须在admission时加入同一inventory，不能继承“已覆盖”结论。
- Tool：command/shell、file/apply patch、MCP/dynamic、fs read/grep/glob、workspace/canvas/VFS、companion/task/wait、terminal/control均映射到fixture id。W6必须从最终catalog动态展开具体contribution，family行只定义最低覆盖矩阵，不替代最终逐tool等式。
- Application：user prompt/steer/modalities、system/workflow/companion delivery、turn terminal/rewind、title/status、hook/context/terminal/control、fork/mailbox/round/lineage均有owner和fixture id。

完成等式仍是：driver contribution count = driver conformance count；tool contribution count = projector count = full fixture count；Main production writer count = current owner count = fixture count。W0机器账本整体标为`planning_inventory`，scenario逐行区分`planned`与`golden_verified`；当前只有seed golden可标为`golden_verified`。后续工作项必须在实际golden存在并通过deep parity后再更新状态，不能用planned owner自证实现完成。

## Route / service ledger

| Main observable surface | Main owner | Current work owner | Required scenario |
| --- | --- | --- | --- |
| Session events GET page | AgentRun journal query + API route | W8 | `journal_get_initial_live_reconnect_refresh` |
| Session NDJSON initial/live | AgentRun journal stream + API route | W8 | `journal_get_initial_live_reconnect_refresh` |
| Connected/heartbeat/resume/lagged/closed | NDJSON transport | W8 | `journal_fork_heartbeat_lagged_closed` |
| Fork inherited prefix/marker | AgentRun journal/fork history | W8 | `journal_fork_heartbeat_lagged_closed` |
| Composer submit/steer/cancel/compact | AgentRun control services | W10 | `agentrun_fork_mailbox_context_lineage_status` |
| Interaction/approval response | AgentRun interaction service | W10 | `frontend_tool_progress_interaction` |
| Fork/fork-submit | AgentRun fork service | W10 | `agentrun_fork_mailbox_context_lineage_status` |
| Mailbox waiting/action/recall/resume | AgentRun mailbox services | W10 | `agentrun_fork_mailbox_context_lineage_status` |
| Context projection/compaction | session/AgentRun context services | W10 | `agentrun_fork_mailbox_context_lineage_status` |

W8/W10必须补充每行实际route method/path、application method、generated DTO和contract test，并把差异归为“Main等价”“Runtime internal-only增量”或blocker；缺失生产route不能以internal Runtime endpoint替代。

### W10 AgentRun outer 实际边界

| Observable behavior | Route | Application owner | Generated DTO | Evidence | Classification |
| --- | --- | --- | --- | --- | --- |
| Workspace / control snapshot | `GET /agent-runs/{run_id}/agents/{agent_id}/workspace`, `GET .../runtime/control` | `AgentRunWorkspaceQueryService::resolve` + canonical `AgentRunRuntime::inspect` boundary | `AgentRunWorkspaceView` | `workspace-selection.json`; workspace/frontend focused tests | Main等价 |
| Project run list / opaque cursor / recursive children | `GET /projects/{project_id}/agent-runs` | `ProjectAgentRunListQuery::execute` | `ProjectAgentRunListView` | `agent_run_list` 7 focused tests | Main等价 |
| Composer / cancel / compact | `POST .../composer-submit`, `POST .../cancel`, `POST .../runtime/context/compact` | durable Runtime mailbox + typed Runtime facade；precondition读取同一 workspace snapshot | command/mailbox generated contracts | API route tests + browser scenario | Main等价 |
| Fork / fork-submit | `POST .../fork`, `POST .../fork-submit` | `AgentRunForkCommandService` + typed Runtime fork port | `AgentRunForkResponse` | application/API focused tests + browser scenario | Main等价 |
| Mailbox inspect / actions / recall / resume | `GET .../mailbox`, message content/delete/promote/move routes, `POST .../mailbox/resume` | mailbox repository/service + typed Runtime delivery | mailbox generated contracts | application/API focused tests + browser scenario | Main等价 |
| Tool approval response | tool approval routes | approval item到canonical Runtime interaction的精确解析 | approval contracts | API approval mapping focused tests + browser interaction scenario | Main等价 |
| Typed Runtime interaction response | `POST .../runtime/interactions/{interaction_id}/respond` | canonical Runtime interaction resolver | `InteractionResponse` | current API typed route `respond_agent_run_interaction`；`agentRunRuntime.test.ts` 的 `responds to a typed Runtime interaction` 精确验证编码路径与 nullable denied body；browser interaction scenario | AgentDash-owned additive extension（Codex 0.144 interaction boundary）；pinned Main 没有对应 route，因此不宣称 Main route 等价；该增量不替换或改变任何 Main 可观察 route/path |
| Runtime inspect / internal event stream | `GET .../runtime`, `GET .../runtime/events/stream/ndjson` | canonical Runtime inspect/event port | Agent Runtime generated contracts | Runtime/API focused tests | Runtime internal-only增量 |

Workspace DTO 的业务字段由 application workspace owner 一次投影；API 只做 generated contract 转换与 lineage 装配。这样 Main 的 command snapshot、mailbox、frame/resource surface 与页面行为保持同一 owner，同时 canonical Runtime 只替换旧 session core/control 查询边界。

## Frontend file ledger

W0对Main/current `packages/app-web/src/features/session`进行了只读相对路径与SHA-256盘点：

- Main 105个文件，current 104个文件；
- 96个同路径文件中29个内容不同；
- Main-only：`agentRunJournalIdentity.ts/test`、`sessionNdjsonEnvelopeValidator.ts`、`sessionPlatformEventDispatcher.ts/test`、`streamTransport.ts/test`；
- current-only：`runtimeContextRequest.ts`、`runtimeSessionAdapter.ts/test`、`sessionStreamReducer.runtime.test.ts`、`useSessionStream.test.ts`、`SessionRuntimeInteractionContext.tsx`。

29个同路径差异集中在feed/reducer/types/platform/round actions/companion和renderer视图。W9必须重新生成逐文件hash ledger；只有envelope adapter、generated import及0.144.1 nullable seam可以保留差异。current-only Runtime adapter/renderer不是允许seam的自动证明。

## Browser scenario ledger

| Scenario | Protected/visible assertion | Owner |
| --- | --- | --- |
| submit | 首项UserInputSubmitted，随后TurnStarted；用户角色/内容/source不变 | W7/W9/W10 |
| no phantom tool | 普通User/Assistant MessageStart不出现tool card | W5/W9 |
| refresh | durable message/reasoning/tool terminal重载后类型、ID、时间不变 | W8/W9 |
| tool progress | started/update/delta/terminal与Main card顺序一致 | W4/W5/W6/W9 |
| interaction | request identity、控件与resolution时序一致 | W4/W5/W6/W10 |
| fork | inherited prefix、marker、lineage、submit target一致 | W8/W10 |
| mailbox | waiting/actions/recall/resume及副作用一致 | W10 |
| context | projection、usage、compaction与refresh一致 | W7/W10 |
| status/system | status bar、title、hook/terminal/control side effect一致 | W7/W10 |

## Spec冲突账本

| Spec条款 | 与本任务冲突 | 本任务期间裁决 | W11迁移方向 |
| --- | --- | --- | --- |
| `backbone-protocol.md`将Runtime lifecycle排除出Backbone并要求UI使用canonical Runtime feed | 会使Main `BackboneEvent` presentation family被Runtime event替代 | Main protected body与PRD优先 | 记录immutable presentation fact与Runtime internal fact的同journal分层 |
| `frontend-backend-contracts.md`要求snapshot transcript baseline + Runtime NDJSON feed | 会悬空Main SessionEvent feed/reducer/renderer | 恢复Main session eventstream，Runtime wrapper只做transport | 描述typed wrapper adapter输出Main等价presentation sequence |
| `frontend-backend-contracts.md`要求command availability只读Runtime snapshot | 与Main command snapshot authority/stale guard的产品行为存在冲突 | W10以Main observable command behavior为oracle，同时守住后端唯一authority | 记录最终后端authority与前端snapshot/stale guard，不保留前端推断 |
| `frontend/state-management.md`的Runtime-only session authority条款 | current reducer直接认识RuntimeEvent并重造展示 | W9恢复Main reducer，仅envelope seam可变 | 记录frontend不认识Runtime internal event |

这些冲突不会被实现agent自行扩大成wrapper allowlist；W11只在全链路deep parity后更新最终spec。

## W0验证记录

```powershell
./scripts/session-parity/Test-SessionParityOracle.ps1
cargo fmt -p agentdash-agent-runtime-test-support -- --check
cargo test -p agentdash-agent-runtime-test-support
```

负例覆盖event缺失、增加、重排、ID变化、timestamp变化、number/string变化、null/omitted、数组重排与durability变化。seed golden固定UserInputSubmitted在TurnStarted之前，并由同一typed Main NDJSON normalizer读取。

### W4 Codex conformance 进度

`crates/agentdash-integration-codex/fixtures/main-presentation.json`固定Main oracle commit与Codex `0.144.1` revision；connector测试使用W0 ordered comparator深比较完整protected body与durability。当前Codex notification矩阵、同源item lifecycle/delta identity、server request identity、method-specific automatic response、unsupported diagnostic及nullable/omitted三态均已通过。共享scenario仍保留`planned`，等待W5/W6/W7共同owner与W8端到端stream完成后再晋级。
