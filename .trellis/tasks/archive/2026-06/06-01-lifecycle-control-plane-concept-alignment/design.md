# Lifecycle 控制面概念一致性 Final Review Design

## Purpose

本任务保留为巨型 Lifecycle 控制面重构 PR 合并前的概念一致性 review 入口。它不再派发大规模实现；实现已拆到后续 6/1-6/3 child / successor tasks。当前 review 的职责是确认代码、contracts、migration、frontend runtime view 和 specs 是否仍符合本任务形成的目标模型。

## Target Model To Verify

```text
SubjectRef / ProjectAgent / Routine / Companion
  -> LifecycleDispatchService
  -> LifecycleRun
  -> LifecycleAgent
  -> AgentFrame
  -> AgentAssignment / ActivityAttemptState
  -> RuntimeSession trace
```

Core invariants:

- Lifecycle / Agent / Frame / Assignment own business runtime facts.
- RuntimeSession owns turn supervision, transport delivery, stream ingestion and trace drill-down.
- Task and Story are subjects or product navigation surfaces, not runtime fact owners.
- WorkflowGraphInstance owns Activity runtime state.
- Artifact/output facts are scoped by graph instance, activity and attempt.
- Public runtime read models are Agent / Lifecycle anchored; session-indexed endpoints are adapters.

## Review Boundaries

Use this task to review:

- Concept drift in code and generated contracts.
- Spec drift against the target invariants.
- Remaining task boundaries and whether they still map to the target model.
- PR merge blockers caused by contradictory facts or misleading public contracts.

Do not use this task to implement:

- Scoped artifact storage.
- Active projection cleanup.
- Database business semantic cleanup.
- New companion persistence or lifecycle branching features.

Those are owned by their dedicated tasks.

## Closeout Output

The final reviewer should write a closeout note in this task directory with:

- target model checklist result
- blocking findings
- non-blocking follow-up tasks
- links to archived successor tasks that fulfilled the original concept goals
- validation commands executed
