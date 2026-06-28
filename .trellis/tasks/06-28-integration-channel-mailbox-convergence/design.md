# Routine 单会话与 Companion Mailbox 收束设计

## Recommended Direction

短期主线从 Host Integration 自定义信道中拆出来，集中收束两个已经散落且可落地的入口：

1. Routine 单会话 / reuse 模式触发已有 AgentRun。
2. Companion sub / parent / human 交互中需要让某个 AgentRun 继续处理的消息。

Mailbox 保持 per-AgentRun durable inbox 与 scheduler，不升级为全局 channel broker。RoutineExecution 与 LifecycleGate 继续作为各自业务事实源；AgentRunMailboxMessage 作为投递事实源。

## Code Evidence

- Routine 已在 `LifecycleAgentReuseResolver` 中解析 reusable run/agent/frame，但 `RoutineExecutor::execute_with_dispatch` 最终仍统一调用 `LifecycleDispatchService::execute_subject`。
- Mailbox application service 已有 `accept_user_message_for_target`，可以面向已解析 AgentRun target 创建 durable message、claim command receipt、选择 idle/running/paused delivery policy。
- `companion_request target=sub` 当前在 `CompanionChildDispatchService::dispatch_child` 后直接调用 `launch_command_with_outcome`。
- `companion_respond` 会依次尝试 resolve parent request gate、resolve hook pending action、complete child result to parent，且这些副作用可以同时命中。
- `CompanionGateControlService` 已有 parent request open/resolve、child result complete、human gate respond 的 durable gate 流程；缺口在于 delivery 仍主要是 runtime notification。
- `target=platform` 当前返回 missing broker error，尚未形成可 materialize 的投递事实。

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
       AgentRunMailboxService::accept_user_message_for_target(source=routine_executor)
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
  -> child mailbox message(source=companion_dispatch, correlation=dispatch_id/gate_id)

child companion_respond
  -> resolve child-owned LifecycleGate
  -> parent mailbox message(source=companion_result, correlation=dispatch_id/gate_id)
  -> optional child mailbox event/source projection for local acknowledgement

companion_request target=parent
  -> open parent-owned LifecycleGate
  -> parent mailbox message(source=companion_parent_request, correlation=gate_id)

parent companion_respond
  -> resolve parent-owned LifecycleGate
  -> child mailbox message(source=companion_parent_response, correlation=gate_id)

companion_request target=human
  -> open current-agent LifecycleGate
  -> human-visible request notification / UI projection

human gate respond
  -> resolve LifecycleGate
  -> requesting AgentRun mailbox message(source=companion_human_response, correlation=gate_id)
```

LifecycleGate remains the durable coordination fact for waiting, review, response validation and parent/child/human correlation. It should not be the only delivery mechanism for messages that an AgentRun must process.

### Platform Boundary

`target=platform` currently has no broker implementation. The target state is:

```text
companion_request target=platform
  -> platform broker / permission grant service creates durable request fact
  -> broker response materializes mailbox message only when an AgentRun needs to continue
```

This keeps platform capability work from becoming another runtime notification side channel while acknowledging that the broker itself is a separate prerequisite.

## Mailbox Envelope Direction

Short-term domain may continue using enum sources. Candidate source values:

- `routine_executor`
- `companion_dispatch`
- `companion_result`
- `companion_parent_request`
- `companion_parent_response`
- `companion_human_response`

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
