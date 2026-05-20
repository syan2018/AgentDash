use std::collections::HashMap;

use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository};
use uuid::Uuid;

/// Lifecycle node 子 session 的 binding label 前缀。
pub const LIFECYCLE_NODE_LABEL_PREFIX: &str = "lifecycle_node:";
pub const LIFECYCLE_ACTIVITY_LABEL_PREFIX: &str = "lifecycle_activity:";

/// 子 session 与 lifecycle node 的关联解析结果。
#[derive(Debug, Clone)]
pub struct LifecycleNodeSessionAssociation {
    pub run: LifecycleRun,
    pub node_key: String,
}

/// 子 session 与 lifecycle activity attempt 的关联解析结果。
#[derive(Debug, Clone)]
pub struct LifecycleActivitySessionAssociation {
    pub run: LifecycleRun,
    pub activity_key: String,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleActivityLabelParts {
    pub run_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
}

/// 从 binding label 里解析 node_key。
pub fn lifecycle_node_key_from_label(label: &str) -> Option<&str> {
    let node_key = label.strip_prefix(LIFECYCLE_NODE_LABEL_PREFIX)?.trim();
    if node_key.is_empty() {
        None
    } else {
        Some(node_key)
    }
}

/// 构造 lifecycle node 子 session 的 binding label。
pub fn build_lifecycle_node_label(node_key: &str) -> String {
    format!("{LIFECYCLE_NODE_LABEL_PREFIX}{node_key}")
}

pub fn build_lifecycle_activity_label(run_id: Uuid, activity_key: &str, attempt: u32) -> String {
    format!("{LIFECYCLE_ACTIVITY_LABEL_PREFIX}{run_id}:{activity_key}#{attempt}")
}

pub fn lifecycle_activity_parts_from_label(label: &str) -> Option<LifecycleActivityLabelParts> {
    let payload = label.strip_prefix(LIFECYCLE_ACTIVITY_LABEL_PREFIX)?.trim();
    let (run_id, activity_attempt) = payload.split_once(':')?;
    let (activity_key, attempt) = activity_attempt.rsplit_once('#')?;
    if activity_key.is_empty() {
        return None;
    }
    Some(LifecycleActivityLabelParts {
        run_id: Uuid::parse_str(run_id).ok()?,
        activity_key: activity_key.to_string(),
        attempt: attempt.parse().ok()?,
    })
}

/// 解析 session 是否为某个 lifecycle node 子 session，并返回其 run + node 关联。
///
/// 查找路径：
/// 1. 读取 session bindings，筛选 `label=lifecycle_node:{node_key}`；
/// 2. 基于 binding.project_id 查询该项目所有 runs；
/// 3. 匹配 `step_states.step_key == node_key && step_states.session_id == session_id`。
pub async fn resolve_node_session_association(
    session_id: &str,
    session_binding_repo: &dyn SessionBindingRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<LifecycleNodeSessionAssociation>, String> {
    let bindings = session_binding_repo
        .list_by_session(session_id)
        .await
        .map_err(|e| format!("查询 session bindings 失败: {e}"))?;
    if bindings.is_empty() {
        return Ok(None);
    }

    let mut runs_cache: HashMap<Uuid, Vec<LifecycleRun>> = HashMap::new();
    for binding in bindings {
        let Some(node_key) = lifecycle_node_key_from_label(&binding.label).map(str::to_string)
        else {
            continue;
        };

        let runs = if let Some(cached) = runs_cache.get(&binding.project_id) {
            cached.clone()
        } else {
            let loaded = run_repo
                .list_by_project(binding.project_id)
                .await
                .map_err(|e| format!("查询 lifecycle runs 失败: {e}"))?;
            runs_cache.insert(binding.project_id, loaded.clone());
            loaded
        };

        let run = runs.into_iter().find(|run| {
            run.step_states.iter().any(|state| {
                state.step_key == node_key && state.session_id.as_deref() == Some(session_id)
            })
        });
        if let Some(run) = run {
            return Ok(Some(LifecycleNodeSessionAssociation { run, node_key }));
        }
    }

    Ok(None)
}

/// 解析 session 是否为某个 lifecycle activity attempt 子 session。
///
/// Activity 子 session label 内含 run_id，因此反查不需要按项目扫描 runs；
/// 该 label 是 runtime 事件回写 ActivityEvent 的定位锚点。
pub async fn resolve_activity_session_association(
    session_id: &str,
    session_binding_repo: &dyn SessionBindingRepository,
    run_repo: &dyn LifecycleRunRepository,
) -> Result<Option<LifecycleActivitySessionAssociation>, String> {
    let bindings = session_binding_repo
        .list_by_session(session_id)
        .await
        .map_err(|e| format!("查询 session bindings 失败: {e}"))?;
    for binding in bindings {
        let Some(parts) = lifecycle_activity_parts_from_label(&binding.label) else {
            continue;
        };
        let Some(run) = run_repo
            .get_by_id(parts.run_id)
            .await
            .map_err(|e| format!("查询 lifecycle run 失败: {e}"))?
        else {
            continue;
        };
        if run.project_id != binding.project_id {
            continue;
        }
        return Ok(Some(LifecycleActivitySessionAssociation {
            run,
            activity_key: parts.activity_key,
            attempt: parts.attempt,
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_activity_label_roundtrips_run_activity_and_attempt() {
        let run_id = Uuid::new_v4();
        let label = build_lifecycle_activity_label(run_id, "plan", 2);

        assert_eq!(
            lifecycle_activity_parts_from_label(&label),
            Some(LifecycleActivityLabelParts {
                run_id,
                activity_key: "plan".to_string(),
                attempt: 2,
            })
        );
    }

    #[test]
    fn lifecycle_activity_label_rejects_incomplete_payload() {
        assert_eq!(
            lifecycle_activity_parts_from_label("lifecycle_activity:plan#1"),
            None
        );
        assert_eq!(
            lifecycle_activity_parts_from_label("lifecycle_activity:not-a-uuid:plan#1"),
            None
        );
        assert_eq!(
            lifecycle_activity_parts_from_label(
                "lifecycle_activity:00000000-0000-0000-0000-000000000000:plan"
            ),
            None
        );
    }
}
