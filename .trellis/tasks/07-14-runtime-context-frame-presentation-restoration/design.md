# Agent Runtime ContextFrame 平台投影恢复设计

## 1. 问题定义

当前 Agent 工具可调用，但 ContextFrame 不再进入会话流。两者表面都来源于 AgentFrame surface，实际却被 07-10 重构拆成了两条不对称链路：

```text
工具链（仍工作）
AgentFrame / CapabilityState / VFS
  -> RuntimeToolProvider.build_tools
  -> DriverToolSurface
  -> NativeRuntimeTool proxies
  -> Agent Core.set_tools
  -> Host callback
  -> Platform ToolBroker

ContextFrame 链（已断）
AgentFrame / product context facts
  -X-> Business Agent Surface context projection
  -X-> Managed Runtime presentation journal
  -> AgentRun context read projection（只剩消费者）
  -> session feed / frontend（只剩消费者）
```

工具 schema 的执行注入和 ContextFrame 的会话展示本来就不是同一载荷。正确关系是“由同一份 surface facts 编译出的两个输出”，而不是让 ContextFrame 驱动工具可用性。

## 1.1 强制参考仓库

本任务的行为 oracle 固定为：

```text
path:   D:/Projects/AgentDash-main-reference
branch: main
commit: 957fa9d60ea3d67efa1bb278fe5b376cf0c34598
mode:   read-only reference
```

实施顺序必须是“定位参考实现 → 提取规则/golden → 在新模块中重建 → 做 payload diff”，禁止根据 UI 截图或当前残余 consumer 反推需求。

参考实现约束外部行为，不约束内部物理结构。旧 `application-runtime-session` 的业务规则可以搬运，mega-module、进程内 notice queue、connector 反查和 stringly builder API 不搬运。

## 2. 现状证据

### 2.1 规划中的目标边界

- `07-10/.../target-crate-shape.md`：Business Agent Surface 位于 `agentdash-agent-runtime::surface`，编译 ContextRecipe、InstructionPlan、ToolCatalogRevision、WorkspaceRequirement 与 HookPlanSnapshot。
- `workstreams/03-business-agent-surface/prd.md`：要求把 context frame selection/delivery plan 从旧 runtime-session 迁出。
- `workstreams/02-managed-runtime-kernel/prd.md`：要求 Hook 产生的 ContextFrame 与 effect 进入 Runtime journal/outbox。
- `workstreams/08-agentrun-cutover/prd.md`：只有在上述迁移完成后才删除 `application-runtime-session`。

### 2.2 当前实现的断点

- `agentdash-agent-runtime::surface::AgentSurfaceCompiler` 存在，但生产 composition 没有调用。
- `agentdash-api::bootstrap::AgentFrameNativeSurfaceCompiler` 在 API 层重新读取 AgentFrame、组装工具、Hook 与 Driver surface。
- 该 compiler 的 context blocks 为空，ContextRecipe 只包含 revision/provenance，未承载 main-reference 的 ContextFrame producer。
- `CanonicalRuntimeSurfaceAdopter` 提交 `SurfaceAdopt` 时使用空 presentation。
- Runtime `context.rs` 拥有 checkpoint/head/compaction 状态机，但没有 ContextFrame projection。
- `agentdash-application-agentrun::context_projection` 与前端 session reducer/UI 仍能消费 ContextFrame。
- 全仓库没有实际 `context_frame` payload producer。

### 2.3 main-reference 的原始职责

旧 `application-runtime-session` 同时包含：

- identity/user/environment/guidelines/memory/assignment/pending-action/compaction frame builders；
- runtime capability/tool/skill/VFS/MCP delta frame builder；
- turn launch 时的 bootstrap frame selection/order；
- live runtime context transition；
- `emit_context_frame` 会话持久化。

旧模块边界过深且耦合，但其中的业务规则不能随 crate 删除。正确迁移应拆分规则归属，而不是恢复 mega-module。

## 3. 方案比较

### 方案 A：在 Native adapter 补发 ContextFrame

拒绝。

- Native 不拥有 AgentFrame/product facts。
- Codex/Remote 会继续缺失。
- 会把平台 presentation 与 vendor execution 再次耦合。
- 无法保证 SurfaceAdopt 与 ContextFrame 原子提交。

