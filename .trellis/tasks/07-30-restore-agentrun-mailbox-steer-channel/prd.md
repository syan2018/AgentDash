# 恢复 AgentRun Mailbox、Steer 与 Channel 投递链路

## Goal

在当前 Complete Agent / AgentRuntime 架构下恢复 AgentRun 的统一 durable message intake、Queue/Steer 调度、失败恢复和前端投影，使用户输入、Companion、Channel、Routine、Workflow、Canvas 以及需要 Agent 继续处理的系统输入不再同步直冲 active turn。

用户价值：

- 正常发送与显式 Steer 重新具有不同且可预测的行为。
- Agent 正忙、暂时不可用或进程重启时，已接收输入不会丢失。
- Companion/Channel 返回不会因为硬 Steer 与 Agent owner callback 并发写入而炸掉原回合。
- 前端重新展示 Waiting / Steer / Pending、来源、状态和恢复操作。

## Background / Confirmed Facts

- 当前 `AgentRunProductInputDeliveryService` 是同步 handoff，明确不提供 offline queue；所有来源最终构造成 `AgentRunProductCommand::SubmitInput`。
- `AgentRunProductCommandFacade::map_command` 在 active turn 可 Steer 时把所有 `SubmitInput` 自动改写成 `AgentCommand::Steer`，因此普通 Enter、Companion result、Channel、Routine 和 Workflow 输入都可能成为硬 Steer。
- Companion 当前先构造 `ChannelDeliveryIntent`，随后绕过通用 Channel delivery，直接调用 Product input delivery；成功以后才把 Channel state 记为 `Materialized`。
- `ChannelService::plan_broadcast_deliveries` 没有生产调用者；`channel/agent_run_delivery.rs` 未被 module 声明、引用已删除接口，是不参与编译的孤儿文件。
- 前端 Ctrl/Cmd+Enter 仍计算 `deliveryIntent="steer"`，但 AgentRun 路径固定发送 `{ kind: "submit_input" }`；显式意图被丢弃。
- 当前 running 状态下 Composer 按钮始终显示 Stop，即使输入框已有内容；旧版行为是有内容时显示 Send、无内容时显示 Stop。
- `DeliverAgentRunProductInput.source/origin` 在构造 `AgentCommand` 前被丢弃；Native canonical projection 把所有 `InputAccepted` 硬编码成 `Prompt + core_composer`，Companion/Channel provenance 无法进入权威历史。
- Dash Agent 的 Core durable callbacks 与 `execute_steer` 都执行整份 repository 的 `load -> clone -> compare_and_swap`，但没有共享 owner writer。当前日志中的 `repository state changed` 即由该竞态触发，并把原回合收敛为 `execution_callback_error`。
- Hook 规则、Rhai evaluator 与 Complete Agent callback route 仍存在，但 turn-boundary/durable delivery 已断裂：
  - `AfterTurn` / `BeforeStop` 的 Product requirement 固定包含 `ContinueTurn`、`RefreshSurface`，当前 Complete Agent surface 编译器将这两种 action 判为 unsupported；required rule 会使 provisioning 失败。
  - Native 生产接线没有安装 `RuntimeTurnBoundaryDelegate`，因此 Agent Loop 的 `after_turn` / `before_stop` 默认不执行 Product Hook。
  - 当前 callback 只能返回一个 `AgentHookDecision`；多个同步决策会报 unsupported，`HookRun` / `HookEffect` 持久化表已经 retired。
  - 旧版 terminal auto-resume、Hook turn-start notice 与 mailbox boundary drain 接线已随 RuntimeSession Mailbox adapter 删除。
- 当前规范互相冲突：AgentRuntime/Product 规范明确禁止 Mailbox，而 Channel、Hooks 等规范仍声明 AgentRun Mailbox 拥有 durable intake、claim、launch/steer 和恢复。
- 参考 worktree `D:\ABCTools_Dev\AgentDash-main-reference` 位于 `957fa9d60`（2026-07-09），保留完整 Mailbox domain、Postgres repository、scheduler、API contracts、Companion/Channel 接线和前端 UI。

