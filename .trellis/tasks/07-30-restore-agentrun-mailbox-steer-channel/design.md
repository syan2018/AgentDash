# AgentRun Mailbox、Steer、Channel 与 Hook 恢复设计

## 1. Architecture

### 1.1 Ownership

| Authority | Owns | Does not own |
| --- | --- | --- |
| AgentRun Product | Mailbox envelope、producer dedup、排序、pause、claim/settlement、dispatcher lease、Channel/Companion/Hook delivery、Product HookRun/HookEffect、前端 projection | concrete Agent history |
| Complete Agent / AgentRuntime | conversation/turn/item/interaction、Submit/Steer/Interrupt/Compact、effect acceptance/inspection、live batches、callback transport | producer queue、Product Hook policy与delivery |
| Channel | participant/admission、delivery intent/state、stable delivery identity | Mailbox claim与 Agent command |
| Dash source owner | repository mutations的串行提交与 fencing | Product Mailbox policy |

Mailbox owner始终是 `run_id + agent_id`。Runtime thread、source coordinate、binding generation、turn/effect ID只作为当前 delivery target与因果证据。

这是一个 AgentRun business module。删除该 module后，Queue/Steer选择、producer dedup、Hook continuation、Channel materialization和恢复复杂度会重新散落到所有调用者，因此该 seam具有真实 depth。AgentRuntime adapter只是内部 seam，不对 producer暴露。

### 1.2 Deep module interface

Product侧恢复一个深模块 `AgentRunMailbox`，外部只依赖：

```rust
trait AgentRunMailboxIntake {
    async fn accept(&self, request: AcceptMailboxMessage) -> Result<MailboxReceipt, MailboxError>;
}

trait AgentRunMailboxControl {
    async fn apply(&self, command: MailboxControlCommand) -> Result<MailboxControlReceipt, MailboxError>;
}

trait AgentRunMailboxDispatch {
    async fn wake_owner(&self, owner: AgentRunMailboxOwner, cause: MailboxWakeCause);
    async fn claim_boundary(
        &self,
        request: MailboxBoundaryClaimRequest,
    ) -> Result<MailboxBoundaryClaim, MailboxError>;
}
```

Producer只能看到 intake receipt，不得直接构造 Agent command。Scheduler通过内部 Complete Agent adapter执行 Submit/Steer并inspect effect。

## 2. Command And Delivery Semantics

### 2.1 Explicit commands

`AgentRunProductCommand`恢复一义一命令：

- `SubmitInput`：只提交新 Agent turn。
- `Steer { expected_turn_id }`：只注入指定 active conversation turn。
- `Interrupt`、`RequestCompaction`、`ResolveInteraction`、`Close`：继续作为即时控制命令。

删除 Product snapshot驱动的 `SubmitInput -> Steer` 自动改写。

### 2.2 Mailbox policy

| Intent / Agent state | Barrier | Drain | Action |
| --- | --- | --- | --- |
| normal + idle | `ImmediateIfIdle` | One | SubmitInput |
| normal + active/cancelling | `AgentRunTurnBoundary` | One | Pending，terminal后SubmitInput |
| explicit steer + matching steerable turn | `AgentLoopTurnBoundary` | All | Steer(expected turn) |
| explicit steer + temporarily non-steerable | 保留原 barrier | All | Blocked/Pending，显示原因，不降级 |
| Hook AfterTurn steering | `AgentLoopTurnBoundary` | All | callback safe boundary claim |
| Hook BeforeStop continuation | `AgentRunTurnBoundary` + `ContinueOnStop` | All | before-stop safe boundary claim |
| Hook terminal auto-resume | `ImmediateIfIdle` | One | terminal后SubmitInput |

## 3. Durable State

### 3.1 Mailbox envelope

正式 schema直接按当前模型设计：

- identity：message id、owner run/agent、origin、source identity、source dedup key；
- payload：typed input blocks、preview、retention；
- policy：delivery intent、barrier、drain mode、stop effect、priority/order；
- target evidence：runtime thread、source coordinate、binding generation、expected turn；
- work state：status、claim owner/token/lease/attempt；
- effect evidence：Agent effect id/idempotency key、accepted receipt、inspect status；
- diagnostics：typed error、retryable、timestamps。

另设 owner dispatcher lease，锁定 `run_id + agent_id`。消息级 `SKIP LOCKED` 不再单独承担顺序保证。

### 3.2 AgentRun Product Hook state

在 AgentRun Product module恢复：

- 当前 AgentFrame/applied surface所采用的 Product Hook plan coordinate；
- canonical HookRun `Accepted -> Running -> terminal`；
- HookEffect descriptor、payload digest、idempotency key、retry policy；
- HookRun recovery lease与 HookEffect delivery lease；
- Hook terminal与完整 effect set原子提交。

新表使用 `agent_run_*` 命名并以 run/agent为 owner，以 runtime thread、turn/item/interaction correlation和 Product binding作为执行证据。旧 `agent_runtime_hook_plan/run/effect` 不复活，因为通用 AgentRuntime不是 Product Hook workflow authority。

## 4. Thin Complete Agent Adapter

AgentRun Mailbox内部 adapter只使用 Complete Agent：

1. `read` authoritative snapshot；
2. 根据 envelope policy选择 Submit或显式 Steer；
3. 以 mailbox message派生稳定 Agent effect identity；
4. `execute`；
5. 超时/unknown时用 `inspect` 同一 effect；
6. `Applied` 才settle，`NotApplied` 才可重派，`Unknown` 保持恢复状态；
7. `live_batches` terminal只负责低延迟 wake；启动扫描和 due scan负责 durable recovery。

