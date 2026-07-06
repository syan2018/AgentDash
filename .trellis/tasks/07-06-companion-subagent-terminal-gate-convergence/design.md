# 修复 SubAgent terminal gate 收束设计

## Problem Model

SubAgent 是 child runtime，但 companion wait gate 是父子协作协议的一部分。只要 child 已经无法再产出 `companion_respond`，系统就必须把 runtime terminal 解释成一个 durable companion result，否则父 Agent 与用户界面都会继续等待一个不会到来的 gate。

权威状态迁移应是：

```text
RuntimeSession terminal
-> AgentRunDeliveryBinding terminal
-> Companion terminal bridge resolves child-owned gate
-> Parent mailbox receives companion-result wake
-> wait/workspace project the resolved result
```

`wait` 和 workspace 是观察面；它们读取 durable gate 状态，不负责发明事实。

## Fact Source Boundary

本任务需要顺手收口的是“同一个事实如何被解释”，不是把所有表和投影合并成一个大状态机。

| 事实 | 权威来源 | 本任务中的用法 |
| --- | --- | --- |
| runtime 实际终态 | `RuntimeSession` terminal notification / event trace | 只在 runtime -> AgentRun seam 作为输入事实 |
| AgentRun 当前 delivery 状态 | `AgentRunDeliveryBinding` | workspace、command policy、terminal bridge reconcile 的查询事实，也是 terminal signal 被产品控制面接纳后的 authority |
| companion 等待是否完成 | `LifecycleGate` | durable waiting fact，open/resolved 必须由 writer/reconciler 修改 |
| 父 Agent 是否被唤醒 | parent `AgentRunMailboxMessage` + command receipt | delivery projection；必须可由 gate result 幂等补投 |
| mailbox 是否暂停 | `AgentRunMailboxState` | child runtime control state，不代表 companion result |
| workspace/mailbox waiting_items | `ConversationWaitingItemModel` / API contract | 只读投影，不能作为事实源 |
| exec terminal 等待 | terminal registry | 与 gate 共用 UI 展示，但不是 LifecycleGate |
| provider/model 可执行性 | provider/account effective capability check | preflight observation，不替代 runtime terminal 收束 |
| runtime -> AgentRun launch evidence | `RuntimeSessionExecutionAnchor` | AgentRun runtime binding 内部校验证据；不作为 companion/API/workspace 的业务定位 interface |

`LifecycleGate` 的 waiting projection 当前有两份解释：`wait_activity` 会把 resolved gate 直接映射为 `completed`，workspace waiting item 则直接暴露 gate.status。修复 failed/cancelled result 时应同步收口这层状态解释，避免同一个 gate 在 `wait` 和 workspace 中显示不同语义。

## Runtime Anchor Containment

`RuntimeSessionExecutionAnchor` 应保留为 create-once launch evidence：它回答“这个 runtime trace 最初锚定到哪个 AgentRun 坐标”。它不应该成为 companion、workspace module、API route 或 wait projection 的业务定位 interface。

更深的 module 形状应是：

```text
RuntimeSession terminal notification
  -> AgentRunTerminalConvergence
       - resolve runtime trace through anchor internally
       - validate/update AgentRunDeliveryBinding
       - apply AgentRun mailbox terminal effect
       - emit AgentRunDeliveryTerminalEvent { run_id, agent_id, frame_id?, terminal_state, turn_id, delivery_trace_ref }
  -> CompanionTerminalGateConvergence consumes AgentRunDeliveryTerminalEvent
```

删除测试：

- 如果删除 `RuntimeSessionExecutionAnchorRepository` 对 companion/API terminal business code 的可见性，复杂度不应在多个调用点重新出现；它应集中到 AgentRun terminal convergence / delivery resolution module 内部。
- 如果删除 `AgentRunDeliveryBinding`，workspace、command policy、terminal status、boot reconcile 都会重新各自读取 runtime trace；因此 binding 是有价值的 AgentRun 事实源。

本任务应避免“新增一个 companion bridge 继续查 anchor”。那会修掉 zombie gate，却把错误 seam 固化得更深。

## Aggressive Convergence Assessment

按第一性原理，`runtime_session_id` 表达的是 executor trace identity，不表达产品控制面的所有权。产品控制面需要的是 `run_id / agent_id / frame_id`、当前 delivery binding、gate result 和 parent mailbox wake。`RuntimeSessionExecutionAnchor` 的价值是 create-once evidence，用来让 AgentRun seam 校验 runtime trace 是否仍对应当前 delivery；它不是跨模块业务地址。

当前引用面可以分成三类：

