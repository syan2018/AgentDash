# Implementation Plan

## Steps

- [x] Remove Task command API routes and router registration.
- [x] Remove Task command DTOs and application command types.
- [x] Remove Task command methods from `StoryActivityActivationService`; keep/rename read-only helpers if needed.
- [x] Remove task launch hints from `LaunchCommand`.
- [x] Remove `StoryStepSpec` family and task composer.
- [x] Remove Story launch route/service and Story composer.
- [x] Adjust frame construction classifier fallback order.
- [x] Update tests away from Story/Task command assumptions.

## Validation

- [x] Residual `rg` scans from parent task.
- [x] Backend compile/clippy.
- [x] ProjectAgent launch/message and lifecycle node paths compile under `cargo check` / clippy after old composer deletion.
