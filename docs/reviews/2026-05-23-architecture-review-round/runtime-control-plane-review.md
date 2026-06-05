# AgentDash Runtime 控制平面架构静态 Review

## 整体判断

AgentDash 的核心架构方向是清楚的：它不是一个普通 “chat app”，而是围绕 **Agent Runtime 控制平面** 在做空间化编排，核心概念包括：

```text
Project / Story / Task
        ↓
SessionBinding
        ↓
SessionConstructionPlan
        ↓
LaunchCommand
        ↓
ExecutionContext
        ↓
AgentConnector
        ↓
Agent Runtime / Local Relay
        ↓
BackboneEnvelope Event Stream
        ↓
UI / Persistence / Lifecycle
```

我觉得当前代码已经把几个重要的长期概念立起来了：`VFS Mount`、`ExecutionContext`、`BackboneEnvelope`、`SessionBinding`、`AgentConnector`、`MountProvider`、`RepositorySet`。这些都是后面演进成平台型系统的关键抽象。

主要风险也比较集中：**composition root 过重、Session pipeline 过长、Relay/Local 事件时序有潜在问题、crate 分层有倒挂、前后端 DTO 与状态层有膨胀趋势**。这些不是“代码写得差”，更像是预研项目进入平台化前的典型拐点。

---

## 1. 主模块梳理

### 后端 crate 结构

| 模块                                                       | 当前职责                                                                           | 观察                                                       |
| -------------------------------------------------------- | ------------------------------------------------------------------------------ | -------------------------------------------------------- |
| `agentdash-domain`                                       | 领域实体、value object、repository trait、Project/Story/Session/Workflow/VFS 等核心模型    | 是目前最干净的底层模块，建议继续保持不依赖上层。                                 |
| `agentdash-spi`                                          | Connector、MountProvider、plugin/platform 相关接口                                   | 是跨边界 contract 层，价值很高。                                    |
| `agentdash-agent-protocol`                               | Agent 事件协议 / Backbone 协议                                                       | 已经在前后端共享事件事实层，方向正确。                                      |
| `agentdash-relay`                                        | Cloud ↔ Local relay 协议                                                         | 协议能力完整，但 `protocol.rs` 已经偏大，后面需要拆。                       |
| `agentdash-application`                                  | Session runtime、VFS、Lifecycle、Workflow、Routine、Hook、Capability、Runtime Gateway | 当前是最大业务中枢，也是复杂度最高的地方。                                    |
| `agentdash-executor`                                     | AgentConnector 实现、CompositeConnector、Pi Agent connector 等                      | 执行适配层，但目前和 `application` 互相缠得有点深。                        |
| `agentdash-agent`                                        | 真正的 agent loop、tool execution、streaming、delegate                               | agent runtime 核心，`agent_loop.rs` 偏大。                     |
| `agentdash-api`                                          | Axum API、AppState 组装、Relay WebSocket、routes、DTO                                | 当前承担了 composition root，`AppState::new_with_integrations` 非常重。 |
| `agentdash-infrastructure`                               | Postgres/SQLite repository 实现、migrations                                       | 目前依赖了 `agentdash-application`，这是一个明显的分层倒挂点。              |
| `agentdash-local`                                        | 本机 runtime、relay client、tool executor、MCP、workspace/shell/file 工具              | 是本机 host + runtime adapter + relay adapter 的混合体。         |
| `agentdash-local-tauri`                                  | Tauri 桌面封装                                                                     | 更像桌面入口层。                                                 |
| `agentdash-mcp`                                          | MCP preset / runtime 相关能力                                                      | 和 application、local、relay 都有交集。                          |
| `agentdash-integration-api` / `agentdash-first-party-integrations` | 插件接口和一方插件                                                                      | 插件方向已经存在，但平台边界还可以继续收敛。                                   |

### 前端 package 结构

| 模块                   | 当前职责                               | 观察                                              |
| -------------------- | ---------------------------------- | ----------------------------------------------- |
| `packages/app-web`   | React/Vite 主应用、页面、store、API client | 业务复杂度主要在这里，页面和 Zustand store 已经开始膨胀。            |
| `packages/app-tauri` | 桌面入口                               | 负责把 web 与 local runtime 结合起来。                   |
| `packages/core`      | 前端核心逻辑 / local runtime 暴露          | 可以作为后续抽共享逻辑的位置。                                 |
| `packages/ui`        | 设计系统 / UI primitives               | 分层合理。                                           |
| `packages/views`     | 可复用视图组件                            | 适合沉淀 workspace、directory browser、runtime views。 |

