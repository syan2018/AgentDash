# Implementation Plan

## Steps

- [x] Remove `startTaskExecution`, `continueTaskExecution`, `cancelTaskExecution` from story service/store.
- [x] Remove Task action controls from `TaskSubjectExecutionPanel`; keep projection summary.
- [x] Remove generated type usage for removed Story/Task command DTOs.
- [x] Remove ProjectAgent default Story/Task toggles and payload fields after backend fields are gone.
- [x] Do not add ProjectAgent subject picker even if generated contracts expose optional `subject_ref`.
- [x] Record Story quick-create-session as future thin facade direction if user-facing copy/spec is touched.
- [x] Update permission/capability UI labels/docs.

## Validation

- [x] Frontend lint/typecheck.
- [x] Contracts generation/check.
- [x] No dedicated UI test target was present for this panel; validated with typecheck/lint and residual route/action scans.
