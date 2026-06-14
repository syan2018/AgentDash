# Backend Hard-cut Design

## Delete

- `crates/agentdash-api/src/routes/task_execution.rs` command routes.
- `crates/agentdash-api/src/dto/task_execution.rs` command DTOs that no longer have routes.
- `StoryActivityActivationService` command methods and related command/result types.
- `TaskLaunchSource`, `TaskLaunchPhase`, `LaunchCommand::task_service_input`.
- `StoryStepSpec`, `StoryStepPhase`, `compose_story_step`, `compose_story_step_to_frame`.
- `workflow/frame_construction/composer_task.rs`.
- Story launch route/service. Future Story quick-create-session must be a new thin facade over ProjectAgent session start, not the old Story Agent route.
- `workflow/frame_construction/composer_story.rs` after SubjectContext assignment replaces needed Story context injection.

## Preserve

- `SubjectExecutionView` and lifecycle views.
- `LifecycleSubjectAssociation` and `SubjectRef`.
- ProjectAgent launch/message routes.
- Lifecycle node and companion frame construction.
- Story/Task business entities and repositories.

## Risk

The current classifier may use story/task association branches to recover launch surface for sessions created through old routes. After deleting those routes, this should no longer be needed; ProjectAgent and lifecycle node paths must remain explicitly covered by tests.
