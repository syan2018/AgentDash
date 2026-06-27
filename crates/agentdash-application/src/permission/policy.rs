//! Permission Policy Engine — 评估 Agent 权限申请是否可自动批准。
//!
//! 数据来源：
//! 1. ProjectAgent.config.auto_grantable_capabilities — Agent 角色声明的可自动获批范围
//! 2. AgentProcedureContract.requestable_capabilities — Lifecycle 定义声明的运行时可申请范围
//! 3. 合并策略：两者取交集 → 自动批准；不在交集内 → 需要用户审批

use agentdash_domain::permission::{PolicyDecision, PolicyOutcome};
use agentdash_domain::workflow::ToolCapabilityPath;

/// Policy 评估服务。
///
/// 接收 Agent 配置中声明的自动授权范围和 Lifecycle 声明的可申请范围，
/// 判断请求的 paths 是否可以自动批准。
pub struct PermissionPolicyService;

impl PermissionPolicyService {
    /// 评估请求的 paths 是否可以自动批准。
    ///
    /// - `requested_paths`: Agent 申请的 capability paths
    /// - `agent_auto_grantable`: Agent 配置中声明的可自动授权 paths（支持 `*` 通配）
    /// - `lifecycle_requestable`: Lifecycle contract 声明的运行时可申请 paths（支持 `*` 通配）
    pub fn evaluate(
        requested_paths: &[ToolCapabilityPath],
        agent_auto_grantable: &[ToolCapabilityPath],
        lifecycle_requestable: &[ToolCapabilityPath],
    ) -> PolicyDecision {
        if requested_paths.is_empty() {
            return PolicyDecision {
                outcome: PolicyOutcome::Rejected,
                matched_rules: vec![],
                reason: "requested_paths is empty".to_string(),
            };
        }

        // 计算 policy 允许自动批准的范围：agent 声明 ∩ lifecycle 声明
        // 如果任一为空，则全部需要用户审批
        let auto_approve_pool =
            compute_auto_approve_pool(agent_auto_grantable, lifecycle_requestable);

        let mut matched = Vec::new();
        let mut unmatched = Vec::new();

        for path in requested_paths {
            if is_path_covered_by_pool(path, &auto_approve_pool) {
                matched.push(path.to_qualified_string());
            } else {
                unmatched.push(path.to_qualified_string());
            }
        }

        if unmatched.is_empty() {
            PolicyDecision {
                outcome: PolicyOutcome::AutoApproved,
                matched_rules: matched,
                reason: "all paths covered by agent_role ∩ lifecycle_contract".to_string(),
            }
        } else if matched.is_empty() {
            PolicyDecision {
                outcome: PolicyOutcome::NeedsUserApproval,
                matched_rules: vec![],
                reason: format!("no auto-approve coverage for: {}", unmatched.join(", ")),
            }
        } else {
            // 部分命中：保守策略，全部走用户审批
            PolicyDecision {
                outcome: PolicyOutcome::NeedsUserApproval,
                matched_rules: matched,
                reason: format!("partial coverage; unmatched: {}", unmatched.join(", ")),
            }
        }
    }

    /// 从 ProjectAgent config JSON 中提取 auto_grantable_capabilities。
    pub fn extract_agent_grantable(config: &serde_json::Value) -> Vec<ToolCapabilityPath> {
        config
            .get("auto_grantable_capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| ToolCapabilityPath::parse(s).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 从 AgentProcedureContract JSON 中提取 requestable_capabilities。
    pub fn extract_lifecycle_requestable(contract: &serde_json::Value) -> Vec<ToolCapabilityPath> {
        contract
            .get("requestable_capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| ToolCapabilityPath::parse(s).ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// 计算自动批准池：agent 声明和 lifecycle 声明的交集。
/// 如果任一为空，返回空（无自动批准能力）。
fn compute_auto_approve_pool(
    agent_paths: &[ToolCapabilityPath],
    lifecycle_paths: &[ToolCapabilityPath],
) -> Vec<ToolCapabilityPath> {
    if agent_paths.is_empty() || lifecycle_paths.is_empty() {
        return vec![];
    }

    // 交集：agent 中的每个 path 必须被 lifecycle 中的某个 path 覆盖，且反之亦然
    // 简化实现：取 agent 中被 lifecycle 覆盖的 paths
    agent_paths
        .iter()
        .filter(|ap| lifecycle_paths.iter().any(|lp| path_covers(lp, ap)))
        .cloned()
        .collect()
}

/// 判断 requested path 是否被 pool 中的某个 path 覆盖。
fn is_path_covered_by_pool(requested: &ToolCapabilityPath, pool: &[ToolCapabilityPath]) -> bool {
    pool.iter().any(|p| path_covers(p, requested))
}

/// 判断 `covering` 是否覆盖 `target`。
///
/// 覆盖规则：
/// - capability 级 path（无 tool）覆盖该 capability 下的所有 tool paths
/// - 精确匹配
/// - `*` 通配符匹配所有（capability 为 `*` 则覆盖一切）
fn path_covers(covering: &ToolCapabilityPath, target: &ToolCapabilityPath) -> bool {
    // 通配符覆盖
    if covering.capability == "*" {
        return true;
    }

    if covering.capability != target.capability {
        return false;
    }

    match (&covering.tool, &target.tool) {
        // covering 是 capability 级 → 覆盖该 capability 下所有 tool
        (None, _) => true,
        // covering 是 tool 级 `*` → 覆盖该 capability 下所有 tool
        (Some(ct), _) if ct == "*" => true,
        // 精确匹配
        (Some(ct), Some(tt)) => ct == tt,
        // covering 有 tool 但 target 是 capability 级 → 不覆盖
        (Some(_), None) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(s: &str) -> ToolCapabilityPath {
        ToolCapabilityPath::parse(s).unwrap()
    }

    #[test]
    fn auto_approve_when_fully_covered() {
        let result = PermissionPolicyService::evaluate(
            &[path("story_management")],
            &[path("story_management"), path("task")],
            &[path("story_management")],
        );
        assert_eq!(result.outcome, PolicyOutcome::AutoApproved);
    }

    #[test]
    fn needs_user_approval_when_not_covered() {
        let result = PermissionPolicyService::evaluate(
            &[path("workflow_management")],
            &[path("story_management")],
            &[path("story_management")],
        );
        assert_eq!(result.outcome, PolicyOutcome::NeedsUserApproval);
    }

    #[test]
    fn needs_user_when_agent_pool_empty() {
        let result = PermissionPolicyService::evaluate(
            &[path("story_management")],
            &[],
            &[path("story_management")],
        );
        assert_eq!(result.outcome, PolicyOutcome::NeedsUserApproval);
    }

    #[test]
    fn capability_level_covers_tool_level() {
        let result = PermissionPolicyService::evaluate(
            &[path("story_management::create_story")],
            &[path("story_management")],
            &[path("story_management")],
        );
        assert_eq!(result.outcome, PolicyOutcome::AutoApproved);
    }

    #[test]
    fn wildcard_tool_covers_all() {
        assert!(path_covers(
            &path("story_management"),
            &path("story_management::create_story")
        ));
    }

    #[test]
    fn extract_agent_grantable_from_json() {
        let config = serde_json::json!({
            "auto_grantable_capabilities": ["story_management", "task::write"]
        });
        let result = PermissionPolicyService::extract_agent_grantable(&config);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].capability, "story_management");
    }

    #[test]
    fn empty_requested_paths_rejected() {
        let result = PermissionPolicyService::evaluate(&[], &[path("x")], &[path("x")]);
        assert_eq!(result.outcome, PolicyOutcome::Rejected);
    }
}
