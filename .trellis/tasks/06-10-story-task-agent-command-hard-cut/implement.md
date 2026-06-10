# Implementation Plan

## Steps

- [ ] Remove Task command API routes and router registration.
- [ ] Remove Task command DTOs and application command types.
- [ ] Remove Task command methods from `StoryActivityActivationService`; keep/rename read-only helpers if needed.
- [ ] Remove task launch hints from `LaunchCommand`.
- [ ] Remove `StoryStepSpec` family and task composer.
- [ ] Remove Story launch route/service and Story composer.
- [ ] Adjust frame construction classifier fallback order.
- [ ] Update tests away from Story/Task command assumptions.

## Validation

- [ ] Residual `rg` scans from parent task.
- [ ] Backend compile/clippy.
- [ ] Targeted tests around ProjectAgent launch/message and lifecycle node launch.