### 方案 B：只在 CanonicalRuntimeSurfaceAdopter 手工拼 live update frame

不推荐。

- 能快速恢复部分 live UI，但 bootstrap、hook、pending action、compaction 仍缺失。
- API composition 会继续承担业务编译。
- 会形成 initial/live 两套 producer，下一次重构仍会断。

### 方案 C：建立统一 Compiled Agent Surface artifact，并由 Managed Runtime 原子接纳 presentation

推荐。

- Business Agent Surface 对同一组 typed facts 一次编译出 model context、driver surface 与 presentation plan。
- ThreadStart 和 SurfaceAdopt 只负责给 presentation plan 补 canonical coordinates，并随 operation 原子提交。
- Adapter 只消费 driver surface。
- hook/compaction/mailbox 的 ContextFrame 在各自 canonical Runtime mutation 中追加，但复用同一 projection vocabulary 和 builder。
- 可用 main-reference golden 证明 payload 等价。

## 4. 目标对象模型

### 4.1 Feature 模块归属

推荐将本功能组织为一个完整的 `context_projection` feature，而不是散落在 surface、hook、API helper 中：

```text
agentdash-agent-protocol/
  src/backbone/context_frame.rs
    # owned serialized payload + typed PlatformEvent wrapper

agentdash-agent-runtime/
  src/context_projection/
    mod.rs                 # public deep-module facade
    facts.rs               # protocol-neutral source facts
    projector.rs           # pure frame construction
    delta.rs               # previous/target surface diff
    delivery.rs            # order/cache/channel/consumption plan
    identity.rs            # deterministic ID/digest/time inputs
    artifact.rs            # RuntimeSurfacePresentationPlan
  src/surface/
    compiler.rs            # invokes context_projection and tool/hook/workspace compilation
  src/context/
    # checkpoint/head/compaction state; calls context_projection for compaction frame
  src/hook/
    # HookRun state; calls context_projection for durable model-visible effects

agentdash-application-agentrun/
  src/agent_run/context_sources/
    # AgentFrame/product repositories -> AgentContextSurfaceFacts

agentdash-infrastructure/
  # immutable compiled artifact persistence only

agentdash-api/
  # composition only; no ContextFrame builder
```

`context_projection` 是 Managed Runtime 的 deep module：外部只提交 typed facts 或读取 compiled plan，不直接调用具体 frame-family builder。

### 4.2 Owned presentation vocabulary

将下列类型从 `agentdash-spi::hooks` 的偶然所有权迁到 AgentDash-owned presentation contract：

- `ContextFrame`
- `ContextFrameSection`
- `ContextDeliveryMetadata`
- `ContextDeliveryEntry/Plan/Record`
- delivery phase/cache/model channel/agent consumption enums

推荐归属 `agentdash-agent-protocol` 的 platform presentation vocabulary，并增加 typed `PlatformEvent::ContextFrameChanged`。该事件的 `frame` payload 与 main-reference 完全一致；只替换旧 `SessionMetaUpdate { key: "context_frame", value }` wrapper。

`ControlPlaneProjectionChangeReason::ContextFrameChanged` 继续只是 read-model invalidation hint，不代替携带完整 payload 的 presentation event。

#### 4.2.1 相对 main-reference 的类型优化

在保持 JSON payload 等价的前提下：

- `kind: String` 改为 closed `ContextFrameKind` enum；
- `delivery_status/channel/message_role` 改为 typed enum；
- frame/plan/digest 使用 newtype，避免 Thread/Turn/Frame ID 混用；
- `RuntimeEventSource` 迁出 hooks SPI；
- section 继续使用 exhaustive enum，但消除 builder 中的 magic string；
- 序列化仍输出 main-reference 的 snake_case/string 值，前端 payload 不变化。

### 4.3 Typed source facts

Application adapters 提供 protocol-neutral facts，不直接构造最终 ContextFrame：

