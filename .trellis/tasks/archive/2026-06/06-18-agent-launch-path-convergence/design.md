# Agent 启动路径主轴收束 Design

## Architecture Intent

目标结构分为两层：

```text
Agent materialization
  -> durable AgentRun / Lifecycle / RuntimeSession / AgentFrame / Gate / Lineage facts
  -> mailbox or launch adapter submits user input
  -> Agent turn launch
  -> FrameConstructionService builds launch envelope
  -> LaunchPlan / PreparedTurn / ConnectorAcceptedTurn
```

materialization 回答“这个 agent/run/frame/runtime 是否存在、归属是谁、是否有 gate/lineage/orchestration anchor”。turn launch 回答“这一轮输入如何进入 connector”。两者不能互相偷职责。

## Proposed Boundaries

### AgentMaterializationService

新增或重塑现有 `LifecycleDispatchService`，让它成为唯一 agent materialization 权威。它应覆盖：

- ProjectAgent draft start / plain AgentRun。
- Workflow AgentCall node。
- Companion child agent。
- Routine subject execution。
- Interaction gate / wait companion。

输出统一 refs：

```text
run_id
agent_id
frame_id
runtime_session_id
optional orchestration binding
optional gate_id
optional subject association
```

如果现有 `LifecycleDispatchService` 足以承载，应直接增强它；如果命名与职责已经偏斜，应改名或抽出新的 service，避免继续让多个 launcher 重复创建事实。

### LaunchCommand And Modifier

`LaunchCommand` 保留来源、用户输入、identity、follow-up trace 等通用字段。来源差异进入 typed modifier：

- `ProjectAgentOwnerModifier`
- `LifecycleNodeModifier`
- `CompanionModifier`
- `RoutineModifier`
- `LocalRelayModifier`
- `HookAutoResumeModifier`
- `RuntimeCommandOverlayModifier`

modifier 不直接启动 connector，只贡献 frame construction 所需 facts、prompt/executor override、context slice、gate/return-channel、terminal effect binding 或 runtime overlay。

### FrameConstructionService

当前 `classify` 是互斥 route。目标是改为：

```text
resolve owner surface composer
  -> apply modifiers in deterministic order
  -> close FrameLaunchSurface
  -> produce FrameLaunchEnvelope
```

owner surface composer 只应回答 owner 基线：

- ProjectAgent owner surface。
- Lifecycle node owner surface。
- Existing frame surface。

Companion 不应是最高优先级 route。Companion 是 child agent 的 modifier：它引用 parent、slice mode、dispatch prompt、selected ProjectAgent、adoption/return channel。

### AgentRun Mailbox

mailbox 保持 durable command 投递层。ProjectAgent start 的首条消息和 workspace composer submit 都进入 mailbox，由 scheduler 决定：

- idle -> launch turn。
- running + steering supported -> steer。
- running + barrier -> queue。
- resume source -> launch source command。

不得把这些判定搬回 route handler。

## Deletion Targets

### Workflow AgentCall Independent Launcher

`workflow/orchestration/agent_node_launcher.rs` 当前自行创建 agent/session/frame/anchor。应改为调用统一 materialization service，并只保留 orchestration-ready node 解析、executor policy 校验和 `NodeStarted` event materialization。

### Companion Tool Inline Launch Orchestration

`companion/tools.rs` 的 `execute_sub_request` 应拆掉厚逻辑：

- tool 层保留 payload 校验、roster 校验、hook trigger 调用和 service 调用。
- companion dispatch service 负责构造 `InteractionDispatchIntent` / `AgentLaunchIntent` 或统一 materialization request。
- child launch input 以 `CompanionModifier` 进入 frame construction。

### LaunchCommand Source-Specific Fields

`local_relay_mcp_servers`、`local_relay_workspace_root`、`companion_hint`、`routine_hint` 等迁入 typed source payload / modifier。删除横向 optional 字段，避免继续膨胀。

## Data Flow

```text
API / tool / scheduler / relay
  -> Source adapter
  -> AgentMaterializationRequest + Vec<LaunchModifier>
  -> materialization refs
  -> mailbox or direct LaunchCommand
  -> SessionLaunchOrchestrator
  -> FrameConstructionService(owner + modifiers)
  -> LaunchPlan
  -> Connector prompt
```

ProjectAgent draft start 是特殊的 two-receipt shell，但不是独立 launch path：

```text
project_agent_start receipt
  -> materialize AgentRun workspace
  -> initial mailbox message
  -> scheduler outcome
```

## Compatibility And Migration

项目未上线，不做旧入口兼容。若数据库事实需要调整，直接添加 migration 并同步 domain / infrastructure / contracts。旧代码路径迁移后删除，不保留 dead wrappers。

## Risks

- Companion 涉及 hook、gate、task assignment、ProjectAgent binding 和 parent/child notification，拆分时必须保证 durable wait 语义不丢。
- Workflow AgentCall node 当前只 materialize NodeStarted，是否应立即投递 turn 需要按现有 orchestration contract 核对。
- `FrameConstructionService` 改为 modifier pipeline 后，modifier 顺序必须确定，避免 VFS/MCP/capability closure 变得不可审计。

## Validation Strategy

- 后端 focused tests 覆盖 service 层，不做大范围 e2e。
- grep 检查生产代码中直接创建 `LifecycleAgent` / `RuntimeSessionExecutionAnchor` / launch frame 的路径是否只剩权威 service。
- `cargo check` 覆盖 Rust 类型收束。
- 前端只在 DTO 或 route 变化时跑相关 service/store tests。
