# Scoped Lifecycle Artifacts Design

## Target Ref

```rust
pub struct ActivityPortArtifactRef {
    pub run_id: Uuid,
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
    pub port_key: String,
}
```

若继续复用 inline lifecycle files，path builder 必须隐藏 raw path：

```text
activity_port_outputs/{graph_instance_id}/{activity_key}/{attempt}/{port_key}
activity_port_inputs/{graph_instance_id}/{activity_key}/{attempt}/{port_key}
```

若新增 first-class repository，表名建议为 `lifecycle_activity_port_artifacts`，唯一键为 `(run_id, graph_instance_id, activity_key, attempt, port_key, direction)`。

## Write Flow

```text
lifecycle VFS artifacts/{port_key}
  -> mount metadata resolves run_id + graph_instance_id
  -> WorkflowGraphInstance.activity_state resolves active activity attempt
  -> validate port_key against declared output ports
  -> write ActivityPortArtifactRef
```

VFS 不得从 run-level output map 反查当前 activity。

## Read Flow

Completion / hook gate：

```text
ActivityAttemptRef
  -> list outputs for same run + graph_instance + activity + attempt
  -> compare against declared required output ports
```

Artifact binding：

```text
upstream ActivityAttemptRef + from_port + alias policy
  -> resolve scoped output
  -> create downstream scoped input
```

Read model：

```text
run_id
  -> list scoped artifacts
  -> group by graph instance / activity / attempt / port
  -> expose ActivityOutputArtifact[]
```

## Affected Areas

- `crates/agentdash-application/src/workflow/lifecycle/journey/mod.rs`
- `crates/agentdash-application/src/workflow/orchestrator.rs`
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs`
- `crates/agentdash-application/src/hooks/*`
- `crates/agentdash-contracts/src/workflow.rs`
- `crates/agentdash-infrastructure/migrations/0001_init.sql`
- `packages/app-web/src/generated/workflow-contracts.ts`

## Validation

- Unit: scoped artifact ref path/repository key roundtrip.
- Integration: two graph instances write same `result` port and complete independently.
- Integration: retry attempt writes a new output without overwriting previous attempt history.
- Hook: `port_output_gate` scopes to current attempt.
- VFS: two graph mounts in one run can read/write same port key independently.
