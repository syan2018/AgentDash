# 修复 SubAgent terminal gate 收束设计

## Problem Model

这次事故表面是 SubAgent runtime failed 后，`companion_wait_follow_up` gate 长期 open。更底层的问题是：系统已经创建了一个 durable wait obligation，却没有在 producer terminal 且 expected result 不可能出现时收束它。

正式状态迁移应是：

```text
RuntimeSession terminal
-> AgentRunDeliveryBinding terminal
-> Wait obligation convergence observes producer terminal
-> LifecycleGate resolved with failed/cancelled/protocol_failed result
-> Parent mailbox receives result wake
-> wait/workspace project the resolved result
```

`wait` 和 workspace 是观察面；它们读取 durable wait/gate 状态，不负责发明写侧事实。Companion 负责声明“父 Agent 正在等子 Agent 的某个结果”，但 producer terminal 后如何收束等待，应落在通用 wait obligation convergence。

## Fact Source Boundary

本任务要收口的是“等待项如何由 producer terminal 事实完成”，不是把 runtime、mailbox、gate 和 projection 合并成一个大状态机。

| 事实 | 权威来源 | 本任务中的用法 |
| --- | --- | --- |
| runtime 实际终态 | `RuntimeSession` terminal notification / event trace | 只在 runtime -> AgentRun seam 作为输入事实 |
| AgentRun 当前 delivery 状态 | `AgentRunDeliveryBinding` | terminal signal 被产品控制面接纳后的 authority，也是 wait producer terminal event 的来源 |
| wait obligation | `LifecycleGate` 或后续 wait record | durable waiting fact，声明 producer、expected result、terminal policy 和 wake intent |
| gate/result 状态 | `LifecycleGate` resolved payload | wait/workspace/parent wake 的 result body authority |
| parent 是否被唤醒 | parent `AgentRunMailboxMessage` + command receipt | delivery projection；必须可由 resolved gate 幂等补投 |
| mailbox 是否暂停 | `AgentRunMailboxState` | child runtime control state，不代表 parent wait result |
| workspace/mailbox waiting_items | `ConversationWaitingItemModel` / API contract | 只读投影，不能作为事实源 |
| exec terminal 等待 | terminal registry | 与 gate 共用 UI 展示，但不是 LifecycleGate |
| provider/model 可执行性 | provider/account effective capability check | preflight observation，不替代 terminal convergence |
| runtime -> AgentRun launch evidence | `RuntimeSessionExecutionAnchor` | AgentRun runtime binding 内部校验证据；不作为 companion/API/workspace 的业务定位 interface |

## Runtime Anchor Containment

`RuntimeSessionExecutionAnchor` 应保留为 create-once launch evidence：它回答“这个 runtime trace 最初锚定到哪个 AgentRun 坐标”。它不应该成为 companion、workspace module、API route 或 wait projection 的业务定位 interface。

AgentRun terminal convergence 的正确形状是：

```text
RuntimeSession terminal notification
  -> AgentRunTerminalConvergence
       - resolve runtime trace through anchor internally
       - validate/update AgentRunDeliveryBinding
       - apply AgentRun mailbox terminal effect
       - emit AgentRunDeliveryTerminalEvent { run_id, agent_id, frame_id?, terminal_state, turn_id, delivery_trace_ref }
```

删除测试：

- 如果删除 `RuntimeSessionExecutionAnchorRepository` 对 API/companion terminal business code 的可见性，复杂度不应在多个调用点重新出现；它应集中到 AgentRun terminal convergence / delivery resolution module 内部。
- 如果删除 `AgentRunDeliveryBinding`，workspace、command policy、terminal status、boot reconcile 都会重新各自读取 runtime trace；因此 binding 是有价值的 AgentRun 事实源。

## Wait Obligation Seam

正式 deep module 应位于 `LifecycleGate` / wait obligation 写侧 convergence，而不是 companion 层。它的 interface 接收 producer terminal fact，不接收 gate kind 或裸 runtime session id。

推荐 interface 形状：

```rust
pub struct WaitProducerTerminalEvent {
    pub producer: WaitProducerRef,
    pub terminal_state: String,
    pub terminal_message: Option<String>,
    pub source_turn_id: Option<String>,
    pub trace_ref: Option<String>,
}

pub enum WaitProducerRef {
    AgentRunDelivery {
        run_id: Uuid,
        agent_id: Uuid,
        frame_id: Option<Uuid>,
    },
    RuntimeNode {
        run_id: Uuid,
        orchestration_id: String,
        node_path: String,
        attempt: u32,
    },
    ExecTerminal {
        terminal_id: String,
    },
}

pub async fn observe_producer_terminal(
    &self,
    event: WaitProducerTerminalEvent,
) -> Result<WaitObligationConvergenceResult, ApplicationError>;

pub async fn reconcile_open_obligations(
    &self,
    limit: usize,
) -> Result<WaitObligationReconcileReport, ApplicationError>;
```

