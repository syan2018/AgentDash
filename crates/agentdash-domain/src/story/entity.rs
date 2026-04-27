use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{StoryContext, StoryPriority, StoryStatus, StoryType};
use crate::task::Task;

/// Story — 用户价值单元
///
/// 从用户角度描述需求的工作单元，维护完整上下文，编排 Task 执行。
/// 归属于 Project。backend_id 已移除，通过 default_workspace_id → Workspace.backend_id 获取。
///
/// **Aggregate root**：Story 持有 `Vec<Task>` 作为聚合内 child entity。
/// 见 `.trellis/spec/backend/story-task-runtime.md` §2.1 / §2.4 / §5。
/// 任何 task mutation 必须通过 `add_task` / `update_task` / `remove_task` 等 aggregate 方法完成；
/// 禁止从外部直接操作 `Story.tasks` 字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Story {
    pub id: Uuid,
    /// 所属项目
    pub project_id: Uuid,
    /// Story 级默认 Workspace（用于 Task 执行 backend 解析继承链）
    pub default_workspace_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub status: StoryStatus,
    pub priority: StoryPriority,
    pub story_type: StoryType,
    pub tags: Vec<String>,
    /// Story 聚合信息：当前关联 Task 数量（UI 冗余字段，由 aggregate 方法自动维护）
    pub task_count: u32,
    /// 结构化设计上下文
    pub context: StoryContext,
    /// Story aggregate 内的 Task 集合（child entity）
    ///
    /// 物理持久化：`stories.tasks JSONB` 列。无独立 `tasks` 表写入；
    /// 任何 mutation 走 `StoryRepository::update(&Story)` 整体写回。
    #[serde(default)]
    pub tasks: Vec<Task>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Story {
    pub fn new(project_id: Uuid, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            default_workspace_id: None,
            title,
            description,
            status: StoryStatus::Created,
            priority: StoryPriority::default(),
            story_type: StoryType::default(),
            tags: vec![],
            task_count: 0,
            context: StoryContext::default(),
            tasks: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    /// 添加一个 Task 到聚合中。
    ///
    /// 自动维护 `task_count` 与 `updated_at`。
    pub fn add_task(&mut self, task: Task) {
        self.tasks.push(task);
        self.refresh_task_count();
        self.updated_at = Utc::now();
    }

    /// 通过闭包就地修改聚合内的某个 Task。
    ///
    /// 若 task 不存在返回 `false`；存在则执行 mutator 并刷新 `updated_at`。
    pub fn update_task<F>(&mut self, task_id: Uuid, mutator: F) -> bool
    where
        F: FnOnce(&mut Task),
    {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            mutator(task);
            self.updated_at = Utc::now();
            true
        } else {
            false
        }
    }

    /// 从聚合中移除指定 Task；返回被移除的实体。
    ///
    /// 自动维护 `task_count` 与 `updated_at`。
    pub fn remove_task(&mut self, task_id: Uuid) -> Option<Task> {
        let pos = self.tasks.iter().position(|t| t.id == task_id)?;
        let removed = self.tasks.remove(pos);
        self.refresh_task_count();
        self.updated_at = Utc::now();
        Some(removed)
    }

    /// 查找 task（只读引用）。
    pub fn find_task(&self, task_id: Uuid) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == task_id)
    }

    /// 查找 task（可变引用）。
    ///
    /// 注意：调用方手动通过 `&mut Task` 修改字段时，**不会** 自动刷新 `updated_at` /
    /// `task_count`。优先使用 `update_task` 闭包形式以保证不变量。
    pub fn find_task_mut(&mut self, task_id: Uuid) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == task_id)
    }

    /// 用 `tasks.len()` 校准 `task_count` 冗余字段。
    fn refresh_task_count(&mut self) {
        self.task_count = self.tasks.len() as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_task(story_id: Uuid, project_id: Uuid, title: &str) -> Task {
        Task::new(project_id, story_id, title.to_string(), String::new())
    }

    #[test]
    fn new_story_has_no_tasks() {
        let project_id = Uuid::new_v4();
        let story = Story::new(project_id, "S".into(), "".into());
        assert_eq!(story.task_count, 0);
        assert!(story.tasks.is_empty());
    }

    #[test]
    fn add_task_updates_count_and_timestamp() {
        let project_id = Uuid::new_v4();
        let mut story = Story::new(project_id, "S".into(), "".into());
        let original_updated_at = story.updated_at;
        // 等待至少 1ms 让 updated_at 能前进（chrono 精度足够）
        std::thread::sleep(std::time::Duration::from_millis(2));

        let task = sample_task(story.id, project_id, "T1");
        let task_id = task.id;
        story.add_task(task);

        assert_eq!(story.tasks.len(), 1);
        assert_eq!(story.task_count, 1);
        assert!(story.updated_at >= original_updated_at);
        assert_eq!(story.find_task(task_id).map(|t| t.title.as_str()), Some("T1"));
    }

    #[test]
    fn update_task_mutates_in_place() {
        let project_id = Uuid::new_v4();
        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = sample_task(story.id, project_id, "old");
        let task_id = task.id;
        story.add_task(task);

        let ok = story.update_task(task_id, |t| {
            t.title = "new".to_string();
        });
        assert!(ok);
        assert_eq!(story.find_task(task_id).map(|t| t.title.as_str()), Some("new"));
    }

    #[test]
    fn update_task_returns_false_for_unknown_id() {
        let project_id = Uuid::new_v4();
        let mut story = Story::new(project_id, "S".into(), "".into());
        let ok = story.update_task(Uuid::new_v4(), |_t| {});
        assert!(!ok);
    }

    #[test]
    fn remove_task_drops_entry_and_updates_count() {
        let project_id = Uuid::new_v4();
        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = sample_task(story.id, project_id, "T1");
        let task_id = task.id;
        story.add_task(task);

        let removed = story.remove_task(task_id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, task_id);
        assert_eq!(story.task_count, 0);
        assert!(story.tasks.is_empty());
    }

    #[test]
    fn remove_task_returns_none_for_unknown_id() {
        let project_id = Uuid::new_v4();
        let mut story = Story::new(project_id, "S".into(), "".into());
        let removed = story.remove_task(Uuid::new_v4());
        assert!(removed.is_none());
    }

    #[test]
    fn find_task_mut_does_not_auto_update_count() {
        let project_id = Uuid::new_v4();
        let mut story = Story::new(project_id, "S".into(), "".into());
        let task = sample_task(story.id, project_id, "T1");
        let task_id = task.id;
        story.add_task(task);

        // 手动改字段，不会触发 task_count 重算
        if let Some(task) = story.find_task_mut(task_id) {
            task.title = "changed".to_string();
        }
        assert_eq!(story.task_count, 1);
        assert_eq!(
            story.find_task(task_id).map(|t| t.title.as_str()),
            Some("changed")
        );
    }
}
