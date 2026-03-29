use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

/// 重启策略配置
///
/// 控制 Task 失败后的自动重试行为。参考 Actant `RestartPolicy`，
/// 增加了指数退避和稳定运行后重置计数的机制。
#[derive(Debug, Clone)]
pub struct RestartPolicy {
    /// 最大重试次数（达到后拒绝重试）
    pub max_restarts: u32,
    /// 退避基准延迟
    pub backoff_base: Duration,
    /// 退避上限延迟
    pub backoff_max: Duration,
    /// 稳定运行超过此时长后，重置重试计数
    pub reset_after: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 3,
            backoff_base: Duration::from_secs(2),
            backoff_max: Duration::from_secs(60),
            reset_after: Duration::from_secs(300),
        }
    }
}

/// 重试决策结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartDecision {
    /// 允许重试，需等待指定延迟后执行
    Allowed { attempt: u32, delay: Duration },
    /// 拒绝重试，已达到最大重试次数
    Denied { attempts_exhausted: u32 },
}

impl RestartDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }
}

/// 单个 Task 的重启状态
#[derive(Debug, Clone)]
struct TaskRestartState {
    count: u32,
    last_failure: Instant,
    last_stable_start: Option<Instant>,
}

/// Per-Task 重启追踪器
///
/// 为每个 Task 维护独立的重启计数和退避状态。
/// Turn Monitor 在检测到失败时咨询此追踪器，决定是自动重试还是标记 Failed。
///
/// 线程安全：内部使用 `std::sync::Mutex`（非 tokio），因为临界区极短（纯内存操作）。
pub struct RestartTracker {
    policy: RestartPolicy,
    states: Mutex<HashMap<Uuid, TaskRestartState>>,
}

impl RestartTracker {
    pub fn new(policy: RestartPolicy) -> Self {
        Self {
            policy,
            states: Mutex::new(HashMap::new()),
        }
    }

    /// 记录 Task 开始稳定运行（turn 成功启动时调用）
    ///
    /// 用于计算"稳定运行时长"——如果 Task 稳定运行超过 `reset_after`，
    /// 下次失败时重试计数会被重置。
    pub fn record_stable_start(&self, task_id: Uuid) {
        let mut states = self.states.lock().expect("RestartTracker Mutex 中毒");
        if let Some(state) = states.get_mut(&task_id) {
            state.last_stable_start = Some(Instant::now());
        }
    }

    /// 报告 Task 失败，返回重试决策
    ///
    /// 核心逻辑：
    /// 1. 如果 Task 自上次失败后稳定运行超过 `reset_after`，重置计数
    /// 2. 如果重试次数未超限，返回 `Allowed` + 指数退避延迟
    /// 3. 否则返回 `Denied`
    pub fn report_failure(&self, task_id: Uuid) -> RestartDecision {
        let mut states = self.states.lock().expect("RestartTracker Mutex 中毒");
        let now = Instant::now();

        let state = states.entry(task_id).or_insert_with(|| TaskRestartState {
            count: 0,
            last_failure: now,
            last_stable_start: None,
        });

        // 如果自上次失败后稳定运行超过阈值，重置计数
        if let Some(stable_start) = state.last_stable_start
            && stable_start > state.last_failure
            && now.duration_since(stable_start) >= self.policy.reset_after
        {
            state.count = 0;
        }

        if state.count >= self.policy.max_restarts {
            return RestartDecision::Denied {
                attempts_exhausted: state.count,
            };
        }

        let delay = std::cmp::min(
            self.policy
                .backoff_base
                .saturating_mul(2u32.saturating_pow(state.count)),
            self.policy.backoff_max,
        );

        state.count += 1;
        state.last_failure = now;
        state.last_stable_start = None;

        RestartDecision::Allowed {
            attempt: state.count,
            delay,
        }
    }

    /// 清除指定 Task 的重启状态（Task 手动重置或删除时调用）
    pub fn clear(&self, task_id: Uuid) {
        let mut states = self.states.lock().expect("RestartTracker Mutex 中毒");
        states.remove(&task_id);
    }

    /// 当前追踪的 Task 数量（诊断用）
    pub fn tracked_count(&self) -> usize {
        self.states.lock().expect("RestartTracker Mutex 中毒").len()
    }

    /// 查询指定 Task 的当前重试次数（诊断/日志用）
    pub fn current_attempt_count(&self, task_id: Uuid) -> u32 {
        self.states
            .lock()
            .expect("RestartTracker Mutex 中毒")
            .get(&task_id)
            .map(|s| s.count)
            .unwrap_or(0)
    }
}

