# Parent Implementation Plan

## Phase 1: Child Planning

- [x] Create child task: `06-10-subject-context-assignment`.
- [x] Create child task: `06-10-story-task-agent-command-hard-cut`.
- [x] Create child task: `06-10-story-task-agent-frontend-contract-cleanup`.
- [x] Review child PRDs/designs and confirm the ProjectAgent subject/context API decision.
- [ ] Curate child `implement.jsonl` / `check.jsonl`.

## Phase 2: Parallel Execution Order

1. Start `06-10-subject-context-assignment` first or in parallel with backend inventory. It defines the replacement model.
2. Start `06-10-story-task-agent-command-hard-cut` after child 1 has at least a stable interface/design. It can remove dead Task paths while child 1 lands.
3. Start `06-10-story-task-agent-frontend-contract-cleanup` after backend route/DTO shape is clear.

## Phase 3: Parent Integration Review

- [ ] Run residual scans across all children:
  - `compose_story_step`
  - `StoryStepSpec`
  - `StoryStepPhase`
  - `TaskLaunchSource`
  - `TaskLaunchPhase`
  - `task_service_input`
  - `/tasks/{id}/start`
  - `/tasks/{id}/continue`
  - `/stories/{id}/launch`
  - `is_default_for_story`
  - `is_default_for_task`
  - `task_management::start_task`
- [ ] Verify ProjectAgent launch/message still works and can receive subject context when that scope is accepted.
- [ ] Verify ProjectAgent UI does not expose a subject picker in this task.
- [ ] Verify Story quick-create-session remains documented as a future thin facade over ProjectAgent session start, not an implemented Story Agent route.
- [ ] Verify Story/Task pages still show business data and read-only lifecycle projection.
- [ ] Verify specs/docs describe Story/Task as subject/context/projection, not agent owner.

## Phase 4: Parent Validation

- [ ] `python ./.trellis/scripts/task.py validate .trellis/tasks/06-10-story-task-agent-cleanup-review`
- [ ] `pnpm run migration:guard` if migrations are touched.
- [ ] Contracts generation/check command used by this repo.
- [ ] `pnpm run backend:clippy`
- [ ] Frontend lint/typecheck command.