1. 已接近正确形状：`DeliveryRuntimeSelectionService` 从 `AgentRunDeliveryBinding` 出发，再用 anchor 校验 launch/frame/node 坐标。这说明 binding 是业务事实源，anchor 是校验证据。
2. 当前修复必须收口的路径：API terminal callback、companion terminal gate 收束、waiting projection。它们共同决定子 Agent terminal 后父 Agent 是否继续等待，继续用 runtime session 反查业务坐标会复制一套事实源。
3. 后续可拆的旁路：workspace module canvas diagnostics、task context visible canvas、部分 frame construction/bootstrap 和 terminal cancel safety net。它们也有 `runtime_session_id -> anchor -> run/agent/frame` 的形状，但不直接决定本次 zombie gate 闭环。

因此当前任务的激进边界是：terminal/companion/wait-workspace 这条链路本次完成 AgentRun-owned seam；其它旁路先登记为 follow-up 收束面，未来应逐步改成消费 `AgentRunRuntimeAddress`、`DeliveryRuntimeSelection` 或更窄的 application port，而不是各自查询 anchor。

推荐后续收束接口形状：

```rust
pub struct AgentRunDeliveryTraceContext {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub current_frame_id: Uuid,
    pub launch_frame_id: Uuid,
    pub delivery_trace_ref: String,
    pub terminal_state: Option<String>,
}

pub async fn resolve_delivery_trace_context(
    &self,
    delivery_trace_ref: &str,
) -> Result<AgentRunDeliveryTraceContext, ApplicationError>;
```

这个接口仍可在 AgentRun 内部读取 anchor，但外部模块只拿到 AgentRun 语义。这样既保留 trace 可诊断性，又避免 runtime session 成为第二套 control-plane address。

## Module Boundary

新增两个协作模块，而不是让 companion 直接吃 runtime session id：

1. AgentRun 层 deep module：`AgentRunTerminalConvergenceService` 或 `AgentRunDeliveryTerminalService`。
2. Companion 层 deep module：`CompanionTerminalGateConvergence`，消费 AgentRun terminal event。

AgentRun terminal convergence 的外部接口保持小而明确：

```rust
pub struct AgentRunRuntimeTerminalCommand {
    pub runtime_session_id: Uuid,
    pub turn_id: Option<Uuid>,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
}

pub async fn converge_runtime_terminal(
    &self,
    command: AgentRunRuntimeTerminalCommand,
) -> Result<AgentRunDeliveryTerminalEvent, ApplicationError>;
```

该模块内部拥有以下知识：

- runtime session execution anchor 到 lifecycle agent 的定位。
- current delivery binding validation。
- terminal transition 写入。
- mailbox terminal pause / completed boundary scheduling。
- delivery trace ref 到 AgentRun public terminal event 的投影。

Companion terminal convergence 的接口不接受裸 runtime session id，而接受 AgentRun 坐标事件：

```rust
pub struct CompanionAgentRunTerminalCommand {
    pub run_id: Uuid,
    pub child_agent_id: Uuid,
    pub child_frame_id: Option<Uuid>,
    pub delivery_trace_ref: Option<String>,
    pub resolved_turn_id: Option<String>,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
}
```

Companion 模块内部只拥有 companion lineage、child-owned gate、result payload 和 parent mailbox wake 的知识。API 层 terminal callback 只调用 AgentRun convergence；boot reconcile 从 delivery binding / gate 出发，也不需要直接暴露 anchor。

## Data Flow

1. `AgentRunTerminalControlCallback::on_session_terminal` 收到 runtime terminal notification。
2. callback 调用 AgentRun terminal convergence interface，不直接查询 anchor 或 mailbox。
3. AgentRun convergence 内部解析 runtime trace evidence，验证 current delivery binding，写入 terminal delivery fact，并处理 AgentRun mailbox terminal effect。
4. AgentRun convergence 返回 `AgentRunDeliveryTerminalEvent`，其中业务定位使用 run/agent/frame，runtime session 只作为 delivery trace ref。
5. companion terminal convergence 检查该 child agent 是否属于 companion dispatch lineage，并查找 child-owned open `companion_wait_follow_up` gate。
6. companion terminal convergence 构造 synthetic child result payload。
7. companion terminal convergence 调用现有 gate resolver 完成 gate resolved。
8. companion terminal convergence 使用现有 parent mailbox delivery 发送 `companion-result:{gate_id}`。
9. `wait_activity` 和 workspace projection 从 resolved gate 读取真实状态。

## Result Contract

companion result status 扩展为：

```text
completed | blocked | needs_follow_up | failed | cancelled
```

推荐 payload 字段：

```json
{
  "status": "failed",
  "summary": "SubAgent runtime failed before companion_respond.",
  "terminal_state": "failed",
  "terminal_message": "Codex API 返回 400 Bad Request: ...",
  "delivery_trace_ref": "...",
  "resolved_turn_id": "...",
  "failure_kind": "runtime_terminal_failed",
  "source": "runtime_terminal",
  "findings": [],
  "follow_ups": [],
  "artifact_refs": []
}
```

