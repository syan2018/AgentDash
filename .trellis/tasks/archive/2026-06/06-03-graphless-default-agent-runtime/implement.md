# Implement Plan

## Preparation

- Re-read backend workflow/session/cross-layer/frontend type specs before editing.
- Start from the current branch state; preserve unrelated uncommitted frontend / Trellis changes.
- Treat the previous freeform seed commit as stopgap code to remove or supersede.

## Ordered Checklist

1. Domain and schema
   - Add `LifecycleRunTopology(Graphless, WorkflowGraph)` and make `LifecycleRun.root_graph_id` optional.
   - Update constructors/helpers so graphless and graph run creation are explicit.
   - Update `SubjectExecutionDispatchResult`, `InteractionGateOpenedDispatchResult`, task execution result, and routine dispatch refs so assignment is optional.
   - Update curated `0001_init.sql` lifecycle run schema with nullable `root_graph_id`, `topology text NOT NULL`, and a topology/root graph check constraint; update repository row mapping / INSERT / UPDATE / list queries.

2. Dispatch service
   - Allow dispatch intents/plans to omit `workflow_graph_ref`.
   - Implement graphless branch in `LifecycleDispatchService`: run, agent, frame, runtime session, anchor, subject association, no graph instance, no assignment.
   - Keep existing graph branch for explicit lifecycle and `start_lifecycle_run`.
   - Ensure runtime anchor writes `None` for graph/activity/assignment in graphless branch.

3. Default entrypoints
   - ProjectAgent launch uses graphless when `default_lifecycle_key` is empty.
   - Story / Task / Routine / Companion default builders use graphless unless an explicit lifecycle is configured.
   - Remove references to `FREEFORM_LIFECYCLE_KEY` from production dispatch paths.
   - Remove Project creation / boot reconcile freeform seeding.

4. ProjectAgent API cleanup
   - Remove `default_procedure_key` from contracts, API request handling, frontend service/store payloads, and ProjectAgent editor UI.
   - Delete `auto:{procedure}` lifecycle wrapper creation.
   - Keep lifecycle key validation for explicit lifecycle only.

5. Projection and cancel/reuse behavior
   - Update LifecycleRun view and SubjectExecution view for `topology=graphless`.
   - Update Task start / continue / view / cancel to work from subject association + lifecycle agent + current frame when assignment is absent; cancel graphless runtime via latest frame runtime session.
   - Update Routine execution and reuse resolver to persist and reuse run / agent / frame refs with optional assignment.
   - Keep Activity-specific projection and cancellation paths for explicit workflow runs.

6. Contracts and frontend
   - Regenerate Rust-to-TS contracts.
   - Update frontend generated consumers for optional `root_graph_id` and optional assignment refs.
   - Ensure stores/components do not assume graph instances exist for every active run.

7. Cleanup
   - Remove unused freeform constants/services/tests or move builders to test fixtures only.
   - Search for `builtin.freeform_session`, `FREEFORM_LIFECYCLE_KEY`, `default_procedure_key`, and `auto:` to confirm no production default path remains.

## Validation Commands

Run focused checks first:

```powershell
cargo test -p agentdash-domain workflow
cargo test -p agentdash-application dispatch_service
cargo test -p agentdash-application task
cargo test -p agentdash-application routine
pnpm run contracts:check
pnpm run frontend:check
```

Then run broader checks if focused changes pass:

```powershell
cargo check
```

## Required Test Scenarios

- ProjectAgent graphless launch succeeds without `WorkflowGraph` lookup.
- ProjectAgent explicit lifecycle still creates graph instance and assignment.
- Task start / continue / cancel default graphless path returns no assignment and can reuse existing agent/frame.
- Routine Fresh / Reuse / PerEntity default graphless paths persist optional assignment refs correctly.
- Companion subagent default path creates graphless child agent and gate when requested.
- LifecycleRunView serializes graphless run with `topology=graphless`, `root_graph_id=null`, and empty graph instances.
- ProjectAgent create/update no longer accepts or emits `default_procedure_key`; no `auto:*` workflow graph is created.

## Rollback Points

- If graphless dispatch destabilizes explicit workflow tests, isolate the branch at dispatch plan construction and keep graph branch untouched.
- If task/routine assignment optionality becomes too broad, first make API/domain fields optional while leaving explicit Activity paths unchanged, then update graphless paths.
- If frontend contract drift is large, prioritize generated DTO consumers and leave visual polish for a follow-up.

## Done Criteria

- No production search hits for default freeform dispatch.
- No production creation of `auto:{procedure}` graph.
- Graphless default Agent sessions work end-to-end from API to frontend projection.
- Explicit WorkflowGraph Activity flows still pass existing tests.
