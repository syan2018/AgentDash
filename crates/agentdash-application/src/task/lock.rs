use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

/// Per-Task 异步操作锁
///
/// 确保同一个 Task 的生命周期操作（start / continue / cancel）串行执行，
/// 防止并发请求导致状态竞争。不同 Task 之间仍可并行操作。
///
/// 设计参考：Actant `withAgentLock` 模式 —— 所有对同一 Agent 的操作
/// 通过 Promise 链串行化；本实现用 `tokio::sync::Mutex` 达到等价效果。
pub struct TaskLockMap {
    locks: Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
}

impl TaskLockMap {
    pub fn new() -> Self {
        Self {
            locks: Mutex::new(HashMap::new()),
        }
    }

    /// 获取指定 Task 的锁并执行异步操作。
    ///
    /// 如果该 Task 已有操作正在执行，当前调用会等待前一个操作完成后再开始。
    /// 闭包 `f` 在锁获取后才被调用，确保 Future 的创建和执行都在临界区内。
    pub async fn with_lock<F, Fut, R>(&self, task_id: Uuid, f: F) -> R
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = R>,
    {
        let lock = self.get_or_create(task_id);
        let _guard = lock.lock().await;
        f().await
    }

    /// 尝试立即获取锁并执行；如果锁已被占用则返回 `Err(TaskLockBusy)`。
    ///
    /// 适用于不希望排队等待的场景（如诊断接口、幂等性快速拒绝）。
    pub async fn try_with_lock<F, Fut, R>(&self, task_id: Uuid, f: F) -> Result<R, TaskLockBusy>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = R>,
    {
        let lock = self.get_or_create(task_id);
        let _guard = lock.try_lock().map_err(|_| TaskLockBusy { task_id })?;
        Ok(f().await)
    }

    /// 当前追踪的锁条目数量（用于诊断/metrics）
    pub fn tracked_count(&self) -> usize {
        self.locks
            .lock()
            .expect("TaskLockMap 内部 Mutex 中毒")
            .len()
    }

    /// 清理不再被任何操作持有或等待的锁条目，释放内存。
    ///
    /// 当 `Arc::strong_count == 1` 时，说明只有 HashMap 自身持有引用，
    /// 没有任何 `with_lock` 调用正在等待或执行，可以安全移除。
    pub fn shrink(&self) {
        let mut locks = self.locks.lock().expect("TaskLockMap 内部 Mutex 中毒");
        locks.retain(|_, lock| Arc::strong_count(lock) > 1);
    }

    fn get_or_create(&self, task_id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().expect("TaskLockMap 内部 Mutex 中毒");
        locks
            .entry(task_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }
}

impl Default for TaskLockMap {
    fn default() -> Self {
        Self::new()
    }
}

/// `try_with_lock` 返回的错误 —— 指定 Task 当前有其他操作正在执行
#[derive(Debug)]
pub struct TaskLockBusy {
    pub task_id: Uuid,
}

impl std::fmt::Display for TaskLockBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Task {} 当前有其他操作正在执行，请稍后重试",
            self.task_id
        )
    }
}

impl std::error::Error for TaskLockBusy {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn same_task_operations_serialize() {
        let lock_map = TaskLockMap::new();
        let task_id = Uuid::new_v4();
        let counter = Arc::new(AtomicU32::new(0));

        let counter_a = counter.clone();
        let lock_map_ref = &lock_map;

        // 两个操作对同一 task_id 应串行执行
        let handle_a = tokio::spawn({
            let counter = counter_a.clone();
            let lock = lock_map_ref.get_or_create(task_id);
            async move {
                let _guard = lock.lock().await;
                // 模拟耗时操作
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });

        // 稍微延迟以确保 handle_a 先获取锁
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let counter_b = counter.clone();
        let lock_b = lock_map_ref.get_or_create(task_id);
        let handle_b = tokio::spawn(async move {
            let _guard = lock_b.lock().await;
            // 此时 handle_a 应已完成
            let val = counter_b.load(Ordering::SeqCst);
            assert_eq!(val, 1, "第二个操作应在第一个完成后才执行");
            counter_b.fetch_add(1, Ordering::SeqCst);
        });

        handle_a.await.unwrap();
        handle_b.await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn different_tasks_run_in_parallel() {
        let lock_map = TaskLockMap::new();
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();

        let started = Arc::new(AtomicU32::new(0));

        let started_a = started.clone();
        let lock_a = lock_map.get_or_create(task_a);
        let handle_a = tokio::spawn(async move {
            let _guard = lock_a.lock().await;
            started_a.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let started_b = started.clone();
        let lock_b = lock_map.get_or_create(task_b);
        let handle_b = tokio::spawn(async move {
            let _guard = lock_b.lock().await;
            started_b.fetch_add(1, Ordering::SeqCst);
            // 不同 task 应该不需要等待，此时 a 还在 sleep
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        });

        // 等待 b 完成
        handle_b.await.unwrap();
        // 此时 a 和 b 都应该已经启动
        assert_eq!(started.load(Ordering::SeqCst), 2);
        handle_a.await.unwrap();
    }

    #[tokio::test]
    async fn try_with_lock_returns_busy_when_held() {
        let lock_map = Arc::new(TaskLockMap::new());
        let task_id = Uuid::new_v4();

        let lock = lock_map.get_or_create(task_id);
        let _guard = lock.lock().await;

        let result = lock_map.try_with_lock(task_id, || async { 42 }).await;

        assert!(result.is_err());
    }

    #[test]
    fn shrink_removes_idle_entries() {
        let lock_map = TaskLockMap::new();
        let task_id = Uuid::new_v4();

        // 创建一个条目
        let _lock = lock_map.get_or_create(task_id);
        assert_eq!(lock_map.tracked_count(), 1);

        // _lock 被 drop 后，只有 HashMap 持有引用
        drop(_lock);
        lock_map.shrink();
        assert_eq!(lock_map.tracked_count(), 0);
    }
}
