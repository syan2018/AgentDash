# Runtime Coordinate Design

## Design Decisions To Preserve

- AgentRun delivery target 是全系统统一控制面事实，不由 workspace、cancel、mailbox、SubjectExecutionView 各自查询和构造。
- AgentRun 应持有或可唯一解析 current delivery binding；binding 指向当前 run / agent / frame / node / attempt / runtime session。
- `RuntimeSessionExecutionAnchor` 是 backlink 与历史证据，不是业务 selection owner。
- repository `latest` 类 API 只能表达 raw ordering，业务 selection 进入 application-level resolver。

## Target Model

```text
LifecycleAgent current delivery binding
  -> runtime_session_id
  -> lifecycle_run_id
  -> lifecycle_agent_id
  -> current_frame_id / launch_frame_id
  -> orchestration_id / node_path / attempt
  -> delivery status and observed_at

DeliveryRuntimeSelectionService
  -> reads AgentRun binding and anchors
  -> applies explicit policy
  -> returns DeliveryRuntimeSelection
```

## RC02 Implementation Contract

`LifecycleAgent` current delivery binding is the persisted AgentRun-facing current runtime coordinate. It should be stored on the agent row, not inferred by workspace, cancel, mailbox or SubjectExecutionView.

First implementation shape:

```rust
pub struct LifecycleAgentCurrentDeliveryBinding {
    pub runtime_session_id: String,
    pub launch_frame_id: Uuid,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
    pub status: DeliveryBindingStatus,
    pub observed_at: DateTime<Utc>,
}

pub enum DeliveryBindingStatus {
    Ready,
    Running,
    Terminal,
    Lost,
    FrameMissing,
    DeliveryMissing,
}
```

`run_id`, `agent_id` and `current_frame_id` remain owned by `LifecycleAgent`; selection output includes them without duplicating them inside the persisted binding object.

`DeliveryRuntimeSelectionService` is application-owned. It consumes repositories and returns an explicit read model instead of exposing repository `latest` as a business selector:

```rust
pub enum DeliveryRuntimeSelectionPolicy {
    CurrentDelivery { run_id: Uuid, agent_id: Uuid },
    RunScopedLatest { run_id: Uuid, agent_id: Option<Uuid> },
    LaunchPrimary { run_id: Uuid, agent_id: Uuid },
    SubjectLatestObserved { subject: SubjectRef },
}

pub struct DeliveryRuntimeSelection {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub current_frame_id: Uuid,
    pub launch_frame_id: Uuid,
    pub runtime_session_id: String,
    pub orchestration_id: Option<Uuid>,
    pub node_path: Option<String>,
    pub node_attempt: Option<u32>,
    pub status: DeliveryBindingStatus,
    pub observed_at: DateTime<Utc>,
    pub address: AgentRunRuntimeAddress,
    pub message_stream: MessageStreamProjectionRef,
    pub anchor: RuntimeSessionExecutionAnchor,
}
```

Typed selection failures include missing run/agent, missing current delivery, incomplete binding, missing/mismatched anchor, and missing current/launch frame. Consumers must handle these failures directly; they must not silently fall back to raw anchor latest.

## Storage Decision

- current delivery binding 持久化在 `LifecycleAgent` 粒度，原因是 AgentRun workspace identity 是 `run_id + agent_id`，且该粒度已经承载 `current_frame_id`。
- `LifecycleRun` 不承载 current delivery binding，原因是同一 run 可以存在多个 agent 和多个 delivery target。
- 第一版不新增独立 binding 表；需要完整 delivery history 时从 `RuntimeSessionExecutionAnchor` 和后续 SubjectExecutionView history 派生。
- binding status 使用 delivery / runtime projection 语义，包含 `ready | running | terminal | lost | frame_missing | delivery_missing` 等面向 AgentRun 的状态。

## Policy Surface

| Policy | Meaning | Consumers |
| --- | --- | --- |
| CurrentDelivery | AgentRun 当前控制面目标 | workspace, mailbox, cancel |
| RunScopedLatest | 同一 run 内最近 delivery 证据 | transition / diagnostics |
| LaunchPrimary | launch 时 primary anchor | history / trace baseline |
| SubjectLatestObserved | subject execution history 的 latest 派生 | SubjectExecutionView |

## Implementation Shape

- 先为 `LifecycleAgent` 增加 current delivery binding 字段和 repository roundtrip。
- 再设计并测试 selection service，不直接大规模重写所有 consumers。
- 第二步把 workspace / cancel / mailbox 迁到 service。
- 第三步扩展 SubjectExecutionView history，并从同一 history 派生 latest。
- 第四步让 resource surface DTO 表达 surface source coordinate。

## Dependencies

- Capability exposure surface 会影响 current frame VFS 和 runtime surface 刷新，但不阻塞 delivery binding 的 owner 决策。
- Control Surface 中 cancel / command policy 的实现应依赖本任务输出的 selection service。

## RC02 Scope Boundary

RC02 implements persistence, roundtrip and the selection service. It does not migrate workspace/cancel/mailbox consumers, does not add SubjectExecutionView history DTOs, and does not add AgentRun resource-surface browser contract fields. Those remain RC04-RC08.
