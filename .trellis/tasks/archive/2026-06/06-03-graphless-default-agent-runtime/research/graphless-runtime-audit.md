# Graphless Runtime Audit

## Confirmed Code Facts

- `LifecycleRun.root_graph_id` is currently a required `Uuid` and is serialized to `LifecycleRunView.root_graph_id`.
- `WorkflowGraphInstance.graph_id` is currently required and graph instances are loaded by several Activity projection paths; graphless should avoid creating graph instances rather than making graph instance graph IDs nullable.
- `AgentFrame.graph_instance_id` and `AgentFrame.activity_key` are already optional.
- `RuntimeSessionExecutionAnchor.assignment_id`, `graph_instance_id`, `activity_key`, and `attempt` are already optional, so graphless anchors can be represented without schema expansion there.
- `LifecycleDispatchService::dispatch_common` currently always resolves a `WorkflowGraphRef`, creates or reuses a `WorkflowGraphInstance`, initializes Activity state, and optionally creates entry assignment.
- `SubjectExecutionDispatchResult` and task/routine result types currently assume assignment exists; these need optional refs for graphless.
- `ProjectAgentLaunchResult.assignment_ref` is already optional in contracts and currently returned as `None`.
- `TaskExecutionResult`, API task responses, and `RoutineDispatchRefs` currently require assignment.

## Default Freeform Entry Points

- ProjectAgent launch uses `default_lifecycle_key.unwrap_or(FREEFORM_LIFECYCLE_KEY)`.
- Story root launch uses `project_agent.default_lifecycle_key.unwrap_or(FREEFORM_LIFECYCLE_KEY)`.
- Task start / continue always build `WorkflowGraphRef::ByKey(FREEFORM_LIFECYCLE_KEY)`.
- Routine dispatch always builds `WorkflowGraphRef::ByKey(FREEFORM_LIFECYCLE_KEY)`.
- Companion subagent dispatch uses `WorkflowGraphRef::ByKey(FREEFORM_LIFECYCLE_KEY)`.
- Project creation and boot reconcile were recently patched to seed `builtin.freeform_session`; this is stopgap code and should be removed by this task.

## Related Cleanup

- ProjectAgent create/update request accepts `default_procedure_key`.
- API logic turns `default_procedure_key` into an auto-generated `auto:{procedure}` WorkflowGraph.
- User decision for this task: delete `default_procedure_key` instead of reinterpreting it as a graphless Agent runtime contract.

## Spec Notes

- Database guideline says current pre-release schema source is the curated `0001_init.sql` baseline.
- Cross-layer contract guideline requires Rust contract DTO changes to regenerate frontend generated TypeScript and avoid frontend-side alias compatibility.
