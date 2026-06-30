# Workspace Module Agent Surface 深模块评估 - Implement

## Phase 0 - Evaluation Only

- [ ] Map current responsibilities in `tools.rs` into read surface, operation execution, Canvas host, RuntimeGateway/channel, presentation notification.
- [ ] Preserve the five existing Agent-facing tool names as thin adapters over `WorkspaceModuleAgentSurface::resolve/execute`.
- [ ] Decide whether to migrate full surface in one implementation slice; if not, define the short-lived split and removal point.
- [ ] Produce an ownership deletion map: old helper/tool responsibility -> new deep module / thin adapter / deleted.
- [ ] Identify tests that can move from AgentTool JSON to typed surface.

## Phase 1 - First Slice Candidate

- [ ] Add `WorkspaceModuleAgentSurface` module.
- [ ] Move visible descriptor resolution and operation catalog into facade.
- [ ] Rewire `workspace_module_list` and `workspace_module_describe` to `resolve(context)` while keeping them thin.
- [ ] Add typed tests for surface resolution.
- [ ] Keep existing AgentTool tests until typed tests cover equivalent behavior.

## Phase 2 - Side Effect Surface Candidate

- [ ] Move `workspace_module_invoke` execution path behind typed command.
- [ ] Move `workspace_module_present` path behind typed command/outcome.
- [ ] Normalize Canvas host and Extension channel outcomes.
- [ ] Thin AgentTool tests to schema/result projection only.
- [ ] Delete or demote old scattered ownership; do not leave old helper paths as parallel business rule owners.

## Validation

- `cargo test -p agentdash-workspace-module workspace_module`
- If RuntimeGateway behavior changes: targeted tests in `agentdash-application-runtime-gateway`

## Stop Conditions

- If implementation pressure pushes toward compatibility branches or duplicate old/new tool surfaces, stop and return to design review.
- If Canvas/Extension channel outcome cannot be typed without broad protocol changes, keep it as a second-stage design question.
- If old tool/helper code still owns visibility, operation readiness or presentation rules after the supposed migration, the task is not complete.
