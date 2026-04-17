use agentdash_domain::context_source::ContextSourceKind;
use agentdash_domain::{project::Project, story::Story, workspace::Workspace};
use agentdash_spi::{ContextFragment, MergeStrategy, ResolveSourcesRequest};

use crate::context::{
    ContextComposer, build_owner_prompt_blocks, resolve_declared_sources, trim_or_dash,
    workspace_context_fragment,
};
use crate::runtime::{Vfs, RuntimeMcpServer};
use crate::session::plan::{
    SessionOwnerType, SessionPlanInput, SessionPlanPhase, build_session_plan_fragments,
    resolve_story_session_composition,
};

/// Story Owner Session 的上下文构建输入
pub struct StoryContextBuildInput<'a> {
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub vfs: Option<&'a Vfs>,
    pub mcp_servers: &'a [RuntimeMcpServer],
    pub effective_agent_type: Option<&'a str>,
    /// 由调用方预解析的工作空间来源片段（File/ProjectSnapshot 类型）
    pub workspace_source_fragments: Vec<ContextFragment>,
    pub workspace_source_warnings: Vec<String>,
}

/// 构建 Story Owner Session 的 context markdown + source summary
///
/// 此函数不依赖任何基础设施，所有需要基础设施（仓库、中继）的数据
/// 由调用方预先获取并通过 `StoryContextBuildInput` 传入。
pub fn build_story_context_markdown(input: StoryContextBuildInput<'_>) -> (String, Vec<String>) {
    let mut composer = ContextComposer::default();

    composer.push(
        "story",
        "story_core",
        10,
        MergeStrategy::Append,
        format!(
            "## Story\n- id: {}\n- title: {}\n- description: {}\n- status: {:?}",
            input.story.id,
            trim_or_dash(&input.story.title),
            trim_or_dash(&input.story.description),
            input.story.status
        ),
    );
    composer.push(
        "project",
        "project_core",
        20,
        MergeStrategy::Append,
        format!(
            "## Project\n- id: {}\n- name: {}",
            input.project.id,
            trim_or_dash(&input.project.name),
        ),
    );
    if let Some(workspace) = input.workspace {
        composer.push_fragment(workspace_context_fragment(workspace));
    }

    let effective_session_composition = resolve_story_session_composition(Some(input.story));
    let session_plan = build_session_plan_fragments(SessionPlanInput {
        owner_type: SessionOwnerType::Story,
        phase: SessionPlanPhase::StoryOwner,
        vfs: input.vfs,
        mcp_servers: input.mcp_servers,
        session_composition: effective_session_composition.as_ref(),
        agent_type: input.effective_agent_type,
        preset_name: None,
        has_custom_prompt_template: false,
        has_initial_context: false,
        workspace_attached: input.workspace.is_some(),
    });
    for fragment in session_plan.fragments {
        composer.push_fragment(fragment);
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
        for fragment in resolved.fragments {
            composer.push_fragment(fragment);
        }
        if !resolved.warnings.is_empty() {
            composer.push(
                "story_context",
                "story_context_warnings",
                59,
                MergeStrategy::Append,
                format!(
                    "## Injection Notes\n{}",
                    resolved
                        .warnings
                        .iter()
                        .map(|item| format!("- {item}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            );
        }
    }

    for fragment in input.workspace_source_fragments {
        composer.push_fragment(fragment);
    }
    if !input.workspace_source_warnings.is_empty() {
        composer.push_fragment(crate::context::build_declared_source_warning_fragment(
            "story_context_warnings",
            69,
            &input.workspace_source_warnings,
        ));
    }

    composer.compose()
}

/// 构建 Story Owner 的 prompt_blocks
///
/// 将 context markdown 打包为 resource block + user blocks。
pub fn build_story_owner_prompt_blocks(
    story_id: uuid::Uuid,
    context_markdown: String,
    user_prompt_blocks: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    build_owner_prompt_blocks(
        &format!("agentdash://story-context/{}", story_id),
        &context_markdown,
        user_prompt_blocks,
    )
}