---

## 2. 运行主链路

我把主链路按“云端直接执行”和“Relay 本机执行”两条看了一下。

### 云端 / 普通 Session 执行链

```text
API Route
  ↓
AppState.services.session_launch / session_core
  ↓
LaunchCommand
  ↓
SessionConstructionProvider
  ↓
SessionRequestAssembler / SessionLaunchPlanner
  ↓
ExecutionContext
  ↓
CompositeConnector
  ↓
PiAgentConnector / Agent Runtime
  ↓
ExecutionStream
  ↓
SessionTurnProcessor
  ↓
Persistence + Eventing + UI
```

核心代码集中在：

```text
crates/agentdash-api/src/app_state.rs
crates/agentdash-application/src/session/runtime_builder.rs
crates/agentdash-application/src/session/hub/mod.rs
crates/agentdash-application/src/session/prompt_pipeline.rs
crates/agentdash-application/src/session/assembler.rs
crates/agentdash-executor/src/connectors/composite.rs
crates/agentdash-agent/src/agent_loop.rs
```

### Relay / 本机执行链

```text
Cloud API
  ↓
RelayAgentConnector
  ↓
BackendRegistry.resolve_backend
  ↓
Relay WebSocket command.prompt
  ↓
agentdash-local CommandHandler
  ↓
local SessionRuntime.launch_command
  ↓
local Agent Runtime
  ↓
EventSessionNotification / EventSessionStateChanged
  ↓
Cloud BackendRegistry.feed_session_event
  ↓
RelayAgentConnector ExecutionStream
  ↓
SessionTurnProcessor / UI
```

核心代码集中在：

```text
crates/agentdash-application/src/relay_connector.rs
crates/agentdash-application/src/backend_transport.rs
crates/agentdash-api/src/relay/ws_handler.rs
crates/agentdash-api/src/relay/registry.rs
crates/agentdash-api/src/workspace_resolution.rs
crates/agentdash-local/src/ws_client.rs
crates/agentdash-local/src/handlers/mod.rs
crates/agentdash-local/src/handlers/prompt.rs
```

### VFS 主链

```text
Project / Story / Workspace / Agent Knowledge / Lifecycle
  ↓
build_derived_vfs
  ↓
Mount / MountLink / capabilities
  ↓
MountProviderRegistry
  ↓
RelayFs / Inline / Canvas / SkillAsset / Lifecycle Provider
  ↓
Runtime tools / Agent tools / UI file surfaces
```

核心代码集中在：

```text
crates/agentdash-domain/src/common/mount.rs
crates/agentdash-application/src/vfs/mount.rs
crates/agentdash-application/src/vfs/provider.rs
crates/agentdash-application/src/vfs/relay_service.rs
crates/agentdash-application/src/vfs/tools/fs.rs
crates/agentdash-spi/src/platform/mount.rs
```

这一块是当前架构里比较有长期价值的部分：用 `Mount` 把 workspace、canvas、skill asset、lifecycle artifacts 等统一成 Agent 可访问资源，后面扩展 external provider / remote FS / sandbox FS 会比较自然。

---

## 3. 我看到的亮点

### 3.1 `ExecutionContext` 的拆分方向是对的

`ExecutionContext` 把 session 维度和 turn 维度拆开：

```text
ExecutionSessionFrame:
  executor_config
  working_directory
  env
  mcp_servers
  vfs
  identity

ExecutionTurnFrame:
  hook_session
  capability_state
  runtime_delegate
  restored_session_state
  context_frames
  assembled_tools
```

这个设计很重要。它让 “Who/Where” 和 “How/Control” 分离，后续做 runtime restore、capability hot update、多 connector、多 backend 都有基础。

### 3.2 `VFS Mount` 是一个好抽象

`Mount` / `MountProvider` / `MountLink` 基本已经形成了统一资源语言。Agent 看到的是 mount，不需要知道背后是 workspace、本地文件、canvas、skill asset 还是 lifecycle artifact。

