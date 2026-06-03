# Lifecycle 控制面概念一致性 Final Review

## Verdict

No blocking findings. The current branch is consistent with the target model for this PR:

```text
LifecycleRun -> LifecycleAgent -> AgentFrame -> AgentAssignment -> RuntimeSession trace
```

`RuntimeSession` is no longer the business runtime ownership source. It now acts as runtime trace / turn supervision / transport delivery substrate, with business runtime facts anchored through lifecycle run, agent, frame, assignment and execution anchors.

## Evidence

- `sessions` baseline no longer carries `project_id`, `executor_config_json` or `tab_layout_json`.
- `SessionShellDto` only exposes session shell / delivery trace fields.
- `RuntimeSessionExecutionAnchor` is the session-to-run/agent/frame/assignment bridge.
- `LifecycleRunView` exposes structured `active_activity_refs` derived from graph instance activity state.
- lifecycle artifact output APIs no longer use run-level port maps as runtime fact source.
- generated contracts are up to date and frontend runtime views consume Agent / Lifecycle anchored read models.

## Residual Scans

Passed with no blocker:

```bash
rg "list_by_session|SessionBinding|lifecycle_step_key" crates packages
rg "HookSessionRuntime|SessionHookSnapshot|companion_context|CompanionWaitRegistry" crates .trellis/spec
rg "active_node_keys|current_activity_key" crates packages .trellis/spec
rg "list_port_outputs|write_port_output|load_port_output_map|activity_outputs_from_port_map" crates
rg "WorkflowContract|step_key" crates packages .trellis/spec
rg "executor_config_json|tab_layout_json" crates packages .trellis/spec
```

The remaining `entry_step_key` hit is a legacy-payload rejection fixture, not a current contract.

## Non-Blocking Follow-Ups

- `lifecycle_runs.execution_log` remains as audit-owner cleanup under `06-03-database-business-semantic-convergence`.
- `stories.task_count` and `project_agents.is_default_for_task` remain business semantics cleanup items under the same task.
- Broader companion persistence and lifecycle branching remain outside this PR scope.

## Validation

- `cargo fmt`
- `git diff --check`
- `cargo check --workspace`
- `cargo test -p agentdash-infrastructure`
- `cargo test -p agentdash-application workflow`
- `cargo test -p agentdash-application hooks`
- `cargo test -p agentdash-application vfs::provider_lifecycle`
- `pnpm run contracts:check`
- `pnpm --filter app-web run typecheck`
- `pnpm --filter app-web test`