该 module 内部拥有以下知识：

- 如何从 producer ref 找到相关 open wait obligations。
- 如何判断 expected result 是否已经由正常 writer 写入。
- terminal state 如何映射 result status：`failed`、`cancelled`、`protocol_failed`。
- resolved payload 如何包含 producer terminal diagnostics。
- 如何通过 gate delivery intent 幂等补投 parent mailbox wake。
- 如何在 boot reconcile 中复用同一套收束规则。

这样 `companion_wait_follow_up` 只是 `LifecycleGate` adapter 内部的一个 gate kind，不是外层 business interface。

## Wait Obligation Declaration

Companion dispatch 打开 gate 时，应把它声明为等待 child AgentRun delivery 产出 companion result 的 obligation。首选可以继续落在 `LifecycleGate.payload` JSON 中，避免过早引入 schema；如果查询和 reconcile 需要更强索引，再新增 migration。

推荐 payload/metadata 形状：

```json
{
  "wait_source": {
    "kind": "agent_run_delivery",
    "run_id": "...",
    "agent_id": "...",
    "frame_id": "...",
    "correlation_ref": "dispatch-..."
  },
  "expected_result": {
    "kind": "companion_result",
    "correlation_ref": "dispatch-..."
  },
  "on_producer_terminal_without_result": {
    "failed": "failed",
    "interrupted": "cancelled",
    "completed": "protocol_failed"
  },
  "wake": {
    "namespace": "companion",
    "target_run_id": "...",
    "target_agent_id": "...",
    "client_command_id": "companion-result:{gate_id}"
  }
}
```

当前已落地的 `find_by_agent_and_correlation` 可以作为 companion result retry 的精确查询继续保留；后续 reconcile 查询应围绕 wait source / producer terminal policy，而不是引入 `list_open_by_kind` 这类宽接口。

## Data Flow

Runtime callback path：

1. `AgentRunTerminalControlCallback::on_session_terminal` 收到 runtime terminal notification。
2. callback 调用 AgentRun terminal convergence interface，不直接查询 anchor 或 mailbox。
3. AgentRun convergence 内部解析 runtime trace evidence，验证 current delivery binding，写入 terminal delivery fact，并处理 AgentRun mailbox terminal effect。
4. AgentRun convergence 返回 `AgentRunDeliveryTerminalEvent`，其中业务定位使用 run/agent/frame，runtime session 只作为 delivery trace ref。
5. callback 将 event 映射为 `WaitProducerTerminalEvent::AgentRunDelivery`，交给 wait obligation convergence。
6. wait obligation convergence 找到等待该 producer 的 open gate，按 terminal policy resolve gate。
7. convergence 使用 gate wake intent 和稳定 `client_command_id = companion-result:{gate_id}` 确保 parent mailbox wake。
8. `wait_activity` 和 workspace projection 从 resolved gate 读取真实状态。

Normal result path：

1. child 调用 `companion_respond`。
2. Companion adapter 用 request/correlation 精确定位对应 gate。
3. gate resolver first-writer-wins 地写入正常 result payload。
4. parent mailbox wake 通过同一稳定 source identity / client command id 投递。
5. 后续 producer terminal event 只做幂等 no-op 或 ensure delivery，不覆盖正常 result。

Boot reconcile path：

1. boot reconcile 调用 wait obligation convergence 的 `reconcile_open_obligations(limit)`。
2. convergence 扫描声明了 producer terminal policy 的 open obligations。
3. 对每个 obligation 读取 producer 当前 terminal fact，例如 `AgentRunDeliveryBinding.status = terminal`。
4. 已 terminal 且 expected result 未出现时，调用与 runtime callback path 相同的收束逻辑。
5. report 输出 reconciled/skipped/errors 和诊断字段。

## Result Contract

wait obligation result status 扩展为：

```text
completed | blocked | needs_follow_up | failed | cancelled | protocol_failed
```

如果 wire/status normalizer 暂不适合新增 `protocol_failed`，可将用户可见 status 暂时映射为 `failed`，但 payload 必须保留：

```json
{
  "status": "failed",
  "failure_kind": "missing_companion_respond",
  "source": "producer_terminal"
}
```

runtime failure payload 推荐字段：

```json
{
  "status": "failed",
  "summary": "SubAgent runtime failed before companion_respond.",
  "terminal_state": "failed",
  "terminal_message": "Codex API returned 400 Bad Request: ...",
  "delivery_trace_ref": "...",
  "resolved_turn_id": "...",
  "failure_kind": "runtime_terminal_failed",
  "source": "producer_terminal",
  "findings": [],
  "follow_ups": [],
  "artifact_refs": []
}
```