这对 Agent 平台很关键，因为未来真正复杂的不是“读文件”，而是：

```text
这个资源来自哪里？
谁有权限？
是否可写？
写入是否需要 materialize？
是否要经过 lifecycle policy？
是否可被 agent 自动发现？
```

当前 VFS 这条线已经能承接这些问题。

### 3.3 `BackboneEnvelope` 作为事件事实层是合理的

后端 runtime、relay、本机 runtime、前端 UI 都围绕 `BackboneEnvelope` / notification 流来走，这比直接把 UI 状态和 runtime 状态耦死要好很多。

后面可以继续把它发展成：

```text
Runtime Event Fact
  ↓
Projector
  ↓
Session View State / Activity View State / Timeline / Audit
```

现在已经有这个雏形。

### 3.4 `SessionBinding` 很有价值

`SessionBinding` 把 Project / Story / Task / Session 的关系集中起来，避免 session id 散落在各种实体上。这个设计建议保留，甚至可以强化成后续 session ownership 的唯一事实源。

---

## 4. 主要风险与问题

### 4.1 `AppState::new_with_integrations` 已经变成超级 composition root

`crates/agentdash-api/src/app_state.rs` 里 `AppState::new_with_integrations` 做了太多事情：

```text
repository 初始化
plugin 注册
shared library seed
backend registry
mount provider registry
VFS service
connector 构建
session runtime 构建
hook provider
runtime gateway
cron scheduler
auth cleanup
terminal effects replay
stall detector
routine executor
audit bus
```

这让 API 层变成了几乎所有系统的启动容器。短期没问题，长期会有几个副作用：

1. 新增一个业务域时，很容易继续往 `AppState` 里塞。
2. 测试某个 service 时，需要构造大量无关依赖。
3. 运行时依赖顺序靠人工维护。
4. 一些循环依赖通过 `RwLock<Option<...>>` 延迟注入解决，架构上会越来越脆。

比较明显的例子是 `SessionRuntimeBuilder` 先构造 services，然后再回填：

```text
session_construction_provider
hook_effect_handler_registry
context_audit_bus
terminal_callback
```

这说明当前依赖图已经不是纯单向了。

建议后面把它拆成几个 kernel/bootstrap：

```text
RepositoryBootstrap
VfsKernel
RelayKernel
SessionKernel
LifecycleKernel
RoutineKernel
AuthKernel
RuntimeGatewayKernel
```

`AppState` 最后只做组合，而不是亲自知道每个 service 怎么创建。

---

### 4.2 `Session` 模块已经接近平台中枢，需要再分层

`crates/agentdash-application/src/session` 下面的职责非常多：

```text
construction
assembler
launch
launch_service
launch_planner
prompt_pipeline
runtime_builder
hub
turn_processor
turn_supervisor
capability_state
capability_service
hook_delegate
hook_runtime
terminal_effects
effects_service
persistence
```

尤其是 `prompt_pipeline.rs`，它承担了完整 turn 启动流程：

```text
build construction
plan launch
build ExecutionContext
prepare runtime tools
prepare MCP tools
prepare capability frames
activate supervisor turn
apply runtime transitions
emit context frames
call connector.prompt
update meta/runtime commands
spawn title generation
spawn turn processor
spawn stream adapter
```

这条链路是核心，但现在一个文件里混了 plan、prepare、commit、side effects、stream attach。未来一旦增加：

```text
multi-agent
branching session
tool approval replay
runtime migration
local/cloud fallback
activity lifecycle step executor
session resume
```

这里会继续膨胀。

建议拆成几个明确阶段：

```text
SessionConstruction
  负责 owner/project/story/task/context 的组装

TurnPreparation
  负责 capability、tools、MCP、VFS、context frames

ConnectorLaunch
  负责构造 ExecutionContext 并调用 AgentConnector

TurnCommit
  负责 turn started event、meta、runtime command 状态落库

TurnEventIngestion
  负责 ExecutionStream → BackboneEnvelope → persistence/eventing
```

这样每一段可以单独测试，也能让 `SessionRuntimeInner` 逐步瘦身。

---

### 4.3 `agentdash-infrastructure` 依赖 `agentdash-application`，分层倒挂

