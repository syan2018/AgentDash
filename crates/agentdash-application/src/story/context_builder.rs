use agentdash_domain::context_source::ContextSourceKind;
use agentdash_domain::{project::Project, story::Story, workspace::Workspace};
use agentdash_spi::{ContextFragment, MergeStrategy, ResolveSourcesRequest};

use crate::context::{
    Contribution, WorkspaceFragmentMode, resolve_declared_sources, trim_or_dash,
    workspace_context_fragment,
};

/// Story Owner Session 的上下文构建输入
pub struct StoryContextBuildInput<'a> {
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    /// 由调用方预解析的工作空间来源片段（File/ProjectSnapshot 类型）
    pub workspace_source_fragments: Vec<ContextFragment>,
    pub workspace_source_warnings: Vec<String>,
}

/// 把 Story owner session 的业务上下文聚合为一个 `Contribution`，供
/// `build_session_context_bundle` 消费。
///
/// 不包含 SessionPlan fragments —— PR 5b 起所有 compose_* 在外层显式 push
/// SessionPlan，保持 contributor 单一职责且与 lifecycle / task 路径一致。
pub fn contribute_story_context(input: StoryContextBuildInput<'_>) -> Contribution {
    let mut fragments = Vec::new();

    fragments.push(ContextFragment {
        slot: "story".to_string(),
        label: "story_core".to_string(),
        order: 10,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:story_context".to_string(),
        content: format!(
            "## Story\n- id: {}\n- title: {}\n- description: {}\n- status: {:?}",
            input.story.id,
            trim_or_dash(&input.story.title),
            trim_or_dash(&input.story.description),
            input.story.status
        ),
    });
    fragments.push(ContextFragment {
        slot: "project".to_string(),
        label: "project_core".to_string(),
        order: 20,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:story_context".to_string(),
        content: format!(
            "## Project\n- id: {}\n- name: {}",
            input.project.id,
            trim_or_dash(&input.project.name),
        ),
    });
    if let Some(workspace) = input.workspace {
        fragments.push(workspace_context_fragment(
            workspace,
            WorkspaceFragmentMode::Owner,
        ));
    }

    let resolvable_sources = input
        .story
        .context
        .source_refs
        .iter()
        .filter(|source| {
            !matches!(
                source.kind,
                ContextSourceKind::File | ContextSourceKind::ProjectSnapshot
            )
        })
        .cloned()
        .collect::<Vec<_>>();

    if let Ok(resolved) = resolve_declared_sources(ResolveSourcesRequest {
        sources: &resolvable_sources,
        base_order: 50,
    }) {
        fragments.extend(resolved.fragments);
        if !resolved.warnings.is_empty() {
            fragments.push(ContextFragment {
                slot: "story_context".to_string(),
                label: "story_context_warnings".to_string(),
                order: 59,
                strategy: MergeStrategy::Append,
                scope: ContextFragment::default_scope(),
                source: "legacy:story_context".to_string(),
                content: format!(
                    "## Injection Notes\n{}",
                    resolved
                        .warnings
                        .iter()
                        .map(|item| format!("- {item}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            });
        }
    }

    fragments.extend(input.workspace_source_fragments);
    if !input.workspace_source_warnings.is_empty() {
        fragments.push(crate::context::build_declared_source_warning_fragment(
            "story_context_warnings",
            69,
            &input.workspace_source_warnings,
        ));
    }

    Contribution::fragments_only(fragments)
}
