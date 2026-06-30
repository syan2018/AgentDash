# Lifecycle gate launch owner convergence design

## Architecture Direction

本任务把三组剩余业务视为同一条控制面 owner 收束链，而不是三组并行 refactor：

```text
canonical LaunchCommand
  -> FrameLaunchEnvelope
  -> LaunchPlan

LifecycleGateResolver
  -> GateTransitionOutcome
  -> delivery / notification adapters

LifecycleDispatchService facade
  -> orchestration starter
  -> runtime materializer
  -> relation/gate writer
  -> reducer bridge
```

顺序固定为 D4 -> D3 -> D2。原因是 D2 拆出的 owner 会消费 launch command 和 gate opening 的最终形态；如果先拆 D2，会把旧 DTO 和旧 gate payload 语义固化到新 owner 中。

## D4. Canonical Launch Command

### Boundary

`agentdash-application-ports` 拥有 canonical launch intent model。AgentRun、RuntimeSession、FrameConstruction 只消费或构造该模型，不再各自定义同构 command/source/modifier。

### File Ownership

D4 的更干净收法是把 launch intent 从 envelope port 中独立出来，形成明确的 ports namespace：

```text
crates/agentdash-application-ports/src/
  launch/
    mod.rs
    command.rs
    modifier.rs
  frame_launch_envelope.rs
```

文件职责：

- `launch/command.rs`：`LaunchCommand`、`LaunchSource`、`LaunchPromptInput`、`LaunchPlanningInput`、source constructor helpers、`reason_tag`。
- `launch/modifier.rs`：`LaunchModifier` 和 source-specific payload，例如 Companion、Routine、LocalRelay、HookAutoResume。
- `launch/mod.rs`：只做 re-export，不承载业务逻辑。
- `frame_launch_envelope.rs`：只保留 `FrameLaunchEnvelopeRequest`、`FrameLaunchEnvelope`、surface、trace、port、accepted launch commit 类型；不再定义 command/source/modifier。

RuntimeSession 的 `session/launch` 目录只保留 runtime launch pipeline 职责：planner、plan、preparation、orchestrator、commit、ingestion、service。`session/launch/command.rs` 不应作为 canonical command 的薄壳保留；其中 `LaunchCommandOutcome` 这类 RuntimeSession result 类型应移到 `service.rs` 或专门的 `outcome.rs`。

AgentRun 侧同理不保留 command model；只在 runtime boundary 或 source adapter 中调用 ports 的 constructor helper。

### Contract Shape

建议在 ports 的 `launch` namespace 中定义：

```rust
pub struct LaunchCommand {
    pub source: LaunchSource,
    pub prompt: LaunchPromptInput,
    pub follow_up_session_id: Option<String>,
    pub identity: Option<AuthIdentity>,
    pub modifiers: Vec<LaunchModifier>,
}

pub struct LaunchPromptInput {
    pub input: Option<Vec<UserInputBlock>>,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: Option<AgentConfig>,
}

pub struct LaunchPlanningInput {
    pub backend_selection: Option<BackendSelectionInput>,
}
```

`LaunchCommand` 只表达 source intent。`LaunchPlanningInput` 表达 planner-only override，首先承载 `backend_selection`。FrameConstruction 不拥有 backend placement 决策。

`FrameLaunchEnvelopeRequest` 应形如：

```rust
pub struct FrameLaunchEnvelopeRequest {
    pub runtime_session_id: String,
    pub command: LaunchCommand,
    pub runtime_trace_state: RuntimeTraceLaunchStateRef,
    pub had_existing_runtime: bool,
    pub requested_runtime_commands: Vec<RuntimeCommandRecord>,
    pub agent_needs_bootstrap: bool,
}
```

`LaunchPlanningInput` 不进入 `FrameLaunchEnvelopeRequest`，除非 frame construction 后来确实需要记录 planner-only trace；默认由 RuntimeSession launch entry 直接传给 planner。

### Data Flow

