use agent_client_protocol::McpServer;
use agentdash_domain::context_source::ContextSourceKind;
use agentdash_domain::{project::Project, story::Story, workspace::Workspace};
use agentdash_executor::ExecutionAddressSpace;
use agentdash_injection::{
    ContextComposer, ContextFragment, MergeStrategy, ResolveSourcesRequest,
    resolve_declared_sources,
};
use serde_json::json;

use crate::address_space::selected_workspace_binding;
use crate::session_plan::{
    SessionPlanInput, SessionPlanOwnerKind, SessionPlanPhase, build_session_plan_fragments,
    resolve_effective_session_composition,
};

/// Story Owner Session 的上下文构建输入
pub struct StoryContextBuildInput<'a> {
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub address_space: Option<&'a ExecutionAddressSpace>,
    pub mcp_servers: &'a [McpServer],
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
        let binding_summary = selected_workspace_binding(workspace)
            .map(|binding| format!(
                "{} @ {}",
                trim_or_dash(&binding.backend_id),
                trim_or_dash(&binding.root_ref)
            ))
            .unwrap_or_else(|| "-".to_string());
        composer.push(
            "workspace",
            "workspace_context",
            30,
            MergeStrategy::Append,
            format!(
                "## Workspace\n- id: {}\n- identity_kind: {:?}\n- name: {}\n- binding: {}\n- working_dir: .",
                workspace.id,
                workspace.identity_kind,
                trim_or_dash(&workspace.name),
                binding_summary,
            ),
        );
    }

    let effective_session_composition =
        resolve_effective_session_composition(input.project, Some(input.story));
    let session_plan = build_session_plan_fragments(SessionPlanInput {
        owner_kind: SessionPlanOwnerKind::StoryOwner,
        phase: SessionPlanPhase::StoryOwner,
        address_space: input.address_space,
        mcp_servers: input.mcp_servers,
        session_composition: Some(&effective_session_composition),
        agent_type: input.effective_agent_type,
        preset_name: None,
        has_custom_prompt_template: false,
        has_initial_context: false,
        workspace_attached: input.workspace.is_some(),
    });
    for fragment in session_plan.fragments {
        composer.push_fragment(fragment);
    }

    if let Some(prd) = clean_text(input.story.context.prd_doc.as_deref()) {
        composer.push(
            "story_context",
            "story_prd",
            40,
            MergeStrategy::Append,
            format!("## Story PRD\n{prd}"),
        );
    }
    if !input.story.context.spec_refs.is_empty() {
        composer.push(
            "story_context",
            "story_spec_refs",
            41,
            MergeStrategy::Append,
            format!(
                "## Spec Refs\n{}",
                input
                    .story
                    .context
                    .spec_refs
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        );
    }
    if !input.story.context.resource_list.is_empty() {
        composer.push(
            "story_context",
            "story_resources",
            42,
            MergeStrategy::Append,
            format!(
                "## Resources\n{}",
                input
                    .story
                    .context
                    .resource_list
                    .iter()
                    .map(|item| format!("- [{}] {} ({})", item.resource_type, item.name, item.uri))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        );
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
        workspace_root: None,
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
    original_prompt: Option<String>,
    original_prompt_blocks: Option<Vec<serde_json::Value>>,
) -> Vec<serde_json::Value> {
    let mut prefix_blocks = Vec::new();
    if !context_markdown.trim().is_empty() {
        prefix_blocks.push(json!({
            "type": "resource",
            "resource": {
                "uri": format!("agentdash://story-context/{}", story_id),
                "mimeType": "text/markdown",
                "text": context_markdown,
            }
        }));
    }

    let user_blocks = match (original_prompt, original_prompt_blocks) {
        (Some(prompt), None) => vec![json!({ "type": "text", "text": prompt })],
        (None, Some(blocks)) => blocks,
        (Some(prompt), Some(mut blocks)) => {
            let mut merged = vec![json!({ "type": "text", "text": prompt })];
            merged.append(&mut blocks);
            merged
        }
        (None, None) => Vec::new(),
    };

    prefix_blocks.extend(user_blocks);
    prefix_blocks
}

fn clean_text(input: Option<&str>) -> Option<&str> {
    input.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn trim_or_dash(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() { "-" } else { trimmed }
}
