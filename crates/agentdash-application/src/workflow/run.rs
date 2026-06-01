use agentdash_domain::workflow::{LifecycleRun, LifecycleRunStatus};

pub fn select_active_run(runs: Vec<LifecycleRun>) -> Option<LifecycleRun> {
    runs.into_iter()
        .filter(|run| {
            run.current_activity_key().is_some()
                && matches!(
                    run.status,
                    LifecycleRunStatus::Ready
                        | LifecycleRunStatus::Running
                        | LifecycleRunStatus::Blocked
                )
        })
        .max_by_key(|run| (active_run_status_priority(run.status), run.updated_at))
}

fn active_run_status_priority(status: LifecycleRunStatus) -> i32 {
    match status {
        LifecycleRunStatus::Running => 3,
        LifecycleRunStatus::Ready => 2,
        LifecycleRunStatus::Blocked => 1,
        LifecycleRunStatus::Draft
        | LifecycleRunStatus::Completed
        | LifecycleRunStatus::Failed
        | LifecycleRunStatus::Cancelled => 0,
    }
}
