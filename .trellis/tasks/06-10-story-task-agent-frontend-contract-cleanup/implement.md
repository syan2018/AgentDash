# Implementation Plan

## Steps

- [ ] Remove `startTaskExecution`, `continueTaskExecution`, `cancelTaskExecution` from story service/store.
- [ ] Remove Task action controls from `TaskSubjectExecutionPanel`; keep projection summary.
- [ ] Remove generated type usage for removed Story/Task command DTOs.
- [ ] Remove ProjectAgent default Story/Task toggles and payload fields after backend fields are gone.
- [ ] Do not add ProjectAgent subject picker even if generated contracts expose optional `subject_ref`.
- [ ] Record Story quick-create-session as future thin facade direction if user-facing copy/spec is touched.
- [ ] Update permission/capability UI labels/docs.

## Validation

- [ ] Frontend lint/typecheck.
- [ ] Contracts generation/check.
- [ ] UI tests for Task projection panel and ProjectAgent route if present.
