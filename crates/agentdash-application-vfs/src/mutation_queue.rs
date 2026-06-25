use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::{Mutex, OwnedMutexGuard};

/// 按资源 key 串行化 VFS 写操作，避免并行工具调用同时修改同一文件。
#[derive(Clone, Default)]
pub(crate) struct MutationQueue {
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl MutationQueue {
    pub(crate) async fn with_locks<F, T>(&self, keys: Vec<String>, future: F) -> T
    where
        F: Future<Output = T>,
    {
        let keys = normalized_keys(keys);
        if keys.is_empty() {
            return future.await;
        }

        let locks = {
            let mut registry = self.locks.lock().await;
            keys.iter()
                .map(|key| {
                    registry
                        .entry(key.clone())
                        .or_insert_with(|| Arc::new(Mutex::new(())))
                        .clone()
                })
                .collect::<Vec<_>>()
        };

        let mut guards: Vec<OwnedMutexGuard<()>> = Vec::with_capacity(locks.len());
        for lock in locks {
            guards.push(lock.lock_owned().await);
        }

        let result = future.await;
        drop(guards);
        result
    }
}

fn normalized_keys(mut keys: Vec<String>) -> Vec<String> {
    keys.retain(|key| !key.trim().is_empty());
    keys.sort();
    keys.dedup();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Barrier;

    #[tokio::test]
    async fn same_key_runs_serially() {
        let queue = MutationQueue::default();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(3));

        let mut handles = Vec::new();
        for _ in 0..2 {
            let queue = queue.clone();
            let active = active.clone();
            let peak = peak.clone();
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                queue
                    .with_locks(vec!["workspace://a.txt".to_string()], async move {
                        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                    })
                    .await;
            }));
        }

        barrier.wait().await;
        for handle in handles {
            handle.await.expect("task should finish");
        }

        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn different_keys_can_run_concurrently() {
        let queue = MutationQueue::default();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(3));

        let mut handles = Vec::new();
        for key in ["workspace://a.txt", "workspace://b.txt"] {
            let queue = queue.clone();
            let active = active.clone();
            let peak = peak.clone();
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                queue
                    .with_locks(vec![key.to_string()], async move {
                        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                    })
                    .await;
            }));
        }

        barrier.wait().await;
        for handle in handles {
            handle.await.expect("task should finish");
        }

        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }
}