当前依赖图里有一个比较明显的问题：

```text
agentdash-infrastructure -> agentdash-application
```

这通常说明 application 层里放了一些 infrastructure adapter 必须实现的 port 或 DTO。短期可以接受，长期会让基础设施层反向知道业务 orchestration 细节。

更理想的方向是：

```text
domain
  ↓
spi / ports
  ↓
application
  ↓
composition root

infrastructure implements ports
executor implements connector ports
api composes everything
local composes local runtime
```

也就是：

```text
application 不应该被 infrastructure 依赖
infrastructure 应该只依赖 domain + ports
api/local 作为 composition root 依赖所有 adapter
```

可以考虑抽一个很小的 crate，例如：

```text
agentdash-ports
agentdash-persistence-ports
agentdash-session-ports
```

把 `SessionPersistence`、terminal effect outbox、repository-facing DTO、runtime event persistence 等接口从 `application` 下沉出去。

---

### 4.4 Relay 事件存在一个比较具体的时序风险

在 `crates/agentdash-application/src/relay_connector.rs` 的 `RelayAgentConnector::prompt` 中，当前顺序大致是：

```rust
let _turn_id = self.transport.relay_prompt(&backend_id, payload).await?;

let (tx, rx) = mpsc::unbounded_channel();
self.transport.register_session_sink(session_id, tx);
```

也就是 **先发 command.prompt，等 response.prompt 回来之后，才注册 session sink**。

但是本机 backend 可能在 `response.prompt` 之前或几乎同时就通过 WebSocket 发：

```text
EventSessionNotification
EventSessionStateChanged
```

云端 `ws_handler` 收到事件后会调用 `BackendRegistry.feed_session_event`。如果此时 sink 还没注册，这个事件就会被丢掉。

建议改成：

```text
1. 创建 channel
2. 注册 session sink
3. 发送 relay_prompt
4. 如果 relay_prompt 失败，再 unregister
5. 如果 session terminal / cancel / stream end，再 unregister
```

这属于小改动、高收益的 P0 级问题。

---

### 4.5 `BackendRegistry::unregister` 里的 pending 清理是 no-op

在 `crates/agentdash-api/src/relay/registry.rs` 里：

```rust
self.pending.write().await.retain(|_, _| true);
```

注释说 pending requests 会自然失败，但这里实际上保留了所有 pending sender。因为 `pending` map 本身还持有 `oneshot::Sender`，等待方不会自然收到 `ResponseDropped`，最后大概率只能等超时。

建议把 pending 从：

```rust
HashMap<msg_id, oneshot::Sender<RelayMessage>>
```

调整为：

```rust
HashMap<msg_id, PendingRequest {
  backend_id,
  tx,
}>
```

然后 backend disconnect 时：

```text
remove all pending where backend_id == disconnected_backend_id
```

这样等待中的 `send_command` 可以立即返回 `ResponseDropped`，而不是卡到 timeout。

---

### 4.6 `RelayAgentConnector::list_executors` 存在 sync/async 不匹配

`AgentConnector::list_executors` 是同步方法，但 relay connector 需要异步查询在线 backend，于是用了：

```rust
tokio::task::block_in_place(|| {
    handle.block_on(async {
        transport.list_online_executors().await
    })
})
```

这是一个架构味道：

1. 如果未来跑在 current-thread runtime 或某些嵌套 runtime 场景，容易出问题。
2. executor discovery 本质是 async / dynamic state。
3. `CompositeConnector` 每次刷新 routing 时也会受这个阻塞点影响。

更好的方向有两个：

```text
方案 A：AgentConnector::list_executors 改 async
方案 B：BackendRegistry 维护 online executor snapshot，list_executors 只读快照
```

我更倾向 B。Discovery 本来就是状态投影，没必要每次同步接口里临时 block async。

---

### 4.7 本机 prompt handler 可能重复启动 session event forwarder

在 `crates/agentdash-local/src/handlers/prompt.rs` 里，每次 `handle_prompt` 成功后都会：

```rust
tokio::spawn(async move {
    forward_session_notifications(session_runtime, &sid, &tid, event_tx).await;
});
```

`forward_session_notifications` 又会：

```rust
let mut rx = session_runtime.eventing.ensure_session(session_id).await;
loop { rx.recv().await ... }
```

