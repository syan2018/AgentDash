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

pub trait LifecycleAgentRepository {
    async fn initialize_title_from_agent(
        &self,
        target: &AgentRunTarget,
        title: &str,
    ) -> Result<bool, DomainError>;
}
```

ProjectAgent Draft 创建与用户输入是两个明确命令：

```http
POST /projects/{project_id}/agents/{project_agent_id}/agent-runs
POST /agent-runs/{run_id}/agents/{agent_id}/composer
```

前者只返回已建立的 `run_id + agent_id + frame_id`；后者才携带
`AgentInputContent[] + client_command_id` 并返回 concrete Agent receipt。

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
- ProjectAgent Draft launch只建立可读取、可订阅的Product/Agent target。首条用户输入在客户端进入
  该target后使用标准composer command同步handoff，原因是live subscriber必须先拥有真实source
  coordinate，才能观察user input → turn start → partial output的完整顺序。
- `runtime_thread_id` 是 Product/Agent 桥接坐标；concrete source coordinate 仍由 Agent owner。
- input handoff 是同步合同。`handoff_id` 从 target + client command id 确定性派生；成功返回
  concrete operation receipt，不存在 queued 结果。
- concrete Agent 首次生成非空 thread name 后，同一次 input handoff 从 authoritative snapshot
  调用 `initialize_title_from_agent` 初始化 `LifecycleAgent.workspace_title`。该更新必须以
  `run_id + agent_id + title absent` 为条件原子执行；此后 AgentRun 标题由 Product 独立持有，
  Agent-native thread name 的后续变化不会覆盖用户修改。
- Product 不提供离线输入队列。Agent unavailable 直接返回 typed error，调用者使用同一 client
  identity 重试。
- Companion、Routine、Workflow 与 human response 都调用同一个
  `AgentRunProductInputDeliveryPort`。其 owner-local document可以保存 handoff/operation
  coordinate，但不能建立 mailbox lifecycle。
- list/workspace/delete 先读取 Product shell。标题只从 `LifecycleAgent.workspace_title` 解析；
  Agent snapshot 是 optional enrichment，service/source read 失败不得让 Product shell、lineage、
  title、subject 或删除能力整体失败。
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
| Agent title 为空 | 不初始化 AgentRun title，保持 pending |
| AgentRun title 已存在 | conditional update 返回 false，保留当前 Product title/source |
| 首次标题持久化失败 | handoff 返回 typed unavailable；调用者以同一 client id 重试，Agent effect 不重复 |
| list item 无 association | 返回 Product item，Agent presentation absent |
| list item Agent read 失败 | 返回 Product item，Agent presentation unavailable |
| binding 指向非 owner AgentFrame | Product document conflict |
| live stream gap/disconnect | 客户端重读 snapshot |
| delete Product owner | 删除 Product 局部 document；concrete Agent 按自己的生命周期关闭/删除 |

## 5. Good / Base / Bad Cases

- Good：Project Agent launch 创建 owner-local frames/association并立即返回target；前端进入该target、
  完成authoritative history baseline后用标准input handoff投递首条输入，同时收到live delta。
- Good：Dash 首次命名写入自身 history 后，input handoff 将同名值仅初始化到 LifecycleAgent；用户
  后续重命名只修改 LifecycleAgent，列表与 workspace 不再依赖 Dash service 可用性。
- Base：Codex/Dash 暂时离线，列表仍展示 AgentRun shell；重新连接后 snapshot enrichment恢复。
- Bad：List 因 Runtime projection stale 返回错误。List 只需要 Product facts，Agent view是可选
  enrichment。
- Bad：Companion 把同步输入命名为 mailbox 并保存 queued/claim/settlement。下游 Agent receipt
  已经是唯一接收证据。

## 6. Tests Required

- launch tests 覆盖 Product facts → Agent create → association commit，Create请求不携带或执行
  Agent input，以及Create applied 后回包丢失时的同 effect inspection。
- input tests 覆盖 deterministic handoff、accepted receipt、duplicate、payload conflict 与
  unavailable 零持久化。
- title tests 覆盖首次非空初始化、空标题忽略、已有 Product title 不覆盖、持久化失败后同 client id
  重试，以及 list/workspace 在 Agent read 失败时仍返回已存标题。
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

```rust
// Wrong: 每次展示都把 Agent-native thread name 当作 AgentRun 标题读穿。
let title = runtime_snapshot.thread_name.unwrap_or_else(|| "新会话".to_owned());

// Correct: 首次命名只初始化一次，之后展示读取 Product-owned LifecycleAgent。
lifecycle_agents
    .initialize_title_from_agent(&target, &snapshot_title)
    .await?;
let title = lifecycle_agent.workspace_title;
```