```rust
struct AgentContextSurfaceFacts {
    source_frame: AgentFrameCoordinate,
    identity: Option<IdentityContextFact>,
    user_context: Option<UserContextFact>,
    environment: Option<EnvironmentContextFact>,
    guidelines: Vec<GuidelineContextFact>,
    assignment: Option<AssignmentContextFact>,
    capability_state: CapabilitySurfaceFact,
    memory: MemorySurfaceFact,
    workspace: WorkspaceSurfaceFact,
    tools: ToolSurfaceFact,
    hooks: HookSurfaceFact,
}
```

这些事实可复用现有 domain/SPI 数据，但最终 presentation ordering、delivery metadata、frame ID/digest 与 delta 规则归 Business Agent Surface。

### 4.4 Compiled surface artifact

统一 compiler 输出：

```rust
struct CompiledAgentSurfaceArtifact {
    source_frame_id: String,
    source_frame_revision: u64,
    snapshot: AgentSurfaceSnapshot,
    driver_surface: MaterializedDriverSurface,
    presentation: RuntimeSurfacePresentationPlan,
    publication: SurfacePublication,
}

struct RuntimeSurfacePresentationPlan {
    digest: String,
    bootstrap_frames: Vec<ContextFrame>,
    adoption_frames: Vec<ContextFrame>,
}
```

- `bootstrap_frames` 在无 previous accepted surface 时生成。
- `adoption_frames` 由 previous accepted AgentFrame/surface 与 target surface 做 deterministic delta 后生成。
- plan 不提前写 Thread/Turn presentation coordinate；coordinate 由接纳该 plan 的 canonical operation 补齐。
- plan 与 driver surface 共享 source revision/digest，禁止分别重编译。

### 4.5 纯 projection 与可重放时间/ID

main-reference 的 `build_context_frame` 在 builder 内直接读取 `Utc::now()`，并由 timestamp 拼 frame ID，不利于重放、golden 与 operation idempotency。

新 projector 接收显式 `ContextProjectionClock` / `ContextProjectionIdentity`：

```rust
struct ContextProjectionIdentity {
    operation_id: RuntimeOperationId,
    recorded_at_ms: i64,
    source_frame_id: AgentFrameId,
    ordinal: u32,
}
```

- projector 是纯函数，不访问时钟、数据库、connector 或全局 queue；
- 对同一 operation/source facts 重算得到相同 frame IDs、顺序与 digest；
- ID 的序列化形状保持 main-reference 可观察语义，必要的动态字段由 golden normalization 明确定义；
- rendered text 与 sections 从同一 typed fact projection 生成，避免两套拼装规则漂移。

### 4.6 Delta 推理模型

main-reference 在多个调用点分别计算 capability/tool/skill/memory delta。新模块先构造规范化 `ContextSurfaceState`，再做一次 typed diff：

```rust
struct ContextSurfaceDelta {
    capability_keys: SetDelta<CapabilityKey>,
    tool_paths: SetDelta<ToolPath>,
    tool_schemas: SetDelta<ToolSchemaIdentity>,
    mcp_servers: SetDelta<McpServerKey>,
    skills: SetDelta<SkillIdentity>,
    memory: MemoryInventoryDelta,
    vfs: VfsSurfaceDelta,
    companions: SetDelta<AgentIdentity>,
    assignment: Option<AssignmentRevisionDelta>,
}
```

projection rules只消费该 delta；空 delta 由类型层统一判定，不再由每个调用点各自猜测是否发 frame。

## 5. 数据流

### 5.1 初始 ThreadStart

```text
Application source adapters
  -> Business Agent Surface compiler
  -> CompiledAgentSurfaceArtifact
  -> Provisioner 持久化 immutable surface artifact + binding reference
  -> 首次 send_message 读取 bootstrap presentation plan
  -> RuntimeCommandEnvelope {
       presentation: user submission + bootstrap ContextFrames,
       command: ThreadStart
     }
  -> Managed Runtime 单一 UoW
  -> journal / outbox / session feed
```

不能在 provision 时提前发 ContextFrame，因为当时 canonical Runtime Thread/Turn 尚未被首条用户输入建立。

### 5.2 live SurfaceAdopt

