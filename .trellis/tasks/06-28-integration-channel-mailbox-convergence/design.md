# Routine 单会话与 Companion Mailbox 收束设计

## Recommended Direction

短期主线从 Host Integration 自定义信道中拆出来，集中收束两个已经散落且可落地的入口：

1. Routine 单会话 / reuse 模式触发已有 AgentRun。
2. Companion sub / parent / human 交互中需要让某个 AgentRun 继续处理的消息。

Mailbox 保持 per-AgentRun durable inbox 与 scheduler，不升级为全局 channel broker。RoutineExecution 与 LifecycleGate 继续作为各自业务事实源；AgentRunMailboxMessage 作为投递事实源。

本任务包含 mailbox source / envelope 的可拓展重建模。目标不是继续给 `MailboxMessageSource` 追加 enum variant，而是把“来源身份”和“调度策略”拆开：来源身份用于审计、projection、dedup、correlation 和未来 channel/integration 接入；调度策略继续由 mailbox delivery/barrier/drain_mode 决定。

## Code Evidence

- Routine 已在 `LifecycleAgentReuseResolver` 中解析 reusable run/agent/frame，但 `RoutineExecutor::execute_with_dispatch` 最终仍统一调用 `LifecycleDispatchService::execute_subject`。
- Mailbox application service 已有 `accept_user_message_for_target`，可以面向已解析 AgentRun target 创建 durable message、claim command receipt、选择 idle/running/paused delivery policy。
- Mailbox 当前使用 closed `MailboxMessageSource` enum 和 migration check constraint 表达来源，已经出现 `canvas_action` 代码/迁移 drift；这说明 source 模型需要先从 closed enum/check constraint 中解耦。
- `companion_request target=sub` 当前在 `CompanionChildDispatchService::dispatch_child` 后直接调用 `launch_command_with_outcome`。
- `companion_respond` 会依次尝试 resolve parent request gate、resolve hook pending action、complete child result to parent，且这些副作用可以同时命中。
- `CompanionGateControlService` 已有 parent request open/resolve、child result complete、human gate respond 的 durable gate 流程；缺口在于 delivery 仍主要是 runtime notification。
- `target=platform` 当前返回 missing broker error，尚未形成可 materialize 的投递事实；该错误路径是边界保护，直到 platform broker 能先产生 durable request fact。

## Routine Convergence

### Current Flow

```text
RoutineTriggerProvider / cron / webhook
  -> RoutineExecutor::fire_routine
  -> RoutineExecution
  -> LifecycleDispatchService::execute_subject
  -> new/reused runtime session launch path
```

### Target Flow

```text
Routine trigger
  -> RoutineExecution
  -> resolve dispatch target
  -> if target is new run/agent:
       lifecycle creation creates AgentRun anchors
       initial input enters mailbox as first message
     if target is existing run/agent:
       AgentRunMailboxService::accept_message_for_target(source_identity=routine trigger)
```

### Ownership

- `RoutineExecution` owns trigger source, payload, rendered prompt, entity key, execution status, and dispatch refs.
- `AgentRunMailboxMessage` owns delivery to the target AgentRun.
- `LifecycleDispatchService` owns creation or reuse of AgentRun anchors, not queued message scheduling.

### Behavior

- `DispatchStrategy::Fresh`: lifecycle creation remains the anchor creation path; first prompt should align with existing ProjectAgent initial mailbox semantics.
- `DispatchStrategy::Reuse`: creates a mailbox message against the reused run/agent delivery runtime instead of launching directly.
- `DispatchStrategy::PerEntity`: first entity run behaves like Fresh; subsequent entity reuse behaves like Reuse.

## Companion Convergence

### Current Flow

```text
companion_request target=sub
  -> CompanionChildDispatchService
  -> lifecycle dispatch / gate
  -> direct launch_command_with_outcome(child session)

companion_request target=parent
  -> open parent-owned LifecycleGate
  -> runtime notification to parent

companion_request target=human
  -> open current-agent LifecycleGate
  -> runtime notification to current session / browser

companion_respond
  -> resolve parent request gate and/or pending action and/or child result gate
  -> runtime notification to parent/child/current session
```

### Target Flow

```text
companion_request target=sub
  -> create/select child AgentRun + gate if wait/review is needed
  -> child mailbox message(source_identity=companion dispatch, correlation=dispatch_id/gate_id)

child companion_respond
  -> resolve child-owned LifecycleGate
  -> parent mailbox message(source_identity=companion result, correlation=dispatch_id/gate_id)
  -> optional child mailbox event/source projection for local acknowledgement

companion_request target=parent
  -> open parent-owned LifecycleGate
  -> parent mailbox message(source_identity=companion parent request, correlation=gate_id)

parent companion_respond
  -> resolve parent-owned LifecycleGate
  -> child mailbox message(source_identity=companion parent response, correlation=gate_id)

companion_request target=human
  -> open current-agent LifecycleGate
  -> human-visible request notification / UI projection

human gate respond
  -> resolve LifecycleGate
  -> requesting AgentRun mailbox message(source_identity=companion human response, correlation=gate_id)
```

