# Implementation Plan

## Steps

- [ ] Audit existing contributors: `contribute_story_context`, `contribute_task_binding`, `contribute_binding_initial_context`, ProjectAgent context construction.
- [ ] Add `SubjectContextAssignmentResolver` in application layer.
- [ ] Support `SubjectRef(kind=project|story|task)` with repository-backed resolution.
- [ ] Thread optional `subject_ref` through ProjectAgent session start DTO/service.
- [ ] Keep ProjectAgent UI unchanged; do not add a ProjectAgent subject picker in this task.
- [ ] Attach resolved contributions into ProjectAgent frame construction.
- [ ] Update specs for SubjectContext assignment.
- [ ] Add tests for Story and Task assignment output.

## Validation

- [ ] Unit tests for resolver.
- [ ] ProjectAgent session start test with `subject_ref=story`.
- [ ] ProjectAgent session start test with `subject_ref=task`.
- [ ] Contract/API test confirms omitted `subject_ref` remains project-scoped.
- [ ] `pnpm run backend:clippy`