Codex、Native、Remote adapter都必须接受 provenance envelope，并在 authoritative history中投影 `submissionKind + source`。

该 adapter不承载 Mailbox policy、HookRun状态机或 producer特例。它只把 AgentRun决定好的显式动作翻译为 concrete Agent command，并把 receipt/inspect/live fact翻译回 AgentRun。

## 5. Hook Boundary Design

### 5.1 Composite callback outcome

将单一 `AgentHookDecision` 改为结构化 `AgentHookOutcome`，分别表达：

- allow/deny；
- input/result rewrite；
- context additions；
- refresh request；
- continuation delivery refs；
- emitted durable effect refs；
- diagnostics。

同一次 resolution允许合法组合，Host仍按 bound surface逐项验证 action。

### 5.2 AfterTurn / BeforeStop

AgentRun Product callback handler按固定顺序执行：

1. 从 Complete Agent callback transport获得已验证的 bound surface coordinate；
2. 在 AgentRun Product module按当前 plan durable accept/start HookRun；
3. Product evaluator生成完整 resolution；
4. Agent-visible messages先写入 Mailbox；
5. HookRun terminal与 HookEffect refs原子提交；
6. 对当前 safe boundary执行带 token 的 Mailbox boundary claim；
7. callback经 AgentRuntime transport返回 composite outcome与 claimed message refs；
8. Agent core持久接纳消息后返回 effect receipt；
9. Mailbox按 receipt settle；unknown通过 Complete Agent inspect恢复。

AfterTurn只claim `AgentLoopTurnBoundary`，BeforeStop只claim `AgentRunTurnBoundary + ContinueOnStop`。未获得 durable claim时不得返回伪造 Continue。

### 5.3 Terminal auto-resume

Agent terminal fact创建稳定 HookEffect。Effect worker以：

`runtime_thread + source_turn + terminal sequence + hook definition`

派生 idempotency identity，向 Mailbox materialize `hook_auto_resume` envelope。重复 callback、effect retry与进程重启返回同一 Mailbox message。

## 6. Channel And Producers

统一数据流：

```text
Composer / Canvas / Routine / Workflow
        -> Mailbox intake

Companion
        -> Channel admission
        -> Channel delivery intent
        -> AgentRun Mailbox materialization
        -> Channel MaterializedDeliveryRef(mailbox_message_id)

AgentRun Product Hook
        -> AgentRun HookRun / HookEffect
        -> AgentRun Mailbox materialization
```

Channel generic dispatcher获得实际生产接线。Companion不再直接调用 Product input delivery。Channel state只有在 Mailbox receipt成功后才能变为 Materialized。

## 7. Dash Mutation Writer

为每个 Dash source建立 owner-scoped mutation writer：

- command、Core callback、terminal fence、surface/effect mutation进入同一串行队列；
- writer在最新 repository上应用 mutation并执行 CAS；
- CAS冲突只表示 stale external owner/fence，不再把合法同 owner并发当业务冲突；
- steering mutex只负责 steer acceptance/terminal ordering，不替代 repository writer。

Mailbox恢复不能替代该修复，两者分别解决 Product intake与 concrete owner并发写入。

## 8. API And Frontend

- 普通 input endpoint进入 Mailbox并返回 receipt/projection。
- 显式 Steer contract带 `expected_turn_id`，不得借用普通 Submit。
- 恢复 Mailbox list/control/update contracts并重新生成 TypeScript。
- `SessionChatView`恢复 Mailbox projection；Composer中 running+空输入为 Stop，running+有输入为 Send。
- Enter发送 normal/queue，Ctrl/Cmd+Enter发送 explicit steer。
- transcript来源只读取 authoritative history provenance；Mailbox row只读取 durable Mailbox projection。

## 9. Migration

- 新增当前 migration序列，不修改历史 migration。
- 创建新 AgentRun Mailbox、owner lease、Product Hook run/effect/work lease schema。
- 从 retired清单移除重新启用的当前表，并加入 readiness required tables。
- `agent_runtime_hook_plan/run/effect` 保持 retired；不通过表名兼容掩盖 authority变化。
- 不回填、不双写、不提供旧 endpoint；开发库按顺序 migrate到唯一新模型。

## 10. Failure And Recovery Matrix

| Failure | Recovery |
| --- | --- |
| intake response丢失 | source dedup返回同一 message/receipt |
| worker claim后崩溃 | DB time lease到期后新 owner接管 |
| Agent execute response unknown | inspect稳定 effect；Unknown不重派 |
| terminal live batch丢失 | startup/due scan读取 authoritative state |
| Channel materialize跨 owner失败 | stable delivery identity重放 |
| Hook callback terminal前崩溃 | HookRun recovery从 durable stage继续 |
| HookEffect delivery崩溃 | effect lease接管并materialize同一 Mailbox message |
| Dash callback与Steer并发 | owner writer串行提交两次合法 mutation |

## 11. Trade-offs

- 本任务不拆分 Hook，是因为任何 AgentRun producer仍可绕过 durable intake都会破坏统一 Mailbox invariant。
- 不直接复原旧 RuntimeSession adapter，是因为当前 authority已迁移到 Product binding + Complete Agent。
- 不把 Mailbox/Hook delivery放入 AgentRuntime，是因为它们依赖 run/agent、Channel、Companion、Workflow和前端控制语义；AgentRuntime应保持 provider-neutral mechanism。
- 增加 owner lease和owner writer，是对旧版仅消息锁与整库CAS缺陷的必要修正。
- 采用单任务分阶段实施，原因是 schema、command semantics、producer wiring和前端 projection必须围绕同一 envelope contract收敛；独立子任务容易形成临时双接口。
