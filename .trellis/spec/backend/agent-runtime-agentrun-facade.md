# AgentRun Product / Agent Facade

## 1. Scope / Trigger

本规范适用于 AgentRun launch、input、fork、surface rebind、conversation read/live stream、
workspace/list/delete 与 Lifecycle/Companion/Routine/Workflow 对 Agent 的调用。修改 Product
command、association、AgentFrame 或 presentation query 时必须复核。

Facade 的职责是组合 Product shell 与 concrete Agent authority，不保存一套“Product Runtime
执行状态”。

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
    pub operation_receipt: ManagedRuntimeOperationReceipt,
}

pub trait AgentRunProductInputDeliveryPort {
    async fn deliver(
        &self,
        command: DeliverAgentRunProductInput,
    ) -> Result<AgentRunProductInputDelivery, AgentRunProductInputDeliveryError>;
}
```

```rust
pub enum AgentRunProductRuntimeSnapshotObservation {
    Absent { requested_target: AgentRunTarget },
    Current {
        product_binding: AgentRunProductRuntimeBinding,
        snapshot: ManagedRuntimeSnapshot,
    },
}
```

`AgentRunProductProjectionGateway::runtime_snapshot` 从 binding 解析 service/source，调用
`CompleteAgentService::read` 并即时 normalize；它不读取 Runtime projection repository。

## 3. Contracts

- launch 先写 LifecycleRun/LifecycleAgent/AgentFrame 与 execution profile intent，再 materialize
  当前 Complete Agent，创建 source，最后把 stable association 写回 LifecycleAgent owner
  document。
- `runtime_thread_id` 是 Product/Agent 桥接坐标；concrete source coordinate 仍由 Agent owner。
- input handoff 是同步合同。`handoff_id` 从 target + client command id 确定性派生；成功返回
  concrete operation receipt，不存在 queued 结果。
- Product 不提供离线输入队列。Agent unavailable 直接返回 typed error，调用者使用同一 client
  identity 重试。
- Companion、Routine、Workflow 与 human response 都调用同一个
  `AgentRunProductInputDeliveryPort`。其 owner-local document可以保存 handoff/operation
  coordinate，但不能建立 mailbox lifecycle。
- list/workspace/delete 先读取 Product shell。Agent snapshot 是 optional enrichment；
  service/source read 失败不得让 Product shell、lineage、title、subject 或删除能力整体失败。
- conversation snapshot 每次来自 concrete Agent authoritative read。`waiting_items` 来自
  LifecycleGate 等 Product owner，和 Agent history在 response 组合，不合并为 mailbox。
- live stream 直接订阅 concrete Agent source 的 process-local events。断线重连重新请求
  conversation snapshot，不依赖 Runtime durable cursor。
- AgentFrame history与 association保存在 LifecycleAgent owner-local JSONB；Dash/Codex history
  不进入 Product document。
- binding digest只 attests Product association document 本身。它不包含 Host generation、
  applied surface、Agent revision 或 availability，也不与这些值做跨 owner equality gate。
- surface rebind 编译当前 immutable AgentFrame intent并通过当前 Host route交给 concrete Agent。
  applied evidence由 Agent receipt/inspection证明，不另建 Product snapshot table。

## 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| target 不存在或跨 Project | side effect 前 not found/forbidden |
| input 为空或 client command id 非法 | bad request |
| Agent unavailable | typed unavailable；无 pending Product row |
| duplicate client command | 返回同一 Agent effect/operation receipt |
| 相同 client id 不同 payload | typed conflict |
| list item 无 association | 返回 Product item，Agent presentation absent |
| list item Agent read 失败 | 返回 Product item，Agent presentation unavailable |
| binding 指向非 owner AgentFrame | Product document conflict |
| live stream gap/disconnect | 客户端重读 snapshot |
| delete Product owner | 删除 Product 局部 document；concrete Agent 按自己的生命周期关闭/删除 |

## 5. Good / Base / Bad Cases

- Good：Project Agent launch 创建 owner-local frames/association；首条输入被 Agent 接收后返回
  receipt，前端同时收到 live delta。
- Base：Codex/Dash 暂时离线，列表仍展示 AgentRun shell；重新连接后 snapshot enrichment恢复。
- Bad：List 因 Runtime projection stale 返回错误。List 只需要 Product facts，Agent view是可选
  enrichment。
- Bad：Companion 把同步输入命名为 mailbox 并保存 queued/claim/settlement。下游 Agent receipt
  已经是唯一接收证据。

## 6. Tests Required

- launch tests 覆盖 Product facts → Agent create → association commit，及 Create applied 后回包
  丢失时的同 effect inspection。
- input tests 覆盖 deterministic handoff、accepted receipt、duplicate、payload conflict 与
  unavailable 零持久化。
- list/workspace tests 注入 binding missing、service resolve failure、Agent read failure，
  断言 Product shell仍返回。
- conversation tests 覆盖 Agent history + LifecycleGate waiting items，且 contract没有 mailbox。
- stream tests 覆盖 live delta 和 disconnect → authoritative snapshot。
- Companion/Routine/Workflow tests 断言统一 input handoff port 与 owner-local receipt。
- migration tests 断言 frames/association owner-local，Product schema没有同步 input handoff 的
  独立 queue/receipt/global binding tables。

## 7. Wrong vs Correct

```rust
// Wrong: 在 Agent 接收前返回 queued 并承诺后台投递。
let message = mailbox.enqueue(draft).await?;
Ok(Queued(message.id))

// Correct: 当前请求完成 concrete Agent handoff。
let receipt = product_input_delivery.deliver(input).await?;
Ok(Accepted(receipt.operation_receipt))
```

```rust
// Wrong: Agent enrichment 失败清空整个列表。
let runtime = projection.runtime_snapshot(&target).await?;

// Correct: Product item先成立，Agent presentation按可用性补充。
let runtime = projection.runtime_presentation_snapshot(&target).await.ok().flatten();
```