LifecycleGate remains the durable coordination fact for waiting, review, response validation and parent/child/human correlation. It should not be the only delivery mechanism for messages that an AgentRun must process.

### Platform Boundary

`target=platform` currently has no broker implementation, so the current behavior remains the missing broker diagnostic for `capability_grant_request`. The target state is:

```text
companion_request target=platform
  -> platform broker / permission grant service creates durable request fact
     (for capability grants this is the PermissionGrant aggregate)
  -> broker applies policy / user approval / runtime capability effect through its own fact/outbox
  -> broker response materializes mailbox message only when an AgentRun needs to continue
```

The request fact owns broker state, permission policy and audit. The mailbox message is a continuation fact for an AgentRun that must process the broker result, using source identity such as `namespace=platform`, `kind=permission_grant_response`, `source_ref=permission_grant_id`, and a correlation ref to the originating companion request or tool call. Platform broker responses that only update permission/runtime capability state complete through the broker fact and capability transition outbox.

This keeps platform capability work from becoming another runtime notification side channel while acknowledging that the broker itself is a separate prerequisite. Runtime notifications may still project UI visibility, but they are derived from durable broker or mailbox facts rather than being the delivery authority.

## Mailbox Source Model

Target source model should be open enough for built-in sources, Routine, Companion and future channel/integration adapters without changing domain enums for every new route.

Recommended source identity shape:

```text
MailboxSourceIdentity
  namespace: "core" | "routine" | "companion" | future integration/channel key
  kind: stable string, e.g. "composer", "trigger", "dispatch", "result", "parent_request"
  source_ref: optional durable fact id, e.g. routine_execution_id / gate_id / dispatch_id
  correlation_ref: optional cross-message correlation id
  actor: user | system | routine | agent | human | platform
  route: optional route metadata, e.g. sub / parent / human / platform_boundary
  display_label_key: frontend/backend projection label key
  metadata_json: source-specific structured facts
```

The mailbox scheduler must not branch on source identity for delivery semantics. Delivery remains driven by `origin`, `delivery`, `barrier`, `drain_mode`, priority and runtime state. Source identity is for fact attribution, dedup, correlation, projection and future adapter governance.

Current enum values should be migrated into this shape:

- `composer` -> `namespace=core`, `kind=composer`
- `canvas_action` -> `namespace=core`, `kind=canvas_action`
- `routine_executor` -> `namespace=routine`, `kind=trigger`
- companion paths -> `namespace=companion`, `kind=dispatch/result/parent_request/parent_response/human_response`
- platform broker continuations -> `namespace=platform`, `kind=permission_grant_response`, `source_ref=permission_grant_id`

Each Routine / Companion mailbox message should carry:

- correlation id: `routine_execution_id`, `dispatch_id`, `gate_id`, `request_id`
- route metadata: `sub`, `parent`, `human`, `platform_boundary`
- origin actor: routine, child agent, parent agent, human, platform broker
- preview
- retained payload policy
- accepted refs after delivery

Human request itself is UI-facing, not AgentRun-facing; the AgentRun-facing mailbox message is the human response. Parent request is AgentRun-facing because parent Agent must process the request.

## Dedup And Recovery

- Duplicate Routine trigger dedups through `RoutineExecution` and mailbox source dedup key.
- Duplicate companion dispatch dedups through `dispatch_id` / gate correlation.
- Duplicate child result, parent response, and human response dedup through `gate_id`.
- Duplicate platform broker continuations dedup through the durable broker request id, e.g. `permission_grant_id`, plus the originating companion/tool correlation.
- Running target AgentRun queues according to mailbox barrier policy.
- Failed/interrupted target AgentRun keeps pending mailbox messages paused or blocked according to existing mailbox policy.
- Gate resolve followed by mailbox delivery failure must leave durable evidence for retry or operator inspection, instead of only logging a warning.

## Projection

Workspace mailbox/status should show Routine and Companion messages in the same projection as user pending messages. Source labels should distinguish:

- Routine trigger
- Companion dispatch
- Companion result
- Companion parent request
- Companion parent response
- Companion human response

Promote/delete/resume should remain mailbox commands. Companion and Routine should not introduce another pending queue.

## Later Split

Agent custom channels, IM group binding, external source subscription, and integration-defined subscriptions belong to the long-term custom channel task. This task prepares the AgentRun mailbox intake boundary those later channels will reuse.