impl Default for RestartTracker {
    fn default() -> Self {
        Self::new(RestartPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_policy() -> RestartPolicy {
        RestartPolicy {
            max_restarts: 3,
            backoff_base: Duration::from_millis(100),
            backoff_max: Duration::from_secs(5),
            reset_after: Duration::from_millis(200),
        }
    }

    #[test]
    fn first_failure_is_allowed() {
        let tracker = RestartTracker::new(test_policy());
        let task_id = Uuid::new_v4();

        let decision = tracker.report_failure(task_id);
        assert_eq!(
            decision,
            RestartDecision::Allowed {
                attempt: 1,
                delay: Duration::from_millis(100), // base * 2^0
            }
        );
    }

    #[test]
    fn exponential_backoff_increases() {
        let tracker = RestartTracker::new(test_policy());
        let task_id = Uuid::new_v4();

        let d1 = tracker.report_failure(task_id);
        let d2 = tracker.report_failure(task_id);
        let d3 = tracker.report_failure(task_id);

        assert_eq!(
            d1,
            RestartDecision::Allowed {
                attempt: 1,
                delay: Duration::from_millis(100),
            }
        );
        assert_eq!(
            d2,
            RestartDecision::Allowed {
                attempt: 2,
                delay: Duration::from_millis(200),
            }
        );
        assert_eq!(
            d3,
            RestartDecision::Allowed {
                attempt: 3,
                delay: Duration::from_millis(400),
            }
        );
    }

    #[test]
    fn denied_after_max_restarts() {
        let tracker = RestartTracker::new(test_policy());
        let task_id = Uuid::new_v4();

        for _ in 0..3 {
            assert!(tracker.report_failure(task_id).is_allowed());
        }

        let decision = tracker.report_failure(task_id);
        assert_eq!(
            decision,
            RestartDecision::Denied {
                attempts_exhausted: 3,
            }
        );
    }

    #[test]
    fn stable_run_resets_count() {
        let policy = RestartPolicy {
            max_restarts: 2,
            backoff_base: Duration::from_millis(10),
            backoff_max: Duration::from_secs(1),
            reset_after: Duration::from_millis(50),
        };
        let tracker = RestartTracker::new(policy);
        let task_id = Uuid::new_v4();

        // 用掉 2 次重试额度
        assert!(tracker.report_failure(task_id).is_allowed());
        assert!(tracker.report_failure(task_id).is_allowed());
        assert!(!tracker.report_failure(task_id).is_allowed()); // denied

        // 模拟稳定运行：手动设置 last_stable_start 到过去
        {
            let mut states = tracker.states.lock().unwrap();
            let state = states.get_mut(&task_id).unwrap();
            state.last_stable_start = Some(Instant::now() - Duration::from_millis(100));
            // 让 last_failure 也在 stable_start 之前
            state.last_failure = Instant::now() - Duration::from_millis(200);
        }

        // 稳定运行后重试计数应被重置
        let decision = tracker.report_failure(task_id);
        assert!(decision.is_allowed());
        assert_eq!(
            decision,
            RestartDecision::Allowed {
                attempt: 1,
                delay: Duration::from_millis(10),
            }
        );
    }

    #[test]
    fn different_tasks_independent() {
        let tracker = RestartTracker::new(test_policy());
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();

        for _ in 0..3 {
            tracker.report_failure(task_a);
        }
        assert!(!tracker.report_failure(task_a).is_allowed());

        // task_b 不受 task_a 影响
        assert!(tracker.report_failure(task_b).is_allowed());
    }

    #[test]
    fn clear_resets_task_state() {
        let tracker = RestartTracker::new(test_policy());
        let task_id = Uuid::new_v4();

        for _ in 0..3 {
            tracker.report_failure(task_id);
        }
        assert!(!tracker.report_failure(task_id).is_allowed());

        tracker.clear(task_id);
        assert!(tracker.report_failure(task_id).is_allowed());
    }

    #[test]
    fn backoff_capped_at_max() {
        let policy = RestartPolicy {
            max_restarts: 20,
            backoff_base: Duration::from_secs(1),
            backoff_max: Duration::from_secs(10),
            reset_after: Duration::from_secs(300),
        };
        let tracker = RestartTracker::new(policy);
        let task_id = Uuid::new_v4();

        // 2^4 = 16s > max(10s)，应被截断
        for _ in 0..4 {
            tracker.report_failure(task_id);
        }
        let decision = tracker.report_failure(task_id);
        assert_eq!(
            decision,
            RestartDecision::Allowed {
                attempt: 5,
                delay: Duration::from_secs(10), // capped
            }
        );
    }
}
