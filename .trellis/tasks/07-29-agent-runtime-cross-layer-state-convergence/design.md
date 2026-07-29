# Agent Runtime 状态面与控制面收束设计

## 设计原则

- **seam不等于第二套事实语言**：Complete Agent callable interface保留，但与Product Runtime共用
  一个contract crate和canonical nested facts。
- **深 module 优先**：复杂的 source read、projection、coherence、terminal fold 藏在小 interface 后。
- **interface 即测试面**：一致性和恢复从 interface 验证，不穿透到每个 mapper/route。
- **一次 authority cutover**：项目未上线，不保留旧 DTO、双轨、fallback 或 compatibility facade。

## 已确认架构决策

2026-07-29已确认保留Complete Agent与Product Runtime两个aggregate wrapper。二者只表达不同
identity/state-machine边界；attached Product state嵌入canonical `AgentObservation`，不得复制、
覆写或维护另一份execution/context/interaction事实。

## Current

```text
Complete Agent source
  -> AgentSnapshot / AgentContextSnapshot
  -> Runtime projector
  -> AgentRuntimeView
  -> Product projection gateway
  -> HTTP route
  -> generated Agent Service + Runtime contracts
  -> Session component request/fence/state
  -> Context inspector

Agent history
  -> Turn terminal + Item event + item-specific status
  -> Session reducer
  -> card registry再组合推理
```

问题在于两条链上的中间层都需要理解 owner 语义。

## Target

```text
Native / Codex / Remote Complete Agent adapters
                  │
                  ▼
  Managed Runtime Contract
  ├─ canonical observation/context/control facts
  ├─ CompleteAgentService + source wrappers
  └─ Product Runtime wrappers
                  │
                  ▼
       RuntimeObservation deep module
       - resolve/read/reconcile
       - source/coordinate validation
       - normalized projection
       - context lower-bound read
          │                 │
          ▼                 ▼
 AgentRuntimeView   AgentRuntimeContextProjection
          └──────────┬──────┘
                     ▼
         Product gateway / HTTP adapter
               pure target routing
                     ▼
      Frontend RuntimeProjectionController
       - target key / abort / generation
       - loading / ready / refreshing / error
       - committed coordinate
                     ▼
                Session UI
```

```text
Agent source history
        ▼
CanonicalConversationProjector
  item_started
  item_updated(progress only)
  item_terminal(outcome, error, timestamps)
        ▼
Session feed fold
  CanonicalItemPresentation
  { item, lifecycle }
        ▼
Card renderer
```

## Module 0 — Unified Managed Runtime Contract

### Physical Cut

将`agentdash-agent-service-api`的command/context/snapshot/profile/service/surface/live等module移动到
`agentdash-agent-runtime-contract`，删除前者crate。现有Runtime command/view/gateway继续位于同一
crate的Product-facing module。

不增加`agentdash-agent-observation`等第三个shared crate。

### Canonical Facts

```rust
pub struct AgentObservation {
    pub revision: AgentObservationRevision,
    pub lifecycle: AgentLifecycleStatus,
    pub execution: AgentExecutionSnapshot,
    pub context: AgentContextCoordinate,
    pub control_availability: AgentControlAvailabilityMap,
    pub interactions: Vec<AgentInteractionSnapshot>,
    pub thread_name: Option<AgentThreadNameSnapshot>,
    pub authority: AgentSnapshotAuthority,
    pub fidelity: SemanticFidelity,
    pub conversation: Vec<CanonicalConversationRecord>,
}

pub struct CompleteAgentSnapshot {
    pub source: AgentSourceCoordinate,
    pub observation: AgentObservation,
    pub applied_surface: Option<AppliedAgentSurface>,
    pub initial_context: Option<AppliedInitialContextEvidence>,
}

pub struct AgentRuntimeView {
    pub thread_id: RuntimeThreadId,
    pub observation: AgentObservation,
    pub presentation_evidence: AgentRuntimePresentationEvidence,
}
```

精确字段可按现有consumer调整，但必须满足：

- canonical facts只定义一次；
- source identity只存在Complete Agent wrapper；
- browser只接收Product wrapper；
- Product binding与未绑定/不可用状态留在application层
  `AgentRunProductRuntimeViewObservation`，不写入Agent observation；
- `AgentRuntimeView`的revision就是canonical source observation revision，不再创建同值
  `RuntimeProjectionRevision`别名；Product owner自己的document revision留在Product aggregate；
- Product lifecycle operation receipt/admission不伪装成Complete Agent发布的source control事实；
- attached wrapper中的observation必须与同revision Complete Agent observation逐值一致；
- 不使用serde transcode连接两套同构类型。

不能把`AgentRuntimeView`直接替换成`CompleteAgentSnapshot`：Product在source出现前已经存在
provisioning/operation/binding状态，但这些状态已经由Product use case与外层observation表达；
browser也不能接收source coordinate与Agent-private evidence。保留安全wrapper是语义需要，
把provisioning再塞进该wrapper或平铺复制observation都不是。

