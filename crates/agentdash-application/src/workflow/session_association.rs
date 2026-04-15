use std::collections::HashMap;

use agentdash_domain::session_binding::SessionBindingRepository;
use agentdash_domain::workflow::{LifecycleRun, LifecycleRunRepository};
use uuid::Uuid;

/// Lifecycle node 子 session 的 binding label 前缀。
pub const LIFECYCLE_NODE_LABEL_PREFIX: &str = "lifecycle_node:";

/// 子 session 与 lifecycle node 的关联解析结果。
#[derive(Debug, Clone)]
pub struct LifecycleNodeSessionAssociation {
    pub run: LifecycleRun,
    pub node_key: String,
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