如果同一个 relay session 多次 prompt / follow-up，每次都启动一个新的 forwarder，就可能造成同一个 session 的事件被多个 receiver 重复转发。这里我没有完整跑 runtime 验证，所以先作为“建议重点核对”的风险点。

建议本机侧维护：

```text
session_id -> forwarder task handle
```

如果已有 forwarder，就不要重复启动；或者把 relay notification forwarding 提升成 session lifecycle 级别，而不是 prompt 级别。

---

### 4.8 `RelayMessage` 协议文件太大，协议域混在一起

`crates/agentdash-relay/src/protocol.rs` 目前聚合了很多类型：

```text
register / ack
ping / pong
prompt / cancel / discover
workspace detect / browse
file read/write/delete/rename/apply_patch
shell exec
materialize
MCP probe/list/call/close
terminal events
session notification/state changed
capabilities changed
```

这说明 relay 已经不是单一协议，而是多条子协议的总线。

建议下一步拆成：

```text
relay/protocol/handshake.rs
relay/protocol/prompt.rs
relay/protocol/workspace.rs
relay/protocol/tool.rs
relay/protocol/mcp.rs
relay/protocol/terminal.rs
relay/protocol/session_event.rs
relay/protocol/capabilities.rs
```

顶层保留：

```rust
pub enum RelayMessage { ... }
```

但具体 payload 类型拆出去。这样不会影响 JSON 协议形态，却能降低维护成本。

---

### 4.9 `RepositorySet` 太宽，service 依赖边界不明显

`RepositorySet` 聚合了大量 repository：

```text
Project
Canvas
Workspace
Story
StateChange
SessionBinding
Backend
RuntimeHealth
AuthSession
Settings
SharedLibrary
LlmProvider
MCP
SkillAsset
ProjectAgent
ProjectVfsMount
Workflow
Lifecycle
Routine
InlineFile
```

优点是 service 拿依赖方便；缺点是任何 service 一旦拿到 `RepositorySet`，它理论上可以访问所有领域数据，边界不明显。

建议按 bounded context 拆小：

```text
ProjectRepos
SessionRepos
VfsRepos
WorkflowRepos
LifecycleRepos
AssetRepos
AuthRepos
RuntimeRepos
```

然后大容器只在 composition root 出现，application service 只接收它真正需要的 repo set。

---

### 4.10 前端状态和 DTO 有膨胀趋势

前端目前有不少大文件：

```text
SettingsPage.tsx
ProjectSettingsPage.tsx
workspace-list.tsx
activity-inspector.tsx
workflowStore.ts
storyStore.ts
workspace-layout.tsx
SessionChatView.tsx
SessionSystemEventCard.tsx
```

这说明前端也进入了平台应用常见阶段：页面、store、service、DTO mapper 都在快速增长。

同时，除了 `generated/backbone-protocol.ts`，很多 API DTO 看起来还是手写 TypeScript 类型和 mapper。后续容易出现：

```text
后端 DTO 改了
前端 mapper 没跟上
运行时才发现字段不一致
```

建议考虑从 Rust schema 生成更多前端类型，比如：

```text
schemars + OpenAPI
ts-rs
typeshare
```

尤其是这些域：

```text
Session
Workflow
Lifecycle
VFS
Routine
ProjectAgent
MCP Preset
SkillAsset
```

---

## 5. 架构演进建议

我建议按优先级分成五步，不要一上来大重构。

---

### P0：先修运行时正确性和可复现性

这些是小改动、高收益。

| 建议                                      | 涉及位置                       | 理由                                  |
| --------------------------------------- | -------------------------- | ----------------------------------- |
| Relay prompt 前先注册 session sink          | `relay_connector.rs`       | 避免本机事件早于 sink 注册导致丢事件。              |
| backend disconnect 时清理 pending requests | `relay/registry.rs`        | 避免命令等待到 timeout。                    |
| 本机 session notification forwarder 去重    | `local/handlers/prompt.rs` | 避免 follow-up 多次 prompt 后重复转发事件。     |
| 增加 `Cargo.lock` / `pnpm-lock.yaml`      | repo root                  | 当前有 git deps，没有 lockfile 会让构建复现性很差。 |
| 加关键链路测试                                 | relay / local / session    | 先锁住协议时序，再做重构。                       |

