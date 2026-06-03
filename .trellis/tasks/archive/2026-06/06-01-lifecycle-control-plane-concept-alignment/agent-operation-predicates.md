# Agent 运转谓词体系草案

## Purpose

定义一套用于描述 Agent 在 Lifecycle 中如何运转的谓词。本文先讨论语义，不绑定数据库 schema。

核心目标：避免继续用 `Session`、当前 `ActivityAttemptState`、`CapabilityState` 各自隐式表达 Agent 状态，让 Agent 的身份、执行位置、能力、上下文、等待与状态变化来源都能被清晰陈述。

## Entities

| Entity | 语义 |
| --- | --- |
| `LifecycleRun` | 一次执行生命过程的追踪记录 |
| `LifecycleActivity` | Lifecycle 下 Workflow graph 中的执行节点 |
| `ActivityAttemptState` | Activity 的一次 executor execution record |
| `LifecycleActor` / `AgentStateAnchor` | `RuntimeSession` 之上的高层封装，LifecycleRun 内的 Agent 状态与 runtime surface 管理对象 |
| `RuntimeSession` | runtime event log / turn / resume substrate |
| `ActivityProcedure` / `ActorProcedure` | 单个 Agent Activity 的行为/能力/上下文契约 |
| `CapabilityProjection` | actor 当前有效工具面 |
| `ContextProjection` | actor 当前可见上下文 |
| `InteractionGate` | human/platform/companion 等等待点 |
| `Artifact` | lifecycle-level 信息交换产物 |
| `SubjectRef` | Story / Task / Project / External 等业务对象引用；Task 本体只是数据/视图对象 |

## Predicate Families

### Identity

```text
actor_in_run(actor, run)
actor_role(actor, role)
actor_uses_agent(actor, project_agent_id)
actor_uses_executor(actor, executor_config_ref)
actor_wraps_session(actor, runtime_session)
```

用于回答：这个 Agent/actor 是谁，属于哪个 LifecycleRun，以什么角色运行，并封装哪个 runtime session。

### Execution Anchor

```text
activity_in_run(activity, run)
attempt_of(attempt_state, activity)
actor_assigned_to(actor, activity)
actor_running_attempt(actor, attempt_state)
attempt_executed_by(attempt_state, actor)
```

用于回答：这个 actor 当前被哪个 Activity / ActivityAttemptState 驱动。

### Runtime Backing

```text
actor_backed_by_session(actor, runtime_session)
actor_runtime_ref(actor, runtime_session|turn|connector_resume_ref)
attempt_started_by_actor(attempt_state, actor)
turn_belongs_to_actor(turn_id, actor)
```

用于回答：runtime session 和 connector turn 只是哪个 actor 的承载；ActivityAttemptState 通过 Actor assignment 追溯到 runtime。

### State

```text
actor_status(actor, ready|running|waiting|blocked|suspended|completed|failed)
attempt_status(attempt_state, ready|claiming|running|completed|failed|cancelled)
run_status(run, ready|running|blocked|completed|failed|cancelled)
```

用于避免把 run 状态、attempt 状态、actor 状态混成一个字段。

### Capability

```text
actor_has_capability(actor, capability_key, source, scope)
actor_can_call_tool(actor, tool_path)
capability_granted_by(capability_key, grant_id)
capability_constrained_by(actor, activity_procedure)
actor_frame_has_capability(actor_frame, capability_key)
```

用于回答：Agent 当前为什么能调用某个工具。capability 的自上而下事实源应在 ActorFrame / Actor runtime surface，而不是散落在 SessionMeta 或单个 attempt 上。

### Context

```text
actor_sees_context(actor, context_ref, source)
actor_inherits_context(actor, parent_actor_or_run, policy)
actor_mounts_vfs(actor, mount_ref, capability)
actor_uses_procedure(actor, activity_procedure)
actor_frame_projects_context(actor_frame, context_ref)
```

用于回答：Agent 当前能看到什么，以及这些上下文来自哪里。context、VFS、MCP、session runtime refs 应由 ActorFrame 管理。

### Subject

```text
actor_acts_on(actor, subject_ref)
activity_payload(activity, subject_ref)
run_subject_association(run, subject_ref, role)
actor_subject_association(actor, subject_ref, role)
```

用于回答：Agent / Activity 正在处理哪个业务对象引用。对 Task，只表达 `SubjectRef(kind=Task, id=T)`；Task entity 本身没有 runtime 语义。ActivityAttemptState 不作为 subject association anchor，它只通过 Actor assignment 成为执行证据。

### Exchange

```text
attempt_produces(attempt_state, artifact)
attempt_consumes(attempt_state, artifact)
artifact_binds_to(source_artifact, target_activity_port)
actor_publishes(actor, artifact)
actor_reads(actor, artifact)
```

用于回答：并发 Agent Activities 如何通过 Lifecycle 层交换信息。

### Causality

```text
actor_revision(actor, revision)
state_changed_by(revision, activity_event|runtime_command|permission_grant|interaction_response)
activity_applies_actor_delta(activity, actor, delta)
runtime_transition_applies_to_actor(command, actor)
```

用于回答：Agent 状态为什么改变，谁改变了它，是否可回放。

### Wait / Gate

```text
actor_waits_on(actor, gate)
attempt_blocked_by(attempt_state, gate)
gate_resolved_by(gate, response)
gate_resumes_actor(gate, actor)
```

用于回答：Agent 为什么暂停，谁能恢复它。

## Working Rule

当我们描述一个 Agent 运转状态时，应尽量能还原成如下句式：

```text
Actor A in LifecycleRun R
wraps RuntimeSession RS,
acts on SubjectRef S,
is assigned to Activity X / ActivityAttemptState #n,
uses ActivityProcedure P,
sees Context C,
has Capability K from Source G,
and its current revision was changed by Event E.
```

如果一个状态无法被这套谓词描述，说明它可能还隐式藏在 Session、LifecycleRun、ActivityAttemptState 或 live runtime 中，需要继续拆解。