`interrupted` 的用户语义建议映射为 `status = "cancelled"`，同时保留 `terminal_state = "interrupted"`，这样跨层展示可以使用稳定业务状态，底层诊断仍能看到 runtime 原文。

`completed` 但没有 expected result 不是有效 success，应映射为 protocol failure。

## Idempotency

现有 parent delivery 使用 `client_command_id = companion-result:{gate_id}`，天然适合作为 dedup key。需要补齐的是 gate 已 resolved 后的 retry：

- 如果 gate open，wait obligation convergence 可以 resolve gate 并投递 parent mailbox。
- 如果 gate 已 resolved，且 resolved payload 对应同一个 request/gate，convergence 跳过 payload 更新，只执行 ensure parent mailbox delivery。
- 如果 gate 已 resolved 为正常 writer 的结果，例如 `companion_respond`，producer terminal convergence 不覆盖 payload，仅确保已经存在的 delivery 可观测。

优先方案是增强现有 gate resolver / delivery intent 协调能力，让 resolved gate 也能重新生成 delivery intent。只有当后续 gate result 种类继续增多、delivery 重试需求扩大时，再考虑持久化 `LifecycleGateDeliveryOutbox`。

## Waiting Projection Convergence

本任务建议增加一个小的 gate waiting projection/status helper，至少统一以下规则：

- open gate 在 agent-facing `wait` 中是 `pending`，在 workspace gate 详情中仍可保留原始 `open`，但 contract 要明确两者的含义。
- resolved gate 优先读取 payload.status，例如 `completed / blocked / needs_follow_up / failed / cancelled / protocol_failed`。
- payload.status 缺失时 fallback 为 `completed`，兼容既有已 resolved gate。
- preview/source_label 的 key 优先级保持现状，但最好由一处 helper 或一组共享测试锁定。

exec terminal waiting item 由 API 层从 terminal registry 追加，可以继续共用 UI 展示模型，但必须在设计上被视作另一个 source adapter。

## Model Capability Preflight

本次触发错误来自 `openai-codex/gpt-5.3-codex` 在本地 catalog 中存在，但当前 ChatGPT account runtime 不支持。launch 前应检查 provider/account effective capability，而不是只看静态 catalog。

推荐位置：

- companion frame construction 或 dispatch 前，确认 selected sub-agent preset 的 provider/model 对当前账号可执行。
- 如果发现不可执行，返回可见配置错误，并确保不会留下长期 open gate。
- 如果 gate 已经创建后才获得 provider 失败，仍由 wait obligation convergence 收束。

preflight 是体验优化和早失败机制；wait obligation convergence 是一致性保证。

## Migration And Contracts

首选方案先用 gate payload 中的 typed wait source / terminal policy 表达 obligation，无需 schema migration。实现时仍需检查：

- gate payload 查询是否需要索引；如果 reconcile 需要可靠高效查询，再新增 typed columns 或 companion-specific indexed repository query。
- Rust 类型或 status validation 是否需要扩展 enum/normalizer。
- TS contract 是否消费 companion/wait status；若有 wire DTO 变化，必须重新生成并检查 contract。
- 如果最终选择 outbox，需要新增当前序号之后的 migration，并通过 migration guard。

## Diagnostics

wait obligation convergence 应记录结构化诊断字段：

- producer_kind
- producer_run_id / producer_agent_id / producer_node_ref
- gate_id
- wait_kind
- terminal_state
- failure_kind
- delivery_trace_ref
- wake_client_command_id
- delivery_outcome

日志语义要支持排查三类结果：resolved and delivered、already resolved ensured delivery、no matching obligation found。

## Tests

测试覆盖以 durable state 为核心：

- child failed -> AgentRun delivery terminal -> wait obligation resolved failed -> parent mailbox result。
- child interrupted -> wait obligation resolved cancelled -> parent mailbox result。
- child completed without companion_respond -> protocol failure。
- companion_respond 与 producer terminal 竞态不覆盖 payload、不重复 parent command。
- resolved gate delivery retry 能补投 parent mailbox。
- wait 对 failed/cancelled/protocol_failed resolved gate 返回 ready item。
- workspace 不再展示 terminal child 的 open waiting item。
- wait 与 workspace 对同一个 resolved companion gate 的 status 语义一致。
- boot reconcile 修复 open obligation + terminal binding，且不依赖 gate kind 作为外部 interface。
- unsupported model preflight 返回可见错误；runtime provider 400 仍能通过 wait obligation convergence 收束。
- API terminal callback 不 import / query `RuntimeSessionExecutionAnchorRepository` for business effects；anchor usage is contained in AgentRun terminal convergence tests.
