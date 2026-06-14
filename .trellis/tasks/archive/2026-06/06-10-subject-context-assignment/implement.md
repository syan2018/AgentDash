# Implementation Plan

## Steps

- [x] Audit existing contributors: `contribute_story_context`, `contribute_task_binding`, `contribute_binding_initial_context`, ProjectAgent context construction.
- [x] Add `SubjectContextAssignmentResolver` in application layer.
- [x] Support `SubjectRef(kind=project|story|task)` with repository-backed resolution.
- [x] Thread optional `subject_ref` through ProjectAgent session start DTO/service.
- [x] Keep ProjectAgent UI unchanged; do not add a ProjectAgent subject picker in this task.
- [x] Attach resolved contributions into ProjectAgent frame construction.
- [x] Update specs for SubjectContext assignment.
- [ ] Add tests for Story and Task assignment output. Current `RepositorySet` is a whole-application port bundle; dedicated resolver unit tests should follow a small subject-context repository port split.

## Validation

- [ ] Unit tests for resolver.
- [ ] ProjectAgent session start test with `subject_ref=story`.
- [ ] ProjectAgent session start test with `subject_ref=task`.
- [x] Contract/API check confirms generated request shape and omitted `subject_ref` remains optional.
- [x] `pnpm run backend:clippy`
