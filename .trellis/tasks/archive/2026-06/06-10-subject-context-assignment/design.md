# SubjectContext Assignment Design

## Proposed Contract

```rust
pub struct SubjectContextAssignmentRequest {
    pub project_id: Uuid,
    pub subject_ref: SubjectRef,
    pub workspace_policy: SubjectWorkspacePolicy,
}

pub struct SubjectContextAssignment {
    pub subject_ref: SubjectRef,
    pub workspace: Option<Workspace>,
    pub contributions: Vec<Contribution>,
    pub capability_scope: CapabilityScopeCtx,
}
```

The resolver lives in application, depends on repositories, and emits context contributions only. It does not launch agents.

## Data Flow

```text
ProjectAgentSessionStartCommand { subject_ref? }
  -> LifecycleDispatchService launch_agent(subject_ref = project or requested subject)
  -> FrameConstructionService / ProjectAgent composer
  -> SubjectContextAssignmentResolver
  -> build_session_context_bundle
  -> AgentFrameBuilder.with_surface_input
  -> FrameLaunchEnvelope
```

## Subject Rules

- `project`: use existing ProjectAgent context and project workspace defaults.
- `story`: add Story core, Project core, story workspace/default project workspace, story declared sources.
- `task`: add Task binding, parent Story context, effective task workspace, task dispatch declared sources, task-specific instruction fragments only if they remain as context, not command semantics.

## Non-goals

- Do not recreate `/tasks/{id}/start`.
- Do not create Story Agent or Task Agent defaults.
- Do not make `SubjectExecutionView` a command input; it remains read-only projection.

## Product Decision

`ProjectAgentSessionStartCommand` accepts optional `subject_ref`; if omitted, it behaves as project-scoped launch. This keeps current ProjectAgent flow intact and gives future Story quick-create-session a place to request Story context without introducing Story Agent.

Current ProjectAgent UI must not add a subject selector. A future Story quick-create-session entry may call this API with `subject_ref=story`, but it must remain a thin facade over ProjectAgent session start.