identity按domain收束：

| Domain | 处理 |
| --- | --- |
| source turn/item/interaction | 统一为一组Agent identity，Runtime presentation直接复用 |
| source effect与Product operation | A0证明是否为同一业务identity；相同则统一，不同则显式关联 |
| Agent source与Product thread | 保留两个类型 |
| source observation revision | canonical observation/context使用同一类型 |
| Product owner document/provisioning revision | 留在Product aggregate，不进入`AgentRuntimeView` |
| surface revision | 若bound/applied引用同一surface ledger则统一，否则显式区分ledger |

context同样拆成canonical recipe与wrapper：

```rust
pub struct AgentContextRecipe {
    pub coordinate: AgentContextCoordinate,
    pub contributions: Vec<AgentContextContribution>,
}

pub struct CompleteAgentContextSnapshot {
    pub source: AgentSourceCoordinate,
    pub recipe: AgentContextRecipe,
}

pub struct AgentRuntimeContextProjection {
    pub thread_id: RuntimeThreadId,
    pub recipe: AgentContextRecipe,
}
```

## Module 1 — RuntimeObservation

### Seam

位于 `agentdash-agent-runtime`。复用统一contract内的`CompleteAgentService` seam，不创建第二个抽象port。
Product gateway 在解析 durable binding 后，把 resolved service/source 交给该 module。

### Proposed interface

精确命名可在实现期调整，interface 必须保持以下语义：

```rust
pub struct AgentRuntimeObservationTarget<'a> {
    pub thread_id: RuntimeThreadId,
    pub source: AgentSourceCoordinate,
    pub service: &'a dyn CompleteAgentService,
}

impl AgentRuntimeObservationTarget<'_> {
    pub async fn read_view(&self) -> Result<AgentRuntimeView, AgentRuntimeObservationError>;

    pub async fn read_context(
        &self,
        requirement: AgentRuntimeContextRequirement,
    ) -> Result<AgentRuntimeContextProjection, AgentRuntimeObservationError>;

    pub async fn reconcile_live(
        &self,
        event: AgentLiveEvent,
    ) -> Result<AgentRuntimeUpdate, AgentRuntimeObservationError>;
}
```

`AgentRuntimeContextRequirement` 使用 Runtime coordinate，不暴露 source type：

```rust
pub struct AgentRuntimeContextRequirement {
    pub at_least: AgentRuntimeContextCoordinate,
}
```

`AgentRuntimeContextProjection` 是 Runtime-owned DTO，包含 target/thread coordinate、snapshot coordinate
和 contributions。Runtime module 内部完成：

1. Runtime revision → source revision query；
2. source identity 校验；
3. snapshot revision lower-bound；
4. equal revision 时 context revision/recipe digest/authority/fidelity 一致性；
5. source DTO → Runtime DTO projection。

Product/API 不再访问上述字段。

### Error model

错误至少区分：

- target/source unavailable；
- source snapshot behind required coordinate；
- same-revision coordinate mismatch；
- source identity mismatch；
- unsupported/observed-only context fidelity；
- malformed source projection；
- live gap/re-read required。

HTTP adapter只映射 typed Runtime error，不重做判断。

## Module 2 — Frontend RuntimeProjectionController

### Seam

位于 `features/agent-run-runtime/model`，作为 hook/controller 暴露给 Session。它消费 generated
Runtime contract，不消费 Agent Service API。

### Interface

```ts
type RuntimeContextProjectionState =
  | { status: "idle" }
  | { status: "loading"; targetKey: string }
  | { status: "ready"; targetKey: string; projection: AgentRuntimeContextProjection }
  | { status: "refreshing"; targetKey: string; projection: AgentRuntimeContextProjection }
  | { status: "error"; targetKey: string; previous: AgentRuntimeContextProjection | null; error: Error };
```

controller 接收 current Runtime context coordinate，负责：

- target change 清空旧 owner state；
- coordinate advance 触发 refresh；
- abort/generation 拒绝旧 target 响应；
- committed coordinate 单调前进；
- refresh failure 保留同 target 上一份 ready projection；
- command response 后 authoritative refresh。

Session component 只渲染 state，并把 compaction command availability 交给 action UI。

## Module 3 — Canonical Item Lifecycle

### Invariant

```text
item_started(id)
  -> zero or more item_updated(id, progress)
  -> exactly one item_terminal(id, outcome)
```

`outcome` 为：

```text
succeeded | failed | lost | cancelled
```

terminal 可携带统一 diagnostic 和 completed timestamp。`completed` 只表示生命周期结束。

### Protocol shape

- Backbone notification 使用 AgentDash-owned terminal evidence，不从 Codex event 名推断 success。
- snapshot/read 与 live projection 使用同一 projector/fold，因此 reconnect 结果一致。
- compaction 只保留一个 canonical item shape；source-specific Codex/native DTO 在 adapter 内结束。
- item body 保存 compaction-specific `operation_id/mode/context_revision`，通用 outcome/error/timestamp
  归 lifecycle evidence。