`interrupted` 的用户语义建议映射为 `status = "cancelled"`，同时保留 `terminal_state = "interrupted"`，这样跨层展示可以使用稳定业务状态，底层诊断仍能看到 runtime 原文。

`completed` 但没有 `companion_respond` 的 child 不是有效 companion success，应映射为：

```json
{
  "status": "failed",
  "failure_kind": "missing_companion_respond",
  "source": "runtime_terminal"
}
```

## Idempotency

现有 parent delivery 使用 `client_command_id = companion-result:{gate_id}`，天然适合作为 dedup key。需要补齐的是 gate 已 resolved 后的 retry：

- 如果 gate open，bridge 可以 resolve gate 并投递 parent mailbox。
- 如果 gate 已 resolved，且 resolved payload 对应同一个 companion request/gate，bridge 应跳过 payload 更新，只执行 ensure parent mailbox delivery。
- 如果 gate 已 resolved 为不同来源结果，例如正常 `companion_respond`，terminal bridge 不覆盖 payload，仅确保已经存在的 delivery 可观测。

优先方案是增强现有 `complete_child_result_to_parent`/delivery 协调能力，让 resolved gate 也能重新生成 delivery intent。只有当后续 gate result 种类继续增多、delivery 重试需求扩大时，再考虑持久化 `LifecycleGateDeliveryOutbox`。

## Waiting Projection Convergence

本任务建议增加一个小的 gate waiting projection/status helper，至少统一以下规则：

- open gate 在 agent-facing `wait` 中是 `pending`，在 workspace gate 详情中仍可保留原始 `open`，但 contract 要明确两者的含义。
- resolved gate 优先读取 payload.status，例如 `completed / blocked / needs_follow_up / failed / cancelled`。
- payload.status 缺失时 fallback 为 `completed`，兼容既有已 resolved gate。
- preview/source_label 的 key 优先级保持现状，但最好由一处 helper 或一组共享测试锁定。

exec terminal waiting item 由 API 层从 terminal registry 追加，不应在本任务内强行改造成 LifecycleGate。它可以继续共用 UI 展示模型，但必须在设计上被视作另一个 source adapter。

## Boot Reconcile

boot reconcile 增加 companion terminal convergence phase：

```text
list open companion wait gates
-> inspect owner child agent delivery/runtime state
-> if child terminal failed/interrupted/completed-without-response
-> call the same terminal bridge
```

这条路径修复历史 zombie gate，也覆盖 terminal callback 曾经失败或进程重启错过 callback 的情况。reconcile 不应实现第二套 payload 映射逻辑，而是复用 terminal bridge。

## Model Capability Preflight

本次触发错误来自 `openai-codex/gpt-5.3-codex` 在本地 catalog 中存在，但当前 ChatGPT account runtime 不支持。launch 前应检查 provider/account effective capability，而不是只看静态 catalog。

推荐位置：

- companion frame construction 或 dispatch 前，确认 selected sub-agent preset 的 provider/model 对当前账号可执行。
- 如果发现不可执行，返回可见配置错误，并确保不会留下长期 open gate。
- 如果 gate 已经创建后才获得 provider 失败，仍由 terminal bridge 收束。

preflight 是体验优化和早失败机制；terminal bridge 是一致性保证。

## Migration And Contracts

预计首选方案不需要 schema migration，因为 gate payload 是 JSON，mailbox delivery 已有 stable command id。实现时仍需检查：

- Rust 类型或 status validation 是否需要扩展 enum/normalizer。
- TS contract 是否消费 companion/wait status；若有 wire DTO 变化，必须重新生成并检查 contract。
- 如果最终选择 outbox，需要新增当前序号之后的 migration，并通过 migration guard。

## Diagnostics

terminal bridge 应记录结构化诊断字段：

- delivery_trace_ref
- child_agent_id
- parent_agent_id
- gate_id
- terminal_state
- failure_kind
- delivery_outcome

日志语义要支持排查三类结果：resolved and delivered、already resolved ensured delivery、no companion gate found。

## Tests

测试覆盖以 durable state 为核心：

- child failed -> gate resolved failed -> parent mailbox result。
- child interrupted -> gate resolved cancelled -> parent mailbox result。
- child completed without companion_respond -> protocol failure。
- companion_respond 与 terminal callback 竞态不覆盖 payload、不重复 parent command。
- resolved gate delivery retry 能补投 parent mailbox。
- wait 对 failed/cancelled resolved gate 返回 ready item。
- workspace 不再展示 terminal child 的 open waiting item。
- wait 与 workspace 对同一个 resolved companion gate 的 status 语义一致。
- boot reconcile 修复 open gate + terminal binding。
- unsupported model preflight 返回可见错误；runtime provider 400 仍能通过 bridge 收束。
- API terminal callback 不 import / query `RuntimeSessionExecutionAnchorRepository` for business effects；anchor usage is contained in AgentRun terminal convergence tests.