## Requirements

### R1. Agent owner 写入一致性

- Dash Agent 同一 source 的 command、Core callback、terminal fence、surface 和 effect mutation 必须经过同一个 owner-scoped 串行写入模块。
- PostgreSQL CAS 继续承担 stale owner/fencing 检查，但合法的同 owner Steer 与 callback 并发不能再互相判定为业务冲突。
- 必须覆盖本次日志对应的 Steer/Core callback 竞态回归测试。

### R2. Durable AgentRun Mailbox

- Mailbox 是 AgentRun Product business module，不属于 AgentRuntime kernel；其 domain、application policy、repository interface与前端 projection均以 AgentRun owner组织。
- Mailbox owner 为 `run_id + agent_id`，不以 RuntimeSession、source coordinate 或当前 frame 作为生命周期 owner。
- 所有消息先形成 durable envelope，再由 scheduler 决定 `submit_input`、`steer`、等待、暂停、阻塞或失败。
- Envelope 至少保留 origin、source identity、payload、delivery policy、barrier、drain mode、priority/order、claim lease、accepted Agent effect、错误与时间字段。
- Intake receipt 与 concrete Agent effect receipt 是两个不同事实：前者证明 Product 已持久接收 envelope，后者证明具体投递 effect 已被 Agent 接纳或应用。

### R3. 显式 Queue / Steer 语义

- 普通 Enter 或默认 producer 输入：
  - Agent idle：提交新 turn。
  - Agent active：进入 Pending，等待当前 Agent turn terminal 后再提交新 turn。
- Ctrl/Cmd+Enter 或显式 `delivery_intent=steer`：
  - 仅在可 Steer 的 active conversation turn 上投递显式 `AgentCommand::Steer { expected_turn_id }`。
  - turn 不匹配、interaction/compaction 不允许 Steer或 active turn 已结束时，保留可解释的 queued/blocked/failed 状态，不得偷偷改成普通 Submit。
- Product command interface 中 `SubmitInput` 与 `Steer` 必须一义一命令；禁止 `SubmitInput` 根据快照自动改写成 Steer。

### R4. 新 AgentRuntime Adapter

- 复用 Complete Agent 的 `read / execute / inspect / live_batches`，不得恢复旧 RuntimeSession、execution anchor、delivery binding 或 SessionCore。
- AgentRuntime/Complete Agent只提供 concrete Agent mechanism：authoritative state、显式 command、effect receipt/inspection、live fact与 callback transport；不得持有 Mailbox、producer、Channel、Hook delivery policy或排队状态。
- Mailbox scheduler 读取 authoritative `AgentRuntimeView` 选择可执行动作，并以 mailbox message 派生稳定 Agent effect identity。
- Agent live terminal 是低延迟 wake；启动扫描、lease recovery 与 authoritative read 是 durable recovery，不能只依赖 process-local live stream。
- 同一 AgentRun owner 同时只能有一个 dispatcher lease，避免多个 worker 越过 `DrainMode::One` 顺序并发 claim。

### R5. 统一 Producer 接线

- Composer、Draft 首条输入、Canvas、Companion child dispatch/result/parent request/response/human response、Routine、Workflow 和需要 Agent 继续处理的 Hook/System 输入统一调用 Mailbox intake interface。
- 公开 `/runtime/commands` 不得继续提供绕过 Mailbox 的普通 input 提交入口；Interrupt、Compaction、Interaction response 等即时控制命令继续直达 AgentRuntime。
- Agent 暂时 unavailable 时 envelope 保持可恢复状态，而不是让调用方丢失已经接受的业务输入。

### R6. Channel 是真实 transport seam