建议补三类测试：

```text
1. relay backend 在 response.prompt 前发送 notification，云端不应丢事件
2. backend 断连后 pending command 应立即失败，而不是等 30s timeout
3. 同一个 local relay session 多次 prompt，不应重复转发同一事件
```

---

### P1：把 AppState 拆成几个 kernel

当前 `AppState` 太像“上帝对象”。可以先不改外部 API，只把 `new_with_integrations` 内部拆掉。

目标结构可以是：

```rust
struct AppState {
    repos: RepositorySet,
    services: ServiceSet,
    config: AppConfig,
    auth_provider: Arc<dyn AuthProvider>,
}

struct ServiceSet {
    session: SessionServices,
    vfs: VfsServices,
    relay: RelayServices,
    routine: RoutineServices,
    lifecycle: LifecycleServices,
    auth: AuthServices,
    runtime_gateway: RuntimeGatewayServices,
}
```

bootstrap 过程拆成：

```text
RepositoryBootstrap::build(...)
PluginBootstrap::build(...)
VfsKernel::build(...)
RelayKernel::build(...)
SessionKernel::build(...)
RoutineKernel::build(...)
AuthKernel::build(...)
RuntimeGatewayKernel::build(...)
```

这样至少能做到：

```text
AppState 不直接知道所有对象如何创建
每个 kernel 可以有自己的测试
依赖顺序更可见
循环注入点更容易暴露
```

---

### P2：重构 Session Launch Pipeline

建议把当前 `prompt_pipeline.rs` 拆成显式阶段。可以先创建几个内部结构，不急着改变外部行为。

目标可以是：

```rust
struct LaunchConstructionResult { ... }
struct TurnPreparationResult { ... }
struct ConnectorLaunchRequest { ... }
struct StartedTurn { ... }
struct AttachedExecutionStream { ... }
```

流程变成：

```text
LaunchCommand
  ↓
construct_session_context()
  ↓
prepare_turn()
  ↓
build_execution_context()
  ↓
start_connector()
  ↓
commit_turn_started()
  ↓
attach_execution_stream()
```

对应模块：

```text
session/construction
session/turn_preparation
session/connector_launch
session/turn_commit
session/event_ingestion
```

这个重构的收益很大：未来要加 lifecycle activity、resume、branch、multi-agent、tool approval replay 时，不会都挤进一个 pipeline。

---

### P3：处理 crate 分层倒挂

当前最需要处理的是：

```text
agentdash-infrastructure -> agentdash-application
```

建议抽 port crate：

```text
agentdash-ports
```

里面放：

```text
repository-facing ports
session persistence port
terminal effect persistence port
runtime event persistence port
audit persistence port
```

理想依赖方向：

```text
agentdash-domain
agentdash-agent-protocol
agentdash-relay
        ↓
agentdash-spi / agentdash-ports
        ↓
agentdash-application
        ↓
composition roots: agentdash-api / agentdash-local / agentdash-local-tauri

adapters:
  agentdash-infrastructure
  agentdash-executor
  agentdash-first-party-integrations
```

注意这里不是说所有 crate 都只能单向。对于 composition root，例如 `agentdash-api`，它可以依赖全部东西。但 `infrastructure` 这种 adapter 最好不要依赖 application orchestration 层。

---

### P4：Relay 协议和 WS 处理拆域

`api/src/relay/ws_handler.rs` 现在也承担了太多副作用：

```text
backend register
runtime health persistence
command response routing
session notification ingest
terminal event projection
capabilities update
shell output routing
```

建议拆成：

```text
RelayHandshakeService
RelayCommandRouter
RelayResponseRouter
RelaySessionEventIngestor
RelayTerminalProjector
BackendRuntimeHealthProjector
```

WS handler 本身只保留：

```text
read message
parse message
dispatch message
write outbound
handle close
```

这样 relay 后续扩 MCP、terminal、workspace、agent events 时不会继续把一个文件变成巨型 switchboard。

---

### P5：前端按 feature/domain 收拢

前端可以逐步从：

```text
pages/
stores/
services/
types/
components/
```

转向：

