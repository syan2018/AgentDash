//! Context builder — 统一的 session 上下文 Bundle 构建入口。
//!
//! 本模块的公共 API（`ContextBuildPhase` / `SessionContextConfig` /
//! `Contribution` / `build_session_context_bundle`）**不依赖任何 domain 类型**，
//! 是纯合并 reducer（参见 PRD D6 决策）。领域自治的 `contribute_*` 纯函数负责
//! 把 domain 对象解包成 `Contribution`，调用方按 phase 组装后喂给 builder。

use agentdash_spi::{ContextFragment, FragmentScopeSet, MergeStrategy, SessionContextBundle};
use uuid::Uuid;

use crate::runtime::RuntimeMcpServer;

// ─── 新契约（零 domain 依赖） ────────────────────────────────

/// Session 上下文构建的触发 phase。
///
/// 与 `SessionPlanPhase` 互补：`SessionPlanPhase` 只区分 project/task start/continue/story 四类，
/// 新增 phase 覆盖 owner bootstrap、lifecycle node、companion、repository rehydrate 等场景。
/// Task 执行路径的 phase 标签（`start_task` / `continue_task`）。
///
/// 主要用于 `contribute_instruction` 在首轮/续跑时选择合适的指令模板，
/// 以及标记 bundle 的构建来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskExecutionPhase {
    Start,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextBuildPhase {
    ProjectAgent,
    TaskStart,
    TaskContinue,
    StoryOwner,
    OwnerBootstrap,
    LifecycleNode,
    Companion,
    RepositoryRehydrate,
}

impl ContextBuildPhase {
    /// 返回 snake_case 字符串标签，供 `SessionContextBundle.phase_tag` 使用。
    pub fn as_tag(&self) -> &'static str {
        match self {
            ContextBuildPhase::ProjectAgent => "project_agent",
            ContextBuildPhase::TaskStart => "task_start",
            ContextBuildPhase::TaskContinue => "task_continue",
            ContextBuildPhase::StoryOwner => "story_owner",
            ContextBuildPhase::OwnerBootstrap => "owner_bootstrap",
            ContextBuildPhase::LifecycleNode => "lifecycle_node",
            ContextBuildPhase::Companion => "companion",
            ContextBuildPhase::RepositoryRehydrate => "repository_rehydrate",
        }
    }
}

/// `build_session_context_bundle` 的全局构建配置。
///
/// 持有 session 身份 + phase 标签 + 未显式声明 scope 的 fragment 的默认值。
pub struct SessionContextConfig {
    pub session_id: Uuid,
    pub phase: ContextBuildPhase,
    pub default_scope: FragmentScopeSet,
}

/// Contribution 是所有领域向 builder 投递 fragment 的标准契约。
///
/// `fragments` 承载 context fragment，`mcp_servers` 承载需要在 runtime 侧注册的 MCP server
/// 声明（例如 `contribute_mcp` 产出的 ACP MCP server 声明）。
pub struct Contribution {
    pub fragments: Vec<ContextFragment>,
    /// application 层 MCP server 抽象 — 边界层再转换为具体协议类型。
    pub mcp_servers: Vec<RuntimeMcpServer>,
}

impl Contribution {
    pub fn empty() -> Self {
        Self {
            fragments: vec![],
            mcp_servers: vec![],
        }
    }

    pub fn fragments_only(fragments: Vec<ContextFragment>) -> Self {
        Self {
            fragments,
            mcp_servers: vec![],
        }
    }
}

/// 纯合并函数 —— 把一组 `Contribution` fold 成单一 `SessionContextBundle`。
///
/// 行为：
/// 1. flatten 所有 contribution 的 fragment；
/// 2. 对未显式声明 scope（`scope.is_empty()`）的 fragment 套用 `config.default_scope`；
///    若 `default_scope` 也为空则退化到 `ContextFragment::default_scope()`；
/// 3. 依次走 `SessionContextBundle::upsert_by_slot` 做 slot 级合并（承接 user-prompt
///    注入去重语义）；
/// 4. 最后按 `fragment.order` 升序排序。
///
/// **重要**：此函数不依赖任何 domain 类型，是依赖倒置后的 reducer。
pub fn build_session_context_bundle(
    config: SessionContextConfig,
    contributions: Vec<Contribution>,
) -> SessionContextBundle {
    let effective_default_scope = if config.default_scope.is_empty() {
        ContextFragment::default_scope()
    } else {
        config.default_scope
    };

    let mut bundle = SessionContextBundle::new(config.session_id, config.phase.as_tag());

    for contribution in contributions {
        for mut fragment in contribution.fragments {
            if fragment.scope.is_empty() {
                fragment.scope = effective_default_scope;
            }
            bundle.upsert_by_slot(fragment);
        }
    }

    bundle
        .bootstrap_fragments
        .sort_by_key(|fragment| fragment.order);
    bundle
}

