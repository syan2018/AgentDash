# Frontend and Contract Cleanup Design

## UI Model

Story and Task screens are work-item surfaces. They show business metadata, context declarations, and read-only lifecycle projections. They do not own agent launch controls.

ProjectAgent screens own ordinary agent launch and should not add a subject selector in this cleanup. Subject context remains a backend/service capability.

Future Story quick-create-session may live on the Story surface. It should choose a ProjectAgent and call ProjectAgent session start with `subject_ref=story`, then navigate to the created session. That future entry is a thin facade over ProjectAgent + SubjectContext assignment, not a Story Agent launch path. Task gets no equivalent quick-create entry in this task.

## Contract Cleanup

Remove command DTOs and generated types tied to:

- Task start / continue / cancel responses.
- Story launch response if the route is removed.
- Optional ProjectAgent `subject_ref` request field may be generated, but current ProjectAgent UI should not expose it.
- ProjectAgent default Story/Task flags if backend removes columns.

## Permission Copy

Keep `story_management` as a data-management capability. Remove `task_management::start_task` because launching a Task Agent is not a product model.
