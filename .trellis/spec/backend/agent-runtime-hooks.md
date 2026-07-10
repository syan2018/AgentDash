# Managed Agent Runtime Hook Orchestration

## 1. Scope / Trigger

本规范适用于 Managed Runtime 对已绑定 Hook Plan 的采用、canonical HookRun 生命周期、HookEffect 持久化与恢复，以及 Driver hook notification 进入 Runtime journal 前的校验。修改 Hook plan revision、execution site、failure policy、effect descriptor、HookRun event 或 hook work lease 时必须复核本规范。

## 2. Signatures

```rust
pub async fn bind_hook_plan(
    &self,
    binding: RuntimeHookPlanBinding,
) -> Result<RuntimeHookPlanBinding, HookOrchestrationError>;

pub async fn accept_hook(
    &self,
    invocation: RuntimeHookInvocation,
) -> Result<HookAdmission, HookOrchestrationError>;

pub async fn start_hook(
    &self,
    hook_run_id: &HookRunId,
) -> Result<HookRun, HookOrchestrationError>;

pub async fn complete_hook(
    &self,
    hook_run_id: &HookRunId,
    completion: HookCompletion,
    effects: Vec<HookEffect>,
) -> Result<HookRun, HookOrchestrationError>;
```

Runtime 从 Thread 当前 durable `BoundRuntimeHookPlan` 解析 invocation；调用者不能传入或替换计划。`HookExecutionSite` 包含 Managed Runtime、Tool Broker、Agent Core Callback、Driver Native 与 Observed Event Reaction，所有 actionful route 共享同一个 canonical HookRun。

## 3. Contracts

- Business Agent Surface 编译 Hook sources 与 requirements，Driver Host 绑定 route/profile；Managed Runtime 只采用 immutable bound plan，并独占 HookRun 状态机与 journal authority。
- 每个 Thread 的首个 `HookPlanRevision` 必须为 1，后续严格增加 1；相同 revision 与完整内容可幂等重放。digest 表达内容而非唯一历史身份，因此不同 revision 可以复用相同 digest。
- Actionful HookRun 必须按 `Accepted -> Running -> Completed | Blocked | Failed | Stopped | Cancelled` 分三次 durable transaction 推进，并分别产生 canonical accepted/started/terminal event。并发重放只有完整 immutable identity 与目标状态一致时才幂等成功。
- Runtime 记录所有 execution site 的 actionful HookRun，但不因此接管 Tool Broker、Agent Core 或 Driver Native 的实际执行。Driver event 不能直接伪造 HookPlan/HookRun Runtime-owned transition。
- 纯 `Observe + ObserveOnly` 的 silent invocation 不创建 HookRun、不写 event，也不推进 durable cursor。
- HookRun correlation 必须属于同一 Thread；Item/Interaction 必须同时匹配其 Turn。数据库用 operation/turn/item/interaction composite foreign key保护同一关系。
- failure policy 同时约束 terminal status、decision、diagnostic 与 effect：FailClosed failure 收敛为 Blocked+Block；FailOpenWithDiagnostic/ObserveOnly/RetryDurableEffect failure 收敛为 Failed+Continue 且包含 diagnostic；RetryDurableEffect plan 必须含 EmitEffect，成功完成必须产生 effect。
- Hook terminal、terminal event 与完整 effect 集在同一 RuntimeCommit 中提交。terminal event包含 decision、message和排序后的 effect IDs，重放必须核对完整集合。
- Effect descriptor携带 type、schema version、target authority、idempotency key、retry limit和payload digest。payload digest必须由递归 key-sorted canonical JSON计算为SHA-256，并在Runtime与PostgreSQL adapter两侧复验。
- HookRunRecovery 与 HookEffect 是两类独立 durable work。两者沿用数据库时钟、`FOR UPDATE SKIP LOCKED`、owner/token/lease/attempt与stale-worker fencing；Run recovery不等同于effect delivery。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| 首plan revision不是1，或后续不严格+1 | typed conflict，projection/journal不变 |
| invocation definition/point不在当前durable plan | `DefinitionNotBound` |
| HookRun ID复用但plan/site/correlation/input不同 | `RunConflict` |
| Item/Interaction不属于相关Turn或Thread | `InvalidCorrelation` / database constraint，整事务回滚 |
| Driver发送HookPlanBound/HookRun transition | quarantine为Runtime-owned hook event violation，并按critical protocol规则收敛 |
| FailClosed提交Failed+Block或Continue | `CompletionPolicy` |
| fail-open failure没有diagnostic | `CompletionPolicy` |
| RetryDurableEffect plan无EmitEffect，或成功无effect | typed policy error |
| effect ID/idempotency key重复、坐标不符或digest不匹配 | `InvalidEffectDescriptor` / atomic rollback |
| terminal/effect exact replay | 返回既有terminal，不新增event/effect |
| terminal/effect replay内容不同 | conflict，不覆盖 durable facts |
| lease过期后的旧owner/token ack/release | `WorkClaimConflict` |

## 5. Good / Base / Bad Cases

**Good case:** Tool Broker invocation 先按当前 plan durable accept，实际 broker 获得执行权后 durable start，最终把 decision、diagnostic 与 canonical-digest effects 原子完成；崩溃后分别从 HookRunRecovery 或 HookEffect queue恢复。

**Base case:** ObserveOnly observer没有行为改变或实质诊断，返回 `SilentObserver`，Thread revision和event cursor保持不变。

**Bad case:** Driver notification自行发送 `HookRunTerminal`，或调用者把旧 plan作为参数传给Runtime执行。这会形成第二事实源，必须在进入projection前拒绝。

## 6. Tests Required

- In-memory behavior覆盖plan revision/replay、silent observer、五类execution site、三段HookRun、并发accept/start/terminal、failure policy与correlation。
- Driver event behavior覆盖所有Runtime-owned hook event forgery与Lost/quarantine语义。
- Effect测试覆盖canonical JSON key ordering、payload变化、batch ID/idempotency冲突、terminal exact/conflicting replay。
- 真实embedded PostgreSQL覆盖Accepted/Running/Terminal各阶段、历史plan revision、复合FK、terminal+effects rollback和projection cursor不前进。
- HookRunRecovery与HookEffect分别覆盖多worker排他、lease过期接管、attempt递增、retry limit和stale ack fencing。
- contracts generation/check与migration guard必须通过；0062不得读取、回填或双写旧Hook/Session事实源。

## 7. Wrong vs Correct

```rust
// Wrong: 调用者提供可能过期或伪造的plan。
runtime.accept_hook(&caller_plan, invocation).await?;

// Correct: Runtime只按Thread当前durable plan接受调用。
runtime.accept_hook(invocation).await?;
```

```rust
// Wrong: 一次事务直接把新run写成terminal，accepted/running无法恢复或审计。
persist_terminal(run_id, effects).await?;

// Correct: 三个durable boundary各自幂等，terminal与effects原子提交。
runtime.accept_hook(invocation).await?;
runtime.start_hook(&run_id).await?;
runtime.complete_hook(&run_id, completion, effects).await?;
```