- Channel admission、delivery intent、AgentRun Mailbox materialization 与 Channel delivery state 形成一条统一路径。
- Companion 不再手工构造 Channel intent 后绕过 Channel transport。
- Channel `MaterializedDeliveryRef` 指向 mailbox message；跨 owner 写入失败必须通过稳定 delivery identity 可重放、可收敛。
- 删除或替换当前未编译的孤儿 `channel/agent_run_delivery.rs`。

### R7. 输入 provenance

- `origin + source identity + submission kind` 必须从 Mailbox 贯穿 AgentRuntime command、Complete Agent adapter、authoritative history 和 canonical `UserInputSubmitted`。
- Native 不得再把全部输入硬编码成 `core_composer`。
- Codex/Remote adapter 必须提供重启后仍可恢复的 provenance；不得只做 process-local UI overlay。

### R8. 完整前端恢复

- 恢复 Waiting / Steer / Pending UI，以及 pause/resume、promote-to-steer、recall/retry、reorder、delete 和完整错误详情。
- Running + 输入为空显示 Stop；Running + 有输入显示 Send。
- Enter 表示 Queue/normal submit，Ctrl/Cmd+Enter 表示显式 Steer，两者必须形成不同 wire intent。
- Transcript 使用 authoritative `submissionKind + source` 展示 User、Steer、Companion、Channel、Routine/Workflow 等来源；Mailbox row 状态只来自 durable Mailbox projection，不做前端时序推断。

### R9. PostgreSQL 与恢复

- 新增当前 migration 序列中的正式 Mailbox schema，并从 `RETIRED_POSTGRES_TABLES` 移除重新启用的表、加入 required readiness。
- AgentRun-anchored Product Hook 使用新的 `agent_run_*` durable run/effect/work schema；旧 `agent_runtime_hook_plan/run/effect` 继续 retired，不改名复活。
- 不恢复旧 RuntimeSession 外键和历史兼容字段；schema 直接以当前 `run_id + agent_id + runtime_thread/source trace` 语义设计。
- claim/recovery 使用数据库时间、owner lease、claim token 与 stale-worker fencing。
- delivery result unknown 必须 inspect 同一 Agent effect；只有 `NotApplied` 可重派，`Unknown` 保持阻塞/待恢复。

### R10. 规范与删除收敛

- 更新 AgentRuntime Product、Kernel、Persistence、Channel、Hooks、frontend/cross-layer 规范，形成单一 Mailbox/AgentRuntime 责任模型。
- 删除“同步 input handoff 不存在 queued”以及“Product 自动选择 Submit/Steer”等与目标行为冲突的规范。
- 不保留双写、兼容 endpoint、旧 RuntimeSession adapter 或 fallback。

### R11. Hook turn-boundary 与 durable effect

- `AfterTurn`、`BeforeStop`、terminal auto-resume 与其它 Agent-visible Hook delivery 必须在本任务内恢复，不拆成可选后续任务。
- Complete Agent Hook callback contract 必须能无损表达同一次 resolution 的 gate、rewrite、context、continue、refresh 与 effect；不得因为只能返回单个 enum 而拒绝合法的组合结果。
- `AfterTurn` 产生的 steering message 先 durable materialize 为 `AgentLoopTurnBoundary + DrainMode::All` Mailbox envelope，再在同一安全边界 claim/consume。
- `BeforeStop` continuation 先 durable materialize 为 `AgentRunTurnBoundary + ContinueOnStop + DrainMode::All` envelope；只有已 claim 的 continuation 可以让 Agent Loop 继续。
- terminal auto-resume 使用 terminal effect identity 建立 `ImmediateIfIdle + DrainMode::One` envelope；重复 terminal callback、服务重启和 unknown response 不得重复续跑。
- AgentRun-anchored Product Hook plan adoption、canonical HookRun、HookEffect、claim lease、重放与 trace归 AgentRun Product module；AgentRuntime只承载已绑定 surface callback及 concrete effect receipt。
- 不恢复旧 RuntimeSession Hook runtime owner，也不让通用 AgentRuntime接管 Product workflow/hook policy。
- Hook 的同步 gate/context decision 与异步 Agent-visible delivery 是两个边界：前者在 Complete Agent callback deadline 内返回，后者以 Mailbox/HookEffect durable fact 驱动。