```text
features/session
features/workflow
features/lifecycle
features/vfs
features/project
features/settings
features/relay
```

每个 feature 内部放：

```text
api.ts
types.ts
store.ts
components/
views/
mappers.ts
```

再配合后端 schema 生成类型，可以减少 mapper 漂移。

优先拆这几个：

```text
workflowStore.ts
storyStore.ts
SettingsPage.tsx
ProjectSettingsPage.tsx
SessionChatView.tsx
SessionSystemEventCard.tsx
activity-inspector.tsx
```

---

## 6. 值得优先拆的文件 / 区域

这几个是我认为最值得排进技术债 roadmap 的地方：

| 优先级 | 文件 / 区域                                                | 建议                                                                            |
| --- | ------------------------------------------------------ | ----------------------------------------------------------------------------- |
| 高   | `agentdash-api/src/app_state.rs`                       | 拆 bootstrap/kernel，降低 composition root 复杂度。                                   |
| 高   | `agentdash-application/src/session/prompt_pipeline.rs` | 拆 launch 阶段，明确 plan/prepare/start/commit/attach。                              |
| 高   | `agentdash-application/src/relay_connector.rs`         | 修 session sink 注册时序；处理 stream end unregister。                                 |
| 高   | `agentdash-api/src/relay/registry.rs`                  | 修 pending request disconnect 清理。                                              |
| 中高  | `agentdash-application/src/session/hub/mod.rs`         | 继续下沉职责，让 hub 只做 orchestration。                                                |
| 中高  | `agentdash-relay/src/protocol.rs`                      | 按协议域拆 payload 类型。                                                             |
| 中高  | `agentdash-domain/src/workflow/value_objects.rs`       | 按 workflow graph / activity / validation / capability / lifecycle contract 拆。 |
| 中   | `agentdash-application/src/vfs/mount.rs`               | 按 mount source 拆 workspace/canvas/skill/lifecycle/agent knowledge。            |
| 中   | `agentdash-agent/src/agent_loop.rs`                    | 拆 streaming accumulator、tool execution、approval、context compaction。           |
| 中   | 前端大型 page/store                                        | 按 feature folder 和生成 DTO 收敛。                                                  |

---

## 7. 一个比较实际的重构路线

我会按这个顺序推进：

```text
第一阶段：修 correctness
  - relay sink 先注册
  - pending request disconnect 清理
  - local forwarder 去重
  - 加 lockfile
  - 加 relay/local/session 关键测试

第二阶段：拆启动装配
  - AppState::new_with_integrations 拆成 Kernel bootstrap
  - ServiceSet 拆成 SessionServices / VfsServices / RelayServices 等
  - RepositorySet 拆小依赖集，但保留总容器

第三阶段：拆 Session Pipeline
  - prompt_pipeline 拆阶段
  - SessionRuntimeInner 瘦身
  - 延迟注入改成 staged builder 或明确 init graph

第四阶段：清理 crate 分层
  - 抽 ports crate
  - 去掉 infrastructure -> application
  - application 只依赖 SPI / ports，不直接知道过多 executor 细节

第五阶段：前后端契约与协议模块化
  - Relay protocol 拆域
  - API DTO 生成 TS 类型
  - 前端 feature folder 收敛
```

---

## 8. 我最建议马上处理的三个点

如果只选三个，我会先做这几个：

1. **修 `RelayAgentConnector::prompt` 的 session sink 注册顺序**
   这是明确的事件丢失风险，改动小，收益高。

2. **修 `BackendRegistry::unregister` 的 pending no-op**
   当前断连后的命令可能白等 timeout，这会让 UI 和 runtime 表现很奇怪。

3. **把 `AppState::new_with_integrations` 拆成 kernel bootstrap**
   这不是 bug，但它是后续所有架构演进的瓶颈。先拆装配，不动业务行为，风险最低。

整体来看，这个仓库已经不是“缺抽象”，而是抽象很多、系统正在长大。下一步最关键的不是再加新概念，而是把几个核心边界固定住：**Session 构造边界、Runtime 执行边界、Relay 事件边界、VFS provider 边界、Application/Infrastructure 分层边界**。这些边界一旦收稳，后面做 lifecycle workflow、多 backend、本地/云端混合、plugin marketplace 都会顺很多。