```text
current accepted surface + target AgentFrame
  -> unified compiler
  -> driver surface + adoption ContextFrame delta
  -> store immutable materialization payload
  -> RuntimeCommandEnvelope {
       presentation: adoption ContextFrames,
       command: SurfaceAdopt
     }
  -> Managed Runtime 单一 UoW
  -> outbox dispatch target surface to bound driver
```

空 delta 不产生 ContextFrame。重复 operation 通过同一个 plan digest/idempotency key 回放，不重复 event。

### 5.3 Hook、pending action 与 auto-resume

- Hook evaluator/adapter 返回 typed completion/effect，不返回 UI JSON。
- Runtime Hook orchestration 将模型可见 context effect 投影成 ContextFrame，并与 HookRun/effect transaction 同交。
- next-turn pending/auto-resume frame 随对应 mailbox/TurnStart operation 提交。
- silent observer/drop disposition 不产生 durable ContextFrame。

### 5.4 Compaction

- 只有平台 managed compaction head activation 能产生 `compaction_summary` ContextFrame。
- Native/Codex opaque compaction telemetry 不产生该 frame，也不推进 platform context head。
- frame 与 checkpoint/head activation 使用同一 Runtime UoW。

## 6. 持久化与 revision

### 6.1 Surface artifact

扩展 Runtime composition repository，按 `(binding_id, surface_revision, surface_digest)` 持久化：

- materialized driver surface；
- presentation plan及其 digest；
- source AgentFrame coordinate。

不要只按 binding 覆盖当前 surface，否则 retry/recovery 无法证明某次 operation 使用了哪一版 presentation plan。

### 6.2 Revision 语义

- Runtime thread revision：任何 durable journal mutation（含 ToolCall lifecycle、ContextFrame presentation）均推进。
- Context revision：只由 materialized context/head 变化推进。
- Surface revision：只由 AgentFrame/surface adoption 推进。
- Transient tool progress、token delta、MCP progress 不推进 durable revision。
- 工具实现是否“无状态”不决定 ToolCall item 是否持久化；会话中已经展示的调用生命周期属于 transcript state。

## 7. main-reference 等价策略

先从 main-reference 提取而不是重新发明：

- ContextFrame payload fixtures；
- 各 frame family 的 section 构造规则；
- delivery phase/order/cache/model channel；
- bootstrap frame 顺序；
- live delta 与空 delta 规则；
- compaction/pending action/auto-resume 触发条件。

建立两个 oracle 层：

1. builder golden：相同 typed facts 得到相同 ContextFrame payload；
2. stream golden：相同业务场景得到相同 payload 序列，允许 wrapper/coordinate 映射。

前端只新增 typed wrapper 的 normalization；进入现有 reducer 后的数据必须与 main-reference 一致。

## 8. 风险与控制

### 风险 1：把 ContextFrame 当成模型 context 本体

控制：MaterializedContext/ContextRecipe 是模型事实；ContextFrame 是其可审计 presentation。两者同源但不能相互反序列化替代。

### 风险 2：一次性搬回旧 mega-module

控制：按 source fact、surface compiler、runtime projection、transport normalization 拆分；禁止恢复 SessionRuntimeInner 或 connector 反查。

### 风险 3：只修 Native

控制：adapter conformance test 断言所有 adapter 都不引用 ContextFrame builder/AgentFrame repository。

### 风险 4：bootstrap plan 在首条消息前丢失

控制：compiled surface artifact durable store；ThreadStart 从 binding 的 exact surface digest 读取 plan。

### 风险 5：surface 已采用但 presentation 缺失

控制：presentation 作为 RuntimeCommandEnvelope 输入，与 SurfaceAdopt projection/journal 同 UoW；失败时不接受 operation。

### 风险 6：前端行为再次漂移

控制：不改 session feed grouping/UI；只在 protocol normalization boundary 适配 typed wrapper，并运行 main-reference golden。

## 9. 确定范围

本任务恢复全部 ContextFrame producer family，而不是仅 live surface update：

- surface bootstrap/adoption 是一个 compiler；
- hook/pending/compaction 使用相同 owned vocabulary 与 Runtime UoW；
- 全部消费者当前仍存在；
- 局部恢复会重新制造多事实源和下一轮迁移债务。

任务保持单一主任务，不拆 child；执行时用工作项和逐工作项提交控制范围。