```text
source adapter
  -> LaunchCommand + LaunchPlanningInput
  -> FrameConstructionService builds FrameLaunchEnvelope
  -> LaunchPlanner consumes command + planning input + envelope
```

`FrameLaunchEnvelopeRequest` 直接携带 canonical `LaunchCommand`。RuntimeSession launch orchestrator 不再调用 `to_frame_launch_command`，AgentRun bridge 不再构造 RuntimeSession-local command，FrameConstruction 不再把 `FrameLaunchCommand` 反向转回 application command。

### Import And Re-export Rules

- 生产代码从 `agentdash_application_ports::launch::{...}` 引入 launch intent 类型。
- `agentdash_application_ports::frame_launch_envelope` 不 re-export launch command 类型，避免 envelope port 重新变成 launch namespace。
- 不新增 `launch_command.rs`、`launch_modifier.rs` 等 ports 根目录平铺文件。
- 不在 AgentRun 或 RuntimeSession 中通过 `pub use` 伪装成本地 command owner；call site 应显式看见 command 来自 ports。
- 测试 helper 可以放在对应模块的 `#[cfg(test)]` 区域；跨 crate fixture 不在 production module 中新增平铺 helper 文件。

## D3. Shared LifecycleGateResolver

### Boundary

`LifecycleGateResolver` owns durable gate validation and transition. Mailbox delivery and session notification are adapter-owned side effects driven by typed intents.

### Contract Shape

```rust
pub enum LifecycleGateCommand {
    RespondHuman { gate_id, payload },
    OpenParentRequest { child_runtime_session_id, turn_id, payload, wait },
    ResolveParentRequest { request_id, parent_runtime_session_id, resolved_turn_id, payload },
    CompleteChildResult { child_runtime_session_id, resolved_turn_id, request_id, payload },
}

pub struct GateTransitionOutcome {
    pub gate: LifecycleGate,
    pub transition: GateTransitionKind,
    pub delivery_intents: Vec<GateDeliveryIntent>,
    pub notification_intents: Vec<GateNotificationIntent>,
}
```

Resolver 输出 durable gate fact 和 delivery/notification intents。Adapters 执行 delivery 并返回 mailbox refs 或 diagnostics；这些结果不再作为 gate transition 的状态事实写回 delivery status blob。

### Data Flow

```text
Companion / Workflow HumanGate command
  -> LifecycleGateResolver
  -> GateTransitionOutcome
  -> Companion context resolver if needed
  -> Mailbox delivery adapter
  -> Session notification adapter
```

Workflow HumanGate 使用同一 resolver 做 transition。Companion facade 可以暂留，但 facade 只编排，不拥有 payload mutation policy。

## D2. LifecycleDispatchService Owner Split

### Boundary

`LifecycleDispatchService` remains the public use-case facade. Internal services own distinct write responsibilities:

- `RunOrchestrationStarter`
- `AgentRuntimeMaterializer`
- `SubjectAssociationWriter`
- `LifecycleRelationWriter`
- `OrchestrationReducerBridge`

### Data Flow

```text
dispatch facade
  -> start or resolve run/orchestration
  -> materialize agent/runtime/frame/anchor
  -> write subject association
  -> write lineage and gate
  -> submit NodeStarted reducer event
  -> persist updated run
  -> return AgentRuntimeRefs / DispatchFacts
```

Graph-backed dispatch must preserve one coordinate through every step:

```text
orchestration_id + node_path + attempt
```

This coordinate is the join key for materialized runtime refs, `RuntimeSessionExecutionAnchor`, reducer event, ready queue mutation, and returned refs.

## Migration And Compatibility

- This PR is still pre-release research work; do not preserve old public DTO models as compatibility fallback.
- No database migration is planned unless implementation discovers a real persisted field change.
- Generated frontend contracts are updated only if Rust contract generation requires it.

## Risk Controls

- Each stage has a focused static grep gate and focused compile gate.
- Do not start D3 until D4 static gate passes.
- Do not start D2 until D3 static gate passes.
- Prefer thin compatibility facades only while moving call sites within the same stage; no long-lived duplicate model remains at stage completion.
