# Scoped Lifecycle Artifacts Design

## Artifact Ref

目标 ref：

```rust
pub struct ActivityPortArtifactRef {
    pub run_id: Uuid,
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
    pub port_key: String,
}
```

Inline file path 可采用：

```text
activity_port_outputs/{graph_instance_id}/{activity_key}/{attempt}/{port_key}
```

输入 artifact 可采用：

```text
port_inputs/{graph_instance_id}/{activity_key}/{attempt}/{port_key}
```

## Write Flow

```text
lifecycle_vfs artifacts/{port_key}
  -> load active context by mount metadata graph_instance_id
  -> current running/claiming attempt
  -> validate port_key in active activity output ports
  -> write scoped artifact
```

## Read Flow

Completion / hook gate：

```text
ActivityAttemptRef
  -> list scoped outputs for this attempt
  -> validate declared ports
```

Artifact binding：

```text
upstream ActivityAttemptRef + from_port
  -> resolve output by alias policy
  -> create downstream input artifact
```

Read model：

```text
run_id
  -> list scoped artifacts
  -> group by graph_instance/activity/attempt/port
```

物理存储可以新增 first-class `lifecycle_activity_artifacts` 表；若继续使用 inline files，则必须通过 typed helper 暴露 `ActivityPortArtifactRef`，调用方不能直接拼 raw path。

## Affected Areas

- `workflow/execution_log.rs`
- `workflow/lifecycle/journey/mod.rs`
- `workflow/orchestrator.rs`
- `workflow/agent_executor.rs`
- `workflow/activity_activation.rs`
- `session/assembler.rs`
- `hooks/provider.rs`
- `hooks/rules.rs`
- `scripts/hook-presets/port_output_gate.rhai`
- `vfs/provider_lifecycle.rs`
- `vfs/lifecycle_catalog.rs`
- `workflow/lifecycle_run_view_builder.rs`

## Migration

Add migration to reshape existing inline lifecycle files. Since the project is pre-release, runtime code should not keep compatibility branches after migration.

## Validation

- Unit: path builder/parser roundtrip for scoped artifact refs。
- Integration: two graph instances write same `result` port and complete independently。
- Integration: retry attempt writes new output without overwriting previous attempt history。
- Hook test: `port_output_gate` scopes to current attempt。
- VFS test: two graph mounts in the same run can write/read the same port key independently。