## Acceptance Criteria

- [ ] Active turn 期间普通 Enter 返回 durable queued mailbox row，且不会写入当前 turn 的 `InputAccepted`。
- [ ] Active turn 期间 Ctrl/Cmd+Enter 产生显式 Steer；release provider boundary 后当前 turn 使用该输入继续，原 turn 不失败。
- [ ] 同时发生 Core callback commit 与 Steer 时，两份历史都成功提交，不出现 `repository state changed` 或 `execution_callback_error`。
- [ ] 两条普通 pending message 严格按顺序在两个后续 turn 中消费；多 worker/重复 wake 不会并发越序。
- [ ] Agent unavailable、服务重启和 response unknown 场景下，Mailbox 可通过同一 message/effect identity 恢复，不重复 launch/steer。
- [ ] Companion result、parent request/response、human response 和 child dispatch 均创建 Channel delivery + Mailbox message；父 Agent active 时默认 Pending，不硬 Steer。
- [ ] Routine、Workflow、Canvas 默认 Queue；只有显式 Steer intent 能进入当前 active turn。
- [ ] Native、Codex 与 Remote history 中能够恢复输入来源与 Prompt/Steer 类型，前端正确显示对应来源。
- [ ] Required `AfterTurn` / `BeforeStop` rule 可以完成 Complete Agent provisioning，不再因 `ContinueTurn` / `RefreshSurface` 被判 unsupported。
- [ ] AfterTurn 多 action resolution 无损执行；Agent-visible continuation 先落 Mailbox，再在 Agent loop safe boundary 消费。
- [ ] BeforeStop gate 可以阻止停止或以 durable continuation 继续；没有 claim 到 continuation 时不得伪造 Continue。
- [ ] terminal auto-resume 使用稳定 HookEffect/Mailbox identity，重复 callback、进程重启与 response unknown 均只启动一次后续 turn。
- [ ] HookRun 与 HookEffect 的 accepted/running/terminal、lease recovery、stale ack fencing 和 trace 可从 PostgreSQL 恢复。
- [ ] 前端恢复 Waiting / Steer / Pending、pause/resume、promote、recall/retry、reorder、delete 与错误详情。
- [ ] Running + 空输入显示 Stop，Running + 有输入显示 Send；Enter 与 Ctrl/Cmd+Enter 的请求 contract 不同。
- [ ] PostgreSQL clean migration、既有开发库顺序 migration、repository claim/recovery 集成测试通过。
- [ ] AgentRuntime、Mailbox、Channel、Companion、Routine/Workflow、API contract generation、frontend state/UI 的相关测试通过。
- [ ] 负向源码检查确认不存在公开普通输入绕过 Mailbox、Product 自动 Submit→Steer、旧 RuntimeSession Mailbox adapter和未编译 Channel delivery 孤儿。

## Out of Scope

- 不恢复旧 RuntimeSession、SessionCore、execution anchor、delivery binding 或其数据库表。
- 不提供旧 API/database compatibility、dual-write 或 fallback。
- 不把 LifecycleGate waiting fact并入 Mailbox message；Waiting 继续由 Gate owner 持有，前端只组合展示。
- 不重做 AgentRuntime conversation、context、compaction、tool/interaction 的既有事实模型，除非是输入 provenance 或 Mailbox wake 所必需的接口调整。
- 不重做 Hook 规则编辑器、Rhai 语言或 workflow hook 配置模型；本任务恢复的是现有规则在新 AgentRuntime 下的执行、投递、持久化与恢复。
- 不在 `agentdash-agent-runtime` 中实现 AgentRun Mailbox、Channel、Companion、Product Hook delivery或其数据库 repository。
