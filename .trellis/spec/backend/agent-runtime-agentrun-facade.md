# AgentRun Product / Complete Agent Facade

## 1. Scope / Trigger

本规范适用于 AgentRun launch、Mailbox input、fork、surface rebind、conversation read/live、
workspace/list/delete，以及 Lifecycle/Companion/Routine/Workflow/Channel 对 Agent 的调用。
修改 Product command、Mailbox、association、AgentFrame 或 presentation query 时必须复核。

Facade 组合 Product shell 与 concrete Agent authority；AgentRun Product 拥有输入交付承诺，
Complete Agent拥有执行历史。

## 2. Signatures

```rust
pub struct DeliverAgentRunProductInput {
    pub target: AgentRunTarget,
    pub content: Vec<AgentInputContent>,
    pub source: AgentInputSourceIdentity,
    pub origin: AgentInputOrigin,
    pub client_command_id: String,
}

pub struct AgentRunProductInputDelivery {
    pub handoff_id: Uuid,
    pub mailbox_message_id: Uuid,
    pub operation_receipt: Option<AgentRuntimeOperationReceipt>,
    pub queued: bool,
}

pub enum AgentRunMailboxDeliveryIntent {
    Queue,
    Steer { expected_turn_id: AgentTurnId },
}
```

```http
POST /projects/{project_id}/agents/{project_agent_id}/agent-runs
POST /agent-runs/{run_id}/agents/{agent_id}/composer-submit
GET  /agent-runs/{run_id}/agents/{agent_id}/mailbox
POST /agent-runs/{run_id}/agents/{agent_id}/mailbox/resume
POST /agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/promote
PUT  /agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}/move
DELETE /agent-runs/{run_id}/agents/{agent_id}/mailbox/messages/{message_id}
```

Runtime `/runtime/commands` 承载 interrupt、compaction、interaction 等即时控制；
Composer input使用Mailbox route。

## 3. Contracts

- launch 先写 LifecycleRun/LifecycleAgent/AgentFrame 与 execution profile intent，再 materialize
  当前 Complete Agent、创建 source，最后把 stable association写回 LifecycleAgent。
- ProjectAgent Draft launch只建立可读取、可订阅的 Product/Agent target。首条用户输入在目标页
  建立 history/live baseline后进入同一 Mailbox composer intake，因而首条与 follow-up共享
  durability、source identity和Queue/Steer语义。
- Mailbox以 `run_id + agent_id` 为 owner。每条消息保存 payload、origin、source identity、
  delivery/barrier/drain mode、priority/order、claim lease和 concrete delivery evidence。
- Queue在 idle 时提交新Turn，在active时保持Pending；显式Steer携带匹配active turn的
  `expected_turn_id`，只在当前owner允许Steer时调用 concrete `AgentCommand::Steer`。
- `handoff_id/mailbox_message_id`证明 Product 已接收；`operation_receipt`证明 concrete Agent
  已接纳。调用方分别保存这两类证据，不以其中一个推断另一个。
- dispatcher按owner获取lease，以 message派生稳定Agent effect identity。execute结果未知时
  inspect同一identity；`NotApplied`才重派，`Unknown`保留恢复状态。
- Companion先形成Channel delivery intent，再以稳定delivery identity materialize Mailbox。
  Routine、Workflow、Canvas和human response使用同一个 Product input delivery port。
- conversation snapshot来自 concrete Agent authoritative read；Mailbox与waiting items作为
  独立Product projection展示，不并入Agent history。
- live stream直接订阅 concrete Agent process-local batches；Mailbox worker的正确性来自
  PostgreSQL scan/lease与authoritative read，live terminal只用于低延迟唤醒。
- list/workspace/delete先读取Product shell。AgentFrame history与association保存在
  LifecycleAgent owner-local JSONB；Dash/Codex history不进入Product document。

## 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| target不存在或跨Project | side effect前 not found/forbidden |
| input为空或client command id非法 | bad request |
| Queue到达active turn | durable Pending |
| Steer缺少/错配expected turn | typed stale；不创建Steer effect |
| Agent unavailable | envelope保持queued/blocked并可恢复 |
| duplicate source identity且payload一致 | 返回同一Mailbox message |
| duplicate source identity但payload不同 | typed conflict |
| dispatcher lease被其它owner持有 | 本worker跳过该AgentRun |
| execute response unknown | 保持consuming/unknown并inspect同一effect |
| Complete Agent service/source不可用 | Product shell与Mailbox仍可读 |
| live stream gap/disconnect | 客户端重读snapshot；worker继续按durable scan运行 |

## 5. Good / Base / Bad Cases

- Good：active turn期间用户按Enter，消息立即显示Pending；terminal后dispatcher按顺序提交下一Turn。
- Good：Ctrl/Cmd+Enter携带当前turn id，Native在safe boundary消费Steer，authoritative history
  保留submission kind与source。
- Base：Agent暂时离线，列表和Mailbox仍可读；恢复后同一message/effect identity继续投递。
- Bad：producer直接调用concrete Submit/Steer会跳过Product durability、source dedup与顺序承诺。

## 6. Tests Required

- launch测试覆盖Product facts → Agent create → association commit；Draft首条输入使用Mailbox route。
- Mailbox PostgreSQL测试覆盖source dedup、owner lease、顺序claim、pause/resume、move、delete、
  expired claim recovery与accepted receipt恢复。
- command测试覆盖idle Queue、active Queue、explicit Steer、stale expected turn和Submit/Steer不互换。
- producer测试覆盖Channel/Companion/Routine/Workflow/Canvas均生成稳定Mailbox source。
- list/workspace测试在Agent resolve/read失败时仍返回Product shell与Mailbox。
- frontend service/component测试覆盖Queue/Steer request差异、Waiting/Steer/Pending、promote、
  recall/retry、reorder、delete、resume及错误详情。

## 7. Wrong vs Correct

```rust
// Wrong: producer直接选择concrete命令，Product无法恢复交付承诺。
complete_agent.execute(AgentCommand::Steer { content, expected_turn_id }).await?;

// Correct: producer声明Product delivery intent，由Mailbox durable materialize并调度。
mailbox.accept(AgentRunMailboxIntakeCommand {
    delivery_intent: AgentRunMailboxDeliveryIntent::Steer { expected_turn_id },
    content,
    ..command
}).await?;
```
