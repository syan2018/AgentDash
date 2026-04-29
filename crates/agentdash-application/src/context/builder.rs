//! Context builder — 统一的 session 上下文 Bundle 构建入口。
//!
//! 本模块的 **新增公共 API**（`ContextBuildPhase` / `SessionContextConfig` /
//! `Contribution` / `build_session_context_bundle`）**不依赖任何 domain 类型**，
//! 是纯合并 reducer（参见 PRD D6 决策）。领域自治的 `contribute_*` 纯函数负责
//! 把 domain 对象解包成 `Contribution`，调用方按 phase 组装后喂给 builder。
//!
//! 旧函数 `build_task_agent_context` 保留过渡，仍依赖 domain；Step 6 才做替换。

use agentdash_spi::{
    ContextFragment, FragmentScopeSet, MergeStrategy, SessionContextBundle,
};
use uuid::Uuid;

use crate::runtime::RuntimeMcpServer;

// ─── 新契约（零 domain 依赖） ────────────────────────────────

/// Session 上下文构建的触发 phase。
///
/// 与 `SessionPlanPhase` 互补：`SessionPlanPhase` 只区分 project/task start/continue/story 四类，
/// 新增 phase 覆盖 owner bootstrap、lifecycle node、companion、repository rehydrate 等场景。
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
/// 3. 依次走 `SessionContextBundle::upsert_by_slot` 做 slot 级合并
///    （替代老的 `filter_user_prompt_injections` / `SESSION_BASELINE_INJECTION_SLOTS`）；
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

    bundle.fragments.sort_by_key(|fragment| fragment.order);
    bundle
}

// ─── 旧 API：build_task_agent_context（过渡期保留，允许依赖 domain） ─────

use super::ContextComposer;
use serde_json::Value;

use super::builtins::build_owner_context_resource_block;
use super::contributor::{
    BuiltTaskAgentContext, ContextContributorRegistry, ContributorInput, TaskAgentBuildInput,
    TaskExecutionPhase,
};
use crate::session::plan::{
    SessionPlanInput, SessionPlanPhase, build_session_plan_fragments,
    resolve_story_session_composition,
};

pub fn build_task_agent_context(
    input: TaskAgentBuildInput<'_>,
    registry: &ContextContributorRegistry,
) -> Result<BuiltTaskAgentContext, String> {
    let contributor_input = ContributorInput {
        task: input.task,
        story: input.story,
        project: input.project,
        workspace: input.workspace,
        phase: input.phase,
        override_prompt: input.override_prompt,
        additional_prompt: input.additional_prompt,
    };

    let working_dir = input.workspace.map(|_| ".".to_string());

    let mut context_composer = ContextComposer::default();
    let mut mcp_servers = Vec::new();

    let all_contributors = registry
        .contributors
        .iter()
        .map(|c| c.as_ref())
        .chain(input.extra_contributors.iter().map(|c| c.as_ref()));

    for contributor in all_contributors {
        let contribution = contributor.contribute(&contributor_input);

        mcp_servers.extend(contribution.mcp_servers);

        for fragment in contribution.fragments {
            if !matches!(fragment.slot.as_str(), "instruction" | "instruction_append") {
                context_composer.push_fragment(fragment);
            }
        }
    }

    let effective_session_composition = resolve_story_session_composition(Some(input.story));
    let session_plan = build_session_plan_fragments(SessionPlanInput {
        owner_ctx: agentdash_domain::session_binding::SessionOwnerCtx::Task {
            project_id: input.project.id,
            story_id: input.story.id,
            task_id: input.task.id,
        },
        phase: match input.phase {
            TaskExecutionPhase::Start => SessionPlanPhase::TaskStart,
            TaskExecutionPhase::Continue => SessionPlanPhase::TaskContinue,
        },
        vfs: input.vfs,
        mcp_servers: &mcp_servers,
        session_composition: effective_session_composition.as_ref(),
        agent_type: input.effective_agent_type,
        preset_name: input.task.agent_binding.preset_name.as_deref(),
        has_custom_prompt_template: input
            .task
            .agent_binding
            .prompt_template
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        has_initial_context: input
            .task
            .agent_binding
            .initial_context
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        workspace_attached: input.workspace.is_some(),
    });
    for fragment in session_plan.fragments {
        context_composer.push_fragment(fragment);
    }

    let (context_prompt, source_summary) = context_composer.compose();

    if context_prompt.trim().is_empty() {
        return Err("构建执行上下文失败：最终 prompt 为空".to_string());
    }

    let system_context = context_prompt.clone();
    let mut prompt_blocks = Vec::new();
    if !context_prompt.trim().is_empty() {
        prompt_blocks.push(build_task_context_resource_block(
            input.task.id.to_string(),
            input.phase,
            context_prompt,
        ));
    }

    Ok(BuiltTaskAgentContext {
        prompt_blocks,
        working_dir,
        source_summary,
        mcp_servers,
        system_context: Some(system_context),
    })
}

fn build_task_context_resource_block(
    task_id: String,
    phase: TaskExecutionPhase,
    markdown: String,
) -> Value {
    let phase_label = match phase {
        TaskExecutionPhase::Start => "start",
        TaskExecutionPhase::Continue => "continue",
    };
    build_owner_context_resource_block(
        &format!("agentdash://task-context/{task_id}?phase={phase_label}"),
        &markdown,
    )
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

    fn frag_default_scope_empty(
        slot: &str,
        order: i32,
        content: &str,
    ) -> ContextFragment {
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
        assert!(bundle.fragments.is_empty());
        assert_eq!(bundle.phase_tag, "task_start");
    }

    #[test]
    fn same_slot_append_merges_content() {
        let config = SessionContextConfig {
            session_id: Uuid::new_v4(),
            phase: ContextBuildPhase::StoryOwner,
            default_scope: ContextFragment::default_scope(),
        };
        let a = Contribution::fragments_only(vec![frag(
            "task",
            10,
            "alpha",
            MergeStrategy::Append,
        )]);
        let b = Contribution::fragments_only(vec![frag(
            "task",
            20,
            "beta",
            MergeStrategy::Append,
        )]);
        let bundle = build_session_context_bundle(config, vec![a, b]);
        assert_eq!(bundle.fragments.len(), 1);
        let merged = &bundle.fragments[0];
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
        assert_eq!(bundle.fragments.len(), 1);
        assert_eq!(bundle.fragments[0].content, "second");
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
        assert_eq!(bundle.fragments.len(), 1);
        let f = &bundle.fragments[0];
        assert!(f.scope.contains(FragmentScope::RuntimeAgent));
        assert!(f.scope.contains(FragmentScope::TitleGen));
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
        let orders: Vec<i32> = bundle.fragments.iter().map(|f| f.order).collect();
        assert_eq!(orders, vec![10, 20, 30]);
    }
}
