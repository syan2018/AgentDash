//! Hook 注入 → ContextFragment / Contribution 转换桥。
//!
//! 把 Hook 链路产出的 `HookInjection` / `SessionHookSnapshot` 转换为统一的
//! `ContextFragment` / `Contribution`，让 Hook 数据可以与其他 contribution 一视同仁
//! 地进入 `build_session_context_bundle`。Bundle 的 `upsert_by_slot` 承担去重语义。

use agentdash_spi::{ContextFragment, HookInjection, MergeStrategy, SessionHookSnapshot};

use crate::context::Contribution;
use crate::context::slot_orders;

/// 已知 Hook slot → order 的固定映射。
///
/// 未覆盖的 slot 使用 `slot_orders::HOOK_DEFAULT`。order 数值来自
/// `context::slot_orders`，与 contributor 产出的 fragment order 单源化，
/// 保证 hook fragment 在 bundle 排序中位于期望区段（workflow/constraint 与
/// lifecycle / contribute_workflow_binding 同位）。
const HOOK_SLOT_ORDERS: &[(&str, i32)] = &[
    ("companion_agents", slot_orders::HOOK_COMPANION_AGENTS),
    ("workflow", slot_orders::HOOK_WORKFLOW),
    ("constraint", slot_orders::HOOK_CONSTRAINT),
];

fn default_hook_order(slot: &str) -> i32 {
    HOOK_SLOT_ORDERS
        .iter()
        .find(|(name, _)| *name == slot)
        .map(|(_, ord)| *ord)
        .unwrap_or(slot_orders::HOOK_DEFAULT)
}

/// 把单个 `HookInjection` 转换为 `ContextFragment`。
///
/// 因 orphan rule，Rust 不允许在此 crate 对 `HookInjection → ContextFragment`
/// 直接 `impl From`（两端都在 `agentdash-spi`）。故以自由函数 + 本地 `Contribution`
/// 的 `From<&SessionHookSnapshot>` 形式完成桥接。
pub fn hook_injection_to_fragment(injection: HookInjection) -> ContextFragment {
    let order = default_hook_order(&injection.slot);
    ContextFragment {
        slot: injection.slot,
        label: injection.source.clone(),
        order,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: injection.source,
        content: injection.content,
    }
}

impl From<&SessionHookSnapshot> for Contribution {
    fn from(snapshot: &SessionHookSnapshot) -> Self {
        Contribution::fragments_only(
            snapshot
                .injections
                .iter()
                .cloned()
                .map(hook_injection_to_fragment)
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_agents_slot_maps_to_order_60() {
        let injection = HookInjection {
            slot: "companion_agents".to_string(),
            content: "## Companion Agents\n- agent_a".to_string(),
            source: "builtin:companion_agents".to_string(),
        };
        let fragment = hook_injection_to_fragment(injection);
        assert_eq!(fragment.slot, "companion_agents");
        assert_eq!(fragment.order, 60);
        assert_eq!(fragment.source, "builtin:companion_agents");
        assert_eq!(fragment.label, "builtin:companion_agents");
        assert!(fragment.content.contains("agent_a"));
        assert!(matches!(fragment.strategy, MergeStrategy::Append));
    }

    #[test]
    fn workflow_slot_maps_to_order_83() {
        let injection = HookInjection {
            slot: "workflow".to_string(),
            content: "body".to_string(),
            source: "workflow:trellis_dev_task:implement".to_string(),
        };
        let fragment = hook_injection_to_fragment(injection);
        assert_eq!(fragment.order, 83);
    }

    #[test]
    fn constraint_slot_maps_to_order_84() {
        let injection = HookInjection {
            slot: "constraint".to_string(),
            content: "must x".to_string(),
            source: "builtin:constraint".to_string(),
        };
        let fragment = hook_injection_to_fragment(injection);
        assert_eq!(fragment.order, 84);
    }

    #[test]
    fn unknown_slot_maps_to_default_order_200() {
        let injection = HookInjection {
            slot: "custom_hook_slot".to_string(),
            content: "hello".to_string(),
            source: "custom:rule".to_string(),
        };
        let fragment = hook_injection_to_fragment(injection);
        assert_eq!(fragment.order, 200);
        assert_eq!(fragment.slot, "custom_hook_slot");
        assert_eq!(fragment.content, "hello");
    }

    #[test]
    fn snapshot_injections_map_to_contribution() {
        let snapshot = SessionHookSnapshot {
            session_id: "sess-1".to_string(),
            injections: vec![
                HookInjection {
                    slot: "companion_agents".to_string(),
                    content: "a".to_string(),
                    source: "src_a".to_string(),
                },
                HookInjection {
                    slot: "custom".to_string(),
                    content: "b".to_string(),
                    source: "src_b".to_string(),
                },
            ],
            ..Default::default()
        };
        let contribution: Contribution = (&snapshot).into();
        assert_eq!(contribution.fragments.len(), 2);
        assert!(contribution.mcp_servers.is_empty());
        assert_eq!(contribution.fragments[0].order, 60);
        assert_eq!(contribution.fragments[1].order, 200);
    }
}