// ─── 辅助 fragment 构造器 ────────────────────────────────────────

/// 把预构建的 continuation Markdown 包装成 `SessionContextBundle`。
///
/// 当 session 冷启动进入 RepositoryRehydrate 且 connector 不支持原生 repository
/// restore 时，SessionHub 先根据历史事件渲染出连贯的 Markdown，再由此函数
/// 封装为 Bundle 供 connector 层统一消费。
///
/// 产出的 Bundle 含单条 fragment：slot=`static_fragment`、scope=默认、
/// source=`session:continuation`。
pub fn build_continuation_bundle_from_markdown(
    session_id: Uuid,
    markdown: String,
) -> SessionContextBundle {
    let mut bundle =
        SessionContextBundle::new(session_id, ContextBuildPhase::RepositoryRehydrate.as_tag());
    if markdown.trim().is_empty() {
        return bundle;
    }
    bundle.upsert_by_slot(ContextFragment {
        slot: "static_fragment".to_string(),
        label: "continuation_transcript".to_string(),
        order: 0,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "session:continuation".to_string(),
        content: markdown,
    });
    bundle
}

pub fn build_declared_source_warning_fragment(
    label: &'static str,
    order: i32,
    warnings: &[String],
) -> ContextFragment {
    ContextFragment {
        slot: "references".to_string(),
        label: label.to_string(),
        order,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:declared_source_warning".to_string(),
        content: format!(
            "## Injection Notes\n{}",
            warnings
                .iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

#[cfg(test)]
mod bundle_tests {
    use super::*;
    use agentdash_spi::FragmentScope;

    fn frag(slot: &str, order: i32, content: &str, strategy: MergeStrategy) -> ContextFragment {
        ContextFragment {
            slot: slot.to_string(),
            label: format!("label_{slot}"),
            order,
            strategy,
            scope: ContextFragment::default_scope(),
            source: "test".to_string(),
            content: content.to_string(),
        }
    }

    fn frag_default_scope_empty(slot: &str, order: i32, content: &str) -> ContextFragment {
        let mut f = frag(slot, order, content, MergeStrategy::Append);
        f.scope = FragmentScopeSet::empty();
        f
    }

    #[test]
    fn empty_contributions_yields_empty_bundle() {
        let config = SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::TaskStart,
            default_scope: ContextFragment::default_scope(),
        };
        let bundle = build_session_context_bundle(config, vec![]);
        assert!(bundle.bootstrap_fragments.is_empty());
        assert!(bundle.turn_delta.is_empty());
        assert_eq!(bundle.phase_tag, "task_start");
    }

    #[test]
    fn same_slot_append_merges_content() {
        let config = SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::StoryOwner,
            default_scope: ContextFragment::default_scope(),
        };
        let a =
            Contribution::fragments_only(vec![frag("task", 10, "alpha", MergeStrategy::Append)]);
        let b = Contribution::fragments_only(vec![frag("task", 20, "beta", MergeStrategy::Append)]);
        let bundle = build_session_context_bundle(config, vec![a, b]);
        assert_eq!(bundle.bootstrap_fragments.len(), 1);
        let merged = &bundle.bootstrap_fragments[0];
        assert!(merged.content.contains("alpha"));
        assert!(merged.content.contains("beta"));
    }

    #[test]
    fn same_slot_override_replaces_content() {
        let config = SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::TaskContinue,
            default_scope: ContextFragment::default_scope(),
        };
        let first = Contribution::fragments_only(vec![frag(
            "instruction",
            10,
            "first",
            MergeStrategy::Append,
        )]);
        let second = Contribution::fragments_only(vec![frag(
            "instruction",
            20,
            "second",
            MergeStrategy::Override,
        )]);
        let bundle = build_session_context_bundle(config, vec![first, second]);
        assert_eq!(bundle.bootstrap_fragments.len(), 1);
        assert_eq!(bundle.bootstrap_fragments[0].content, "second");
    }

    #[test]
    fn empty_scope_gets_default_scope() {
        let config = SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::OwnerBootstrap,
            default_scope: FragmentScope::RuntimeAgent | FragmentScope::TitleGen,
        };
        let contribution =
            Contribution::fragments_only(vec![frag_default_scope_empty("task", 10, "alpha")]);
        let bundle = build_session_context_bundle(config, vec![contribution]);
        assert_eq!(bundle.bootstrap_fragments.len(), 1);
        let f = &bundle.bootstrap_fragments[0];
        assert!(f.scope.contains(FragmentScope::RuntimeAgent));
        assert!(f.scope.contains(FragmentScope::TitleGen));
    }

    /// 回归固化：`bce0825` 场景 —— title generator 不应看到 RuntimeAgent scope 的 agent context
    /// fragment。换言之，所有 `legacy:session_plan` / `legacy:contributor:*` / `contribute_*`
    /// 产出 fragment 的 scope 必须是 `RuntimeAgent | Audit`（**不含** `TitleGen`），
    /// 从协议层防止 agent 指令泄漏到 title 生成路径。
    #[test]
    fn title_gen_scope_excludes_agent_context() {
        let config = SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::StoryOwner,
            default_scope: ContextFragment::default_scope(),
        };
        // 模拟典型 contribute_* 产出 —— 未标记 TitleGen
        let fragments = vec![
            frag("task", 10, "task body", MergeStrategy::Append),
            frag("story", 20, "story body", MergeStrategy::Append),
            frag(
                "instruction",
                30,
                "agent instruction",
                MergeStrategy::Append,
            ),
            frag("workflow", 40, "workflow binding", MergeStrategy::Append),
        ];
        let bundle =
            build_session_context_bundle(config, vec![Contribution::fragments_only(fragments)]);

        // 验证：默认 scope 不含 TitleGen
        for f in &bundle.bootstrap_fragments {
            assert!(
                !f.scope.contains(FragmentScope::TitleGen),
                "fragment {}({}) 不应带 TitleGen scope —— 会导致 agent 指令泄漏到 title gen 路径",
                f.label,
                f.slot
            );
        }

        // 验证：filter_for(TitleGen) 应为空
        let title_visible: Vec<_> = bundle.filter_for(FragmentScope::TitleGen).collect();
        assert!(
            title_visible.is_empty(),
            "title_gen scope 过滤结果应为空，实际找到 {} 条",
            title_visible.len()
        );

        // 验证：RuntimeAgent scope 的 fragment 都在（用于系统 prompt）
        let runtime_visible: Vec<_> = bundle.filter_for(FragmentScope::RuntimeAgent).collect();
        assert_eq!(runtime_visible.len(), bundle.bootstrap_fragments.len());
    }

    /// 回归固化：显式声明 TitleGen scope 的 fragment 才能进入 title gen 视图。
    ///
    /// 这是 `title_gen_scope_excludes_agent_context` 的对偶测试 —— 保证 scope 机制不是一刀切
    /// 关掉 TitleGen 入口，而是按协议精确控制。
    #[test]
    fn title_gen_scope_allows_explicit_opt_in() {
        let config = SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::StoryOwner,
            default_scope: ContextFragment::default_scope(),
        };

        // 默认 fragment
        let mut runtime_frag = frag("task", 10, "task body", MergeStrategy::Append);
        assert_eq!(runtime_frag.scope, ContextFragment::default_scope());

        // 显式标记 TitleGen 的 fragment
        let mut title_frag = frag("title_hint", 20, "会话标题提示", MergeStrategy::Append);
        title_frag.scope =
            FragmentScope::RuntimeAgent | FragmentScope::TitleGen | FragmentScope::Audit;
        // 同时修改 runtime_frag 的 scope，避免 slot 冲突（slot 名不同已经隔离）
        runtime_frag.label = "runtime_task".to_string();

        let bundle = build_session_context_bundle(
            config,
            vec![Contribution::fragments_only(vec![runtime_frag, title_frag])],
        );

        let title_visible: Vec<_> = bundle.filter_for(FragmentScope::TitleGen).collect();
        assert_eq!(title_visible.len(), 1);
        assert_eq!(title_visible[0].slot, "title_hint");
    }

    #[test]
    fn fragments_sorted_by_order() {
        let config = SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::StoryOwner,
            default_scope: ContextFragment::default_scope(),
        };
        let contribution = Contribution::fragments_only(vec![
            frag("z", 30, "z_body", MergeStrategy::Append),
            frag("a", 10, "a_body", MergeStrategy::Append),
            frag("m", 20, "m_body", MergeStrategy::Append),
        ]);
        let bundle = build_session_context_bundle(config, vec![contribution]);
        let orders: Vec<i32> = bundle
            .bootstrap_fragments
            .iter()
            .map(|f| f.order)
            .collect();
        assert_eq!(orders, vec![10, 20, 30]);
    }
}
