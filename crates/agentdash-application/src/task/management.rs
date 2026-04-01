use uuid::Uuid;

use agentdash_domain::task::{Task, TaskAggregateCommandRepository};

pub async fn create_task_aggregate(
    task_command_repo: &dyn TaskAggregateCommandRepository,
    task: &Task,
) -> Result<(), agentdash_domain::DomainError> {
    task_command_repo.create_for_story(task).await
}

pub async fn delete_task_aggregate(
    task_command_repo: &dyn TaskAggregateCommandRepository,
    task_id: Uuid,
) -> Result<Task, agentdash_domain::DomainError> {
    task_command_repo.delete_for_story(task_id).await
}