### Frontend fold

Session reducer 输出：

```ts
interface CanonicalItemPresentation {
  item: AgentDashThreadItem;
  lifecycle:
    | { status: "running"; started_at_ms: number }
    | {
        status: "terminal";
        outcome: "succeeded" | "failed" | "lost" | "cancelled";
        completed_at_ms: number;
        error: RuntimeTerminalDiagnostic | null;
      };
}
```

renderer 不接收 `"started" | "updated" | "completed"` 裸字符串，不读取 Turn terminal 修补 item。

## Module 4 — Transport Encoding Normalization

### Owner

wire encoding spec拥有 carrier规范，例如：

```text
domain RuntimeU64
  <-> JSON decimal string
  <-> TypeScript bigint
```

contract generator不拥有该语义，只把规范机械应用到具体 DTO shape；frontend transport也不重新解释。

### Generator Input

- Rust `JsonSchema` / TS export metadata；
- encoding spec 中声明的 wire scalar marker（例如 `RuntimeU64`）；
- generated contract root types。

### Generated Output

- TypeScript DTO；
- 符合 encoding spec 的 recursive normalization decoder/encoder；
- malformed input diagnostics；
- stable JSON Schema。

### Rules

- `$ref`、object、optional、array、map 与 union 递归展开；
- wire scalar normalization由 encoding kind + schema marker决定，不维护字段路径表；
- enum value由 generated TypeScript union拥有，runtime validator只验证可安全派发的 shape；
- JSON object/definitions/properties canonical sort；union array顺序保留；
- generator self-test覆盖合法、非法与 missing root。

## Module 5 — Test Support And Boundary Proof

### Builders

`agentdash-agent-runtime-test-support` 提供：

- `AgentSnapshotBuilder`；
- `AgentRuntimeViewBuilder`；
- `AgentRuntimeContextProjectionBuilder`；
- canonical item lifecycle scenario builder。

builder 默认生成内部一致的 identity/revision/digest，但调用测试必须显式选择关键场景；production
contract 不增加 `Default` 来迁就 fixture。

### Replace, Don't Layer

- observation interface tests 替换 Product/API 的 coordinate 细节测试；
- API 保留 route/authorization/error mapping 测试；
- adapter 保留 source mapping 与 capability fidelity 测试；
- canonical lifecycle suite 替换每个 renderer 对 event type 的重复断言；
- encoding normalization/generator tests 替换 hand-written nested scalar patch tests。

## Ownership Matrix

| Fact / behavior | Owner module | Adapter/consumer |
| --- | --- | --- |
| canonical execution/context/interaction/control facts | Managed Runtime Contract | Complete Agent、Runtime view、frontend |
| source history/context revision | Complete Agent wrapper | Native/Codex/Remote |
| normalized observation/coherence | RuntimeObservation | Product gateway |
| route/actor/target resolution | AgentRun Product facade | HTTP |
| wire serialization/handshake | Runtime Wire | local/remote transport |
| item lifecycle projection | Canonical Conversation projector | Session feed |
| target-keyed async UI state | RuntimeProjectionController | Session UI |
| carrier/normalized value编码规则 | Runtime Wire encoding spec | contract generator、frontend transport |
| schema/codec机械生成 | contract generator | generated outputs |

## Hard Deletions

- `agentdash-agent-service-api` crate、schema、generated TS与所有Cargo依赖；
- 重复的Runtime execution/context/interaction/control types及serde transcode；
- Product/API 对source context query/snapshot wrapper的直接使用；
- frontend Runtime 路径对 generated Agent Service context DTO 的 import；
- `contextSnapshotFence.ts` 的 UI-local coherence interface；
- compaction 双 discriminator branch；
- terminal `ItemUpdated` 特例与 renderer event-type fallback；
- 绕过 encoding spec 的 handwritten nested scalar field paths；
- 被 deep interface coverage 取代的完整 DTO literals/pass-through tests。

## Change Simulations

### Context coordinate 新增字段

期望只修改 source owner、RuntimeObservation projector 与 generated artifact。Product/API/controller
通过 opaque Runtime coordinate 传递，不新增字段级逻辑。

### 新增 maintenance item

source projector声明 body并使用通用 lifecycle terminal；Session reducer与card shell无需新增终态推理。

### 新增 nested wire integer

只改 Rust contract/schema并引用既有 encoding kind，normalization codec自动生成；若 generator
不支持该结构，contract check在提交前失败。

## 风险

- canonical item terminal 是 protocol hard cut，必须同步 Native/Codex/read/live/frontend，不可拆成双轨。
- Runtime context DTO cutover 会影响 generated contracts；先建立 characterization，再一次删除旧 Agent
  Service frontend contract。
- encoding/generator 改动可能造成一次性大 diff；先固定 semantic digest，再 canonicalize，确保不是
  丢定义。
- 若 implementation 证明某 pass-through 层拥有额外 Product policy，应保留该行为，但移动到具名
  Product interface，不塞回 RuntimeObservation。
