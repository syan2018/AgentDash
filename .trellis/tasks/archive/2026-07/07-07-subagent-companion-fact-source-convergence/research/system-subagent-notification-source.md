# System / SubAgent Notification Source Cleanup

## Scope

This slice traces AgentDash project-native Agent / SubAgent / Companion / system notification ingress. The concrete live sample was a `<subagent_notification>...</subagent_notification>` payload surfaced by the current host/tool layer as if it were a user message. AgentDash must treat that shape as system/subagent-origin notification material at its conversation/model-context ingress boundary, not as human composer input.

## Ingress Chain

### Companion mailbox wake

Current AgentDash chain before this slice:

```text
LifecycleGate result
  -> GateMailboxWakeIntent
  -> build_parent_result_mailbox_input_text(...)
  -> AgentRunCompanionMailboxDelivery::deliver_child_result_to_parent(...)
  -> AgentRunMailboxService::accept_intake_message(origin=Companion, source=companion/result)
  -> mailbox scheduler consumes envelope
  -> launch/steer path projects UserInputSubmitted
```

The mailbox row already had structured `MailboxSourceIdentity` (`namespace`, `kind`, `source_ref`, `correlation_ref`, `actor`, `route`, `metadata`) and `MailboxMessageOrigin::Companion`. The leak was downstream: scheduler/model-context projection treated the envelope payload as ordinary user input when committing `UserInputSubmitted`.

### AgentDash launch commit

Current AgentDash chain before this slice:

```text
LaunchCommand
  -> LaunchPlanner resolved payload
  -> Connector accepted turn
  -> TurnCommitter::commit_accepted_launch_events(...)
  -> build_user_input_submitted_envelope(...)
```

This was correct only for human-origin launch sources. Companion/system/hook/workflow/routine continuations needed a source-aware projection instead of reusing the human input event.

### Host/tool-layer subagent notification sample

The `<subagent_notification>` sample can arrive at AgentDash as text-shaped ingress from an upstream host/tool boundary. Even when the upstream envelope is text-shaped, AgentDash has enough information at launch commit time to prevent it from becoming a `UserInputSubmitted` fact: marker-shaped project subagent notification text is classified as `system_message` with `kind=subagent_notification`.

## Implemented Boundary Fix

- `LaunchSource` now has an internal `SystemDelivery` variant for AgentDash system-origin continuations.
- `LaunchPlan` / `PreparedTurn` carry `LaunchSource` to the accepted launch commit boundary.
- `TurnPreparer` converts non-human launch delivery, including marker-shaped `<subagent_notification>` text, into a `system_delivery` `ContextFrame` for connector system context. The connector prompt payload is reduced to a short runtime continuation instruction so the original notification body is not consumed as ordinary human text.
- `TurnCommitter::commit_accepted_launch_events` now emits `UserInputSubmitted` only for human-input launch sources (`HttpPrompt`, `LifecycleAgentUserMessage`, `LocalRelayPrompt`) and only when the text is not the project subagent notification marker shape.
- Companion/system/hook/workflow/routine launch sources emit `PlatformEvent::SessionMetaUpdate { key="system_message", value={...} }` with bounded `kind`, `origin`, `source`, `status`, `summary`, and `turn_id`.
- Mailbox steering keeps `UserInputSubmitted` only for `MailboxMessageOrigin::User`.
- Non-user mailbox delivery emits a `system_message` platform projection with `origin`, `source`, `delivery_kind`, `status`, `summary`, and bounded refs (`mailbox_message_id`, `turn_id`, `gate_id`, `correlation_ref`, `delivery_runtime_session_id`).

## Remaining Boundary

This repository can stop AgentDash conversation/feed/model-context facts from recording project subagent/system notifications as human input. It cannot change how the external host/tool layer initially displays the live sample before AgentDash receives it. The integration point for that outer layer is the same discriminant: subagent/system delivery must enter AgentDash as system/subagent-origin metadata or be normalized at `TurnCommitter`/mailbox ingress before `UserInputSubmitted` is considered.
