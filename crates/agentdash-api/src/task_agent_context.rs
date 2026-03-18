use std::collections::{BTreeSet, HashMap};
use std::path::{Component, Path};
use std::sync::Arc;

use agent_client_protocol::McpServer;
use agentdash_domain::context_source::{ContextSlot, ContextSourceKind, ContextSourceRef};
use agentdash_domain::{project::Project, story::Story, task::Task, workspace::Workspace};
use agentdash_executor::ExecutionAddressSpace;
use agentdash_injection::{
    ContextComposer, ContextFragment, MergeStrategy, ResolveSourcesOutput, ResolveSourcesRequest,
    resolve_declared_sources,
};
use agentdash_mcp::injection::McpInjectionConfig;
use agentdash_relay::FileEntryRelay;
use serde_json::{Value, json};

use crate::address_space_access::{ListOptions, ResourceRef};
use crate::app_state::AppState;
use crate::session_plan::{
    SessionPlanInput, SessionPlanOwnerKind, SessionPlanPhase, build_session_plan_fragments,
    resolve_effective_session_composition,
};

// ─── 公共抽象：可扩展的上下文构建框架 ───────────────────────────

/// Contributor 的结构化产出 — 同时包含上下文片段和 ACP MCP Server 声明
pub struct Contribution {
    pub context_fragments: Vec<ContextFragment>,
    /// ACP 协议 McpServer 列表，将作为 per-session 工具注入
    pub mcp_servers: Vec<McpServer>,
}

impl Contribution {
    pub fn fragments_only(fragments: Vec<ContextFragment>) -> Self {
        Self {
            context_fragments: fragments,
            mcp_servers: vec![],
        }
    }
}

/// 上下文贡献者 — 所有上下文来源实现此 trait
///
/// 通过 Contributor 模式，新的上下文来源只需实现此 trait 并注册到构建流程，
/// 无需修改核心构建逻辑。
pub trait ContextContributor: Send + Sync {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution;
}

/// 贡献者输入 — 传递给每个 Contributor 的共享上下文
pub struct ContributorInput<'a> {
    pub task: &'a Task,
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub phase: TaskExecutionPhase,
    pub override_prompt: Option<&'a str>,
    pub additional_prompt: Option<&'a str>,
}

// ─── 执行阶段与构建结果 ─────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskExecutionPhase {
    Start,
    Continue,
}

pub struct BuiltTaskAgentContext {
    pub prompt_blocks: Vec<Value>,
    pub working_dir: Option<String>,
    pub source_summary: Vec<String>,
    /// ACP 协议 McpServer 列表 — 由 Connector 通过 `session/new` 传递给 Agent
    pub mcp_servers: Vec<McpServer>,
}

// ─── 内置 Contributor 实现 ──────────────────────────────────

/// Task/Story/Project/Workspace 核心上下文
struct CoreContextContributor;

impl ContextContributor for CoreContextContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        let mut fragments = Vec::new();

        fragments.push(ContextFragment {
            slot: "task",
            label: "task_core",
            order: 10,
            strategy: MergeStrategy::Append,
            content: format!(
                "## Task\n- id: {}\n- title: {}\n- description: {}\n- status: {:?}",
                input.task.id,
                trim_or_dash(&input.task.title),
                trim_or_dash(&input.task.description),
                input.task.status
            ),
        });

        fragments.push(ContextFragment {
            slot: "story",
            label: "story_core",
            order: 20,
            strategy: MergeStrategy::Append,
            content: format!(
                "## Story\n- id: {}\n- title: {}\n- description: {}",
                input.story.id,
                trim_or_dash(&input.story.title),
                trim_or_dash(&input.story.description),
            ),
        });

        if let Some(prd) = clean_text(input.story.context.prd_doc.as_deref()) {
            fragments.push(ContextFragment {
                slot: "story_context",
                label: "story_prd",
                order: 30,
                strategy: MergeStrategy::Append,
                content: format!("## Story PRD\n{prd}"),
            });
        }

        if !input.story.context.spec_refs.is_empty() {
            let refs = input
                .story
                .context
                .spec_refs
                .iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n");
            fragments.push(ContextFragment {
                slot: "story_context",
                label: "story_spec_refs",
                order: 31,
                strategy: MergeStrategy::Append,
                content: format!("## Spec Refs\n{refs}"),
            });
        }

        if !input.story.context.resource_list.is_empty() {
            let resources = input
                .story
                .context
                .resource_list
                .iter()
                .map(|res| format!("- [{}] {} ({})", res.resource_type, res.name, res.uri))
                .collect::<Vec<_>>()
                .join("\n");
            fragments.push(ContextFragment {
                slot: "story_context",
                label: "story_resources",
                order: 32,
                strategy: MergeStrategy::Append,
                content: format!("## Resources\n{resources}"),
            });
        }

        fragments.push(ContextFragment {
            slot: "project",
            label: "project_config",
            order: 40,
            strategy: MergeStrategy::Append,
            content: format!(
                "## Project\n- id: {}\n- name: {}\n- default_agent_type: {}",
                input.project.id,
                trim_or_dash(&input.project.name),
                input
                    .project
                    .config
                    .default_agent_type
                    .as_deref()
                    .unwrap_or("-")
            ),
        });

        if let Some(workspace) = input.workspace {
            fragments.push(ContextFragment {
                slot: "workspace",
                label: "workspace_context",
                order: 50,
                strategy: MergeStrategy::Append,
                content: format!(
                    "## Workspace\n- id: {}\n- backend_id: {}\n- name: {}\n- working_dir: .\n- type: {:?}\n- status: {:?}",
                    workspace.id,
                    trim_or_dash(&workspace.backend_id),
                    trim_or_dash(&workspace.name),
                    workspace.workspace_type,
                    workspace.status,
                ),
            });
        }

        Contribution::fragments_only(fragments)
    }
}

/// Agent 绑定上下文（initial_context）
struct BindingContextContributor;

impl ContextContributor for BindingContextContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        let mut fragments = Vec::new();

        if let Some(initial_context) =
            clean_text(input.task.agent_binding.initial_context.as_deref())
        {
            fragments.push(ContextFragment {
                slot: "initial_context",
                label: "binding_initial_context",
                order: 80,
                strategy: MergeStrategy::Append,
                content: format!("## Initial Context\n{initial_context}"),
            });
        }

        Contribution::fragments_only(fragments)
    }
}

/// 声明式来源注入 Contributor（Story + Task）
struct DeclaredSourcesContributor;

impl ContextContributor for DeclaredSourcesContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        let mut sources = input.story.context.source_refs.clone();
        sources.extend(input.task.agent_binding.context_sources.clone());

        if sources.is_empty() {
            return Contribution::fragments_only(Vec::new());
        }

        let mut fragments = Vec::new();
        let resolvable_sources = sources
            .iter()
            .filter(|source| {
                !matches!(
                    source.kind,
                    ContextSourceKind::File | ContextSourceKind::ProjectSnapshot
                )
            })
            .cloned()
            .collect::<Vec<_>>();

        match resolve_declared_sources(ResolveSourcesRequest {
            sources: &resolvable_sources,
            workspace_root: None,
            base_order: 82,
        }) {
            Ok(result) => {
                fragments.extend(result.fragments);
                if !result.warnings.is_empty() {
                    fragments.push(ContextFragment {
                        slot: "references",
                        label: "declared_source_warnings",
                        order: 89,
                        strategy: MergeStrategy::Append,
                        content: format!(
                            "## Injection Notes\n{}",
                            result
                                .warnings
                                .iter()
                                .map(|item| format!("- {item}"))
                                .collect::<Vec<_>>()
                                .join("\n")
                        ),
                    });
                }
                Contribution::fragments_only(fragments)
            }
            Err(err) => {
                fragments.push(ContextFragment {
                    slot: "references",
                    label: "declared_source_error",
                    order: 89,
                    strategy: MergeStrategy::Append,
                    content: format!("## Injection Error\n- 声明式上下文来源解析失败：{}", err),
                });
                Contribution::fragments_only(fragments)
            }
        }
    }
}

pub struct StaticFragmentsContributor {
    fragments: Vec<ContextFragment>,
}

impl StaticFragmentsContributor {
    pub fn new(fragments: Vec<ContextFragment>) -> Self {
        Self { fragments }
    }
}

impl ContextContributor for StaticFragmentsContributor {
    fn contribute(&self, _input: &ContributorInput<'_>) -> Contribution {
        Contribution::fragments_only(self.fragments.clone())
    }
}

/// 指令模板 Contributor
struct InstructionContributor;

impl ContextContributor for InstructionContributor {
    fn contribute(&self, input: &ContributorInput<'_>) -> Contribution {
        let mut fragments = Vec::new();

        let workspace_path = input
            .workspace
            .map(|_| ".".to_string())
            .unwrap_or_else(|| ".".to_string());

        let template = resolve_instruction_template(input);
        let rendered = render_template(
            &template,
            &template_vars(
                &input.task.title,
                &input.task.description,
                &input.story.title,
                &workspace_path,
            ),
        );
        fragments.push(ContextFragment {
            slot: "instruction",
            label: "binding_template",
            order: 90,
            strategy: MergeStrategy::Override,
            content: format!("## Instruction\n{rendered}"),
        });

        if input.phase == TaskExecutionPhase::Continue {
            if let Some(additional) = clean_text(input.additional_prompt) {
                fragments.push(ContextFragment {
                    slot: "instruction",
                    label: "user_additional_prompt",
                    order: 100,
                    strategy: MergeStrategy::Append,
                    content: format!("## Additional Prompt\n{additional}"),
                });
            }
        }

        Contribution::fragments_only(fragments)
    }
}

/// MCP 能力注入 Contributor — 通过 ACP 协议类型声明 MCP Server，并在上下文中附加简要说明
pub struct McpContextContributor {
    pub config: McpInjectionConfig,
}

impl McpContextContributor {
    pub fn new(config: McpInjectionConfig) -> Self {
        Self { config }
    }
}

impl ContextContributor for McpContextContributor {
    fn contribute(&self, _input: &ContributorInput<'_>) -> Contribution {
        let label: &'static str = match self.config.scope {
            agentdash_mcp::scope::ToolScope::Relay => "mcp_relay_tools",
            agentdash_mcp::scope::ToolScope::Story => "mcp_story_tools",
            agentdash_mcp::scope::ToolScope::Task => "mcp_task_tools",
        };

        Contribution {
            context_fragments: vec![ContextFragment {
                slot: "mcp_config",
                label,
                order: 85,
                strategy: MergeStrategy::Append,
                content: self.config.to_context_content(),
            }],
            mcp_servers: vec![self.config.to_acp_mcp_server()],
        }
    }
}

// ─── 构建入口 ────────────────────────────────────────────────

const DEFAULT_START_TEMPLATE: &str = r#"你是该任务的执行 Agent。
请结合任务上下文完成实现，并在完成后给出关键变更与验证结果。

任务标题：{{task_title}}
任务描述：{{task_description}}
Story：{{story_title}}
工作目录：{{workspace_path}}"#;

const DEFAULT_CONTINUE_TEMPLATE: &str = r#"请在当前会话上下文基础上继续推进该任务。
优先完成未完成项，并说明下一步建议。

任务标题：{{task_title}}
任务描述：{{task_description}}
Story：{{story_title}}
工作目录：{{workspace_path}}"#;

/// 上下文贡献者注册表 — 持有"常驻"贡献者，避免在构建函数中硬编码
///
/// 存放在 `AppState` 中，所有 Task 构建共享同一注册表实例。
/// 动态/per-request 贡献者（如 MCP 注入）通过 `extra_contributors` 传入。
pub struct ContextContributorRegistry {
    contributors: Vec<Box<dyn ContextContributor>>,
}

impl ContextContributorRegistry {
    /// 创建包含内置贡献者的注册表
    pub fn with_builtins() -> Self {
        Self {
            contributors: vec![
                Box::new(CoreContextContributor),
                Box::new(BindingContextContributor),
                Box::new(DeclaredSourcesContributor),
                Box::new(InstructionContributor),
            ],
        }
    }

    /// 注册新的常驻贡献者
    pub fn register(&mut self, contributor: Box<dyn ContextContributor>) {
        self.contributors.push(contributor);
    }

    /// 常驻贡献者数量
    pub fn len(&self) -> usize {
        self.contributors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.contributors.is_empty()
    }
}

pub struct TaskAgentBuildInput<'a> {
    pub task: &'a Task,
    pub story: &'a Story,
    pub project: &'a Project,
    pub workspace: Option<&'a Workspace>,
    pub address_space: Option<&'a ExecutionAddressSpace>,
    pub effective_agent_type: Option<&'a str>,
    pub phase: TaskExecutionPhase,
    pub override_prompt: Option<&'a str>,
    pub additional_prompt: Option<&'a str>,
    /// per-request 动态贡献者（如 MCP 注入，每次构建内容不同）
    pub extra_contributors: Vec<Box<dyn ContextContributor>>,
}

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
    let mut instruction_composer = ContextComposer::default();
    let mut mcp_servers: Vec<McpServer> = Vec::new();

    let all_contributors = registry
        .contributors
        .iter()
        .map(|c| c.as_ref())
        .chain(input.extra_contributors.iter().map(|c| c.as_ref()));

    for contributor in all_contributors {
        let contribution = contributor.contribute(&contributor_input);

        mcp_servers.extend(contribution.mcp_servers);

        for fragment in contribution.context_fragments {
            match fragment.slot {
                "instruction" | "instruction_append" => {
                    instruction_composer.push_fragment(fragment)
                }
                _ => context_composer.push_fragment(fragment),
            }
        }
    }

    let effective_session_composition =
        resolve_effective_session_composition(input.project, Some(input.story));
    let session_plan = build_session_plan_fragments(SessionPlanInput {
        owner_kind: SessionPlanOwnerKind::TaskExecution,
        phase: match input.phase {
            TaskExecutionPhase::Start => SessionPlanPhase::TaskStart,
            TaskExecutionPhase::Continue => SessionPlanPhase::TaskContinue,
        },
        address_space: input.address_space,
        mcp_servers: &mcp_servers,
        session_composition: Some(&effective_session_composition),
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

    let (context_prompt, mut source_summary) = context_composer.compose();
    let (instruction_prompt, instruction_sources) = instruction_composer.compose();
    source_summary.extend(instruction_sources);

    let combined_prompt = [context_prompt.as_str(), instruction_prompt.as_str()]
        .iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("\n\n");

    if combined_prompt.trim().is_empty() {
        return Err("构建执行上下文失败：最终 prompt 为空".to_string());
    }

    let mut prompt_blocks = Vec::new();
    if !context_prompt.trim().is_empty() {
        prompt_blocks.push(build_task_context_resource_block(
            input.task.id.to_string(),
            input.phase,
            context_prompt,
        ));
    }
    if !instruction_prompt.trim().is_empty() {
        prompt_blocks.push(json!({
            "type": "text",
            "text": instruction_prompt,
        }));
    }

    Ok(BuiltTaskAgentContext {
        prompt_blocks,
        working_dir,
        source_summary,
        mcp_servers,
    })
}

// ─── 辅助函数 ────────────────────────────────────────────────

fn build_task_context_resource_block(
    task_id: String,
    phase: TaskExecutionPhase,
    markdown: String,
) -> Value {
    let phase_label = match phase {
        TaskExecutionPhase::Start => "start",
        TaskExecutionPhase::Continue => "continue",
    };

    json!({
        "type": "resource",
        "resource": {
            "uri": format!("agentdash://task-context/{task_id}?phase={phase_label}"),
            "mimeType": "text/markdown",
            "text": markdown,
        }
    })
}

pub async fn resolve_workspace_declared_sources(
    state: &Arc<AppState>,
    sources: &[ContextSourceRef],
    workspace: Option<&Workspace>,
    base_order: i32,
) -> Result<ResolveSourcesOutput, String> {
    let indexed_sources = sorted_sources(sources)
        .into_iter()
        .filter(|source| {
            matches!(
                source.kind,
                ContextSourceKind::File | ContextSourceKind::ProjectSnapshot
            )
        })
        .collect::<Vec<_>>();

    if indexed_sources.is_empty() {
        return Ok(ResolveSourcesOutput {
            fragments: Vec::new(),
            warnings: Vec::new(),
        });
    }

    let Some(workspace) = workspace else {
        return resolve_workspace_source_unavailable(
            &indexed_sources,
            "声明式来源依赖 Workspace，但当前上下文未绑定可用 Workspace",
        );
    };

    let backend_id = match normalize_workspace_backend_id(workspace) {
        Ok(backend_id) => backend_id,
        Err(err) => return resolve_workspace_source_unavailable(&indexed_sources, &err),
    };
    if !state.backend_registry.is_online(backend_id).await {
        return resolve_workspace_source_unavailable(
            &indexed_sources,
            &format!("Workspace 所属 Backend 当前不在线: {backend_id}"),
        );
    }

    let mut fragments = Vec::new();
    let mut warnings = Vec::new();

    for (position, source) in indexed_sources.into_iter().enumerate() {
        let order = base_order + position as i32;
        let resolved = match source.kind {
            ContextSourceKind::File => {
                resolve_workspace_file_source(state, workspace, source, order).await
            }
            ContextSourceKind::ProjectSnapshot => {
                resolve_workspace_snapshot_source(state, workspace, source, order).await
            }
            _ => continue,
        };

        match resolved {
            Ok(fragment) => fragments.push(fragment),
            Err(err) if source.required => return Err(err),
            Err(err) => warnings.push(format!(
                "source `{}` 已跳过: {err}",
                display_source_label(source)
            )),
        }
    }

    Ok(ResolveSourcesOutput {
        fragments,
        warnings,
    })
}

fn resolve_workspace_source_unavailable(
    sources: &[&ContextSourceRef],
    message: &str,
) -> Result<ResolveSourcesOutput, String> {
    if sources.iter().any(|source| source.required) {
        return Err(message.to_string());
    }
    Ok(ResolveSourcesOutput {
        fragments: Vec::new(),
        warnings: sources
            .iter()
            .map(|source| {
                format!(
                    "source `{}` 已跳过: {message}",
                    display_source_label(source)
                )
            })
            .collect(),
    })
}

pub fn build_declared_source_warning_fragment(
    label: &'static str,
    order: i32,
    warnings: &[String],
) -> ContextFragment {
    ContextFragment {
        slot: "references",
        label,
        order,
        strategy: MergeStrategy::Append,
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

fn sorted_sources(sources: &[ContextSourceRef]) -> Vec<&ContextSourceRef> {
    let mut indexed_sources = sources.iter().enumerate().collect::<Vec<_>>();
    indexed_sources.sort_by(|(left_index, left), (right_index, right)| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left_index.cmp(right_index))
    });
    indexed_sources
        .into_iter()
        .map(|(_, source)| source)
        .collect()
}

fn normalize_workspace_backend_id(workspace: &Workspace) -> Result<&str, String> {
    let backend_id = workspace.backend_id.trim();
    if backend_id.is_empty() {
        Err("Workspace.backend_id 不能为空".to_string())
    } else {
        Ok(backend_id)
    }
}

async fn resolve_workspace_file_source(
    state: &Arc<AppState>,
    workspace: &Workspace,
    source: &ContextSourceRef,
    order: i32,
) -> Result<ContextFragment, String> {
    let path = normalize_source_locator_path(&source.locator)?;
    let address_space = state
        .address_space_service
        .session_for_workspace(workspace)?;
    let read = state
        .address_space_service
        .read_text(
            &address_space,
            &ResourceRef {
                mount_id: "main".to_string(),
                path: path.clone(),
            },
        )
        .await
        .map_err(|e| format!("工作空间文件读取失败: {e}"))?;

    Ok(ContextFragment {
        slot: fragment_slot(&source.slot),
        label: fragment_label(&source.kind),
        order,
        strategy: MergeStrategy::Append,
        content: render_source_section(
            source,
            truncate_text(
                format_file_like_read_tool(&read.path, &read.content),
                source.max_chars,
            ),
        ),
    })
}

async fn resolve_workspace_snapshot_source(
    state: &Arc<AppState>,
    workspace: &Workspace,
    source: &ContextSourceRef,
    order: i32,
) -> Result<ContextFragment, String> {
    let sub_path = normalize_snapshot_locator(&source.locator)?;
    let address_space = state
        .address_space_service
        .session_for_workspace(workspace)?;
    let listed = state
        .address_space_service
        .list(
            &address_space,
            "main",
            ListOptions {
                path: sub_path.clone().unwrap_or_else(|| ".".to_string()),
                pattern: None,
                recursive: true,
            },
        )
        .await
        .map_err(|e| format!("项目快照读取失败: {e}"))?;

    Ok(ContextFragment {
        slot: fragment_slot(&source.slot),
        label: fragment_label(&source.kind),
        order,
        strategy: MergeStrategy::Append,
        content: render_source_section(
            source,
            build_workspace_snapshot_from_entries(
                &workspace.container_ref,
                sub_path.as_deref(),
                &listed.entries,
                source.max_chars,
            ),
        ),
    })
}

fn normalize_source_locator_path(locator: &str) -> Result<String, String> {
    let trimmed = locator.trim();
    if trimmed.is_empty() {
        return Err("文件来源 locator 不能为空".to_string());
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err("文件来源 locator 不能是绝对路径".to_string());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("文件来源 locator 不能包含 `..`".to_string());
    }

    Ok(trimmed.replace('\\', "/"))
}

fn normalize_snapshot_locator(locator: &str) -> Result<Option<String>, String> {
    let trimmed = locator.trim();
    if trimmed.is_empty() || trimmed == "." {
        return Ok(None);
    }
    normalize_source_locator_path(trimmed).map(Some)
}

fn build_workspace_snapshot_from_entries(
    workspace_root: &str,
    sub_path: Option<&str>,
    files: &[FileEntryRelay],
    max_chars: Option<usize>,
) -> String {
    let mut summary_entries = BTreeSet::new();
    for file in files {
        let rel = file.path.trim_matches('/');
        if rel.is_empty() {
            continue;
        }
        let parts = rel.split('/').collect::<Vec<_>>();
        if parts.len() == 1 {
            summary_entries.insert(parts[0].to_string());
            continue;
        }

        summary_entries.insert(format!("{}/", parts[0]));
        if parts.len() == 2 {
            summary_entries.insert(rel.to_string());
            continue;
        }
        summary_entries.insert(format!("{}/{}/", parts[0], parts[1]));
    }

    let entries = summary_entries
        .into_iter()
        .take(48)
        .map(|entry| format!("- {entry}"))
        .collect::<Vec<_>>()
        .join("\n");

    let tech_stack = detect_tech_stack_from_entries(files);
    let root_display = sub_path
        .map(|path| format!("{}/{}", workspace_root.trim_end_matches('/'), path))
        .unwrap_or_else(|| workspace_root.to_string())
        .replace('\\', "/");

    truncate_text(
        format!(
            "## 项目快照\n- root: {}\n- tech_stack: {}\n\n## 目录摘要\n{}",
            root_display,
            tech_stack.join(", "),
            entries
        ),
        max_chars,
    )
}

fn detect_tech_stack_from_entries(files: &[FileEntryRelay]) -> Vec<&'static str> {
    let paths = files
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    let mut stack = Vec::new();
    if paths.iter().any(|path| *path == "Cargo.toml") {
        stack.push("Rust");
    }
    if paths.iter().any(|path| *path == "package.json") {
        stack.push("Node.js");
    }
    if paths.iter().any(|path| *path == "pnpm-lock.yaml") {
        stack.push("pnpm");
    }
    if paths
        .iter()
        .any(|path| *path == "playwright.config.ts" || *path == "playwright.config.js")
    {
        stack.push("Playwright");
    }
    if stack.is_empty() {
        stack.push("unknown");
    }
    stack
}

fn format_file_like_read_tool(path: &str, content: &str) -> String {
    let numbered = content
        .lines()
        .enumerate()
        .map(|(index, line)| format!("{:>4} | {}", index + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    if numbered.is_empty() {
        format!("文件: {}\n   1 | ", path.replace('\\', "/"))
    } else {
        format!("文件: {}\n{}", path.replace('\\', "/"), numbered)
    }
}

fn truncate_text(content: String, max_chars: Option<usize>) -> String {
    const DEFAULT_TRUNCATE_CHARS: usize = 12_000;
    let max = max_chars.unwrap_or(DEFAULT_TRUNCATE_CHARS);
    if content.chars().count() <= max {
        return content;
    }

    let truncated = content.chars().take(max).collect::<String>();
    format!("{truncated}\n\n> 内容已截断")
}

fn render_source_section(source: &ContextSourceRef, content: String) -> String {
    let title = display_source_label(source);
    format!("## 来源: {title}\n{content}")
}

fn display_source_label(source: &ContextSourceRef) -> &str {
    source.label.as_deref().unwrap_or(source.locator.as_str())
}

fn fragment_label(kind: &ContextSourceKind) -> &'static str {
    match kind {
        ContextSourceKind::ManualText => "declared_manual_text",
        ContextSourceKind::File => "declared_file_source",
        ContextSourceKind::ProjectSnapshot => "declared_project_snapshot",
        ContextSourceKind::HttpFetch => "declared_http_fetch",
        ContextSourceKind::McpResource => "declared_mcp_resource",
        ContextSourceKind::EntityRef => "declared_entity_ref",
    }
}

fn fragment_slot(slot: &ContextSlot) -> &'static str {
    match slot {
        ContextSlot::Requirements => "requirements",
        ContextSlot::Constraints => "constraints",
        ContextSlot::Codebase => "codebase",
        ContextSlot::References => "references",
        ContextSlot::InstructionAppend => "instruction_append",
    }
}

fn resolve_instruction_template(input: &ContributorInput<'_>) -> String {
    match input.phase {
        TaskExecutionPhase::Start => {
            if let Some(override_prompt) = clean_text(input.override_prompt) {
                return override_prompt.to_string();
            }
            input
                .task
                .agent_binding
                .prompt_template
                .clone()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_START_TEMPLATE.to_string())
        }
        TaskExecutionPhase::Continue => input
            .task
            .agent_binding
            .prompt_template
            .clone()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CONTINUE_TEMPLATE.to_string()),
    }
}

fn template_vars(
    task_title: &str,
    task_description: &str,
    story_title: &str,
    workspace_path: &str,
) -> HashMap<&'static str, String> {
    let mut vars = HashMap::new();
    vars.insert("task_title", trim_or_dash(task_title).to_string());
    vars.insert(
        "task_description",
        trim_or_dash(task_description).to_string(),
    );
    vars.insert("story_title", trim_or_dash(story_title).to_string());
    vars.insert("workspace_path", trim_or_dash(workspace_path).to_string());
    vars
}

fn render_template(template: &str, vars: &HashMap<&'static str, String>) -> String {
    let mut rendered = template.to_string();
    for (key, value) in vars {
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
    }
    rendered
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::context_source::{
        ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
    };
    use agentdash_domain::workspace::{Workspace, WorkspaceStatus, WorkspaceType};

    #[test]
    fn compose_keeps_initial_context_when_instruction_slot_is_override() {
        let mut composer = ContextComposer::default();
        composer.push(
            "initial_context",
            "binding_initial_context",
            80,
            MergeStrategy::Append,
            "## Initial Context\ncontext from binding",
        );
        composer.push(
            "instruction",
            "binding_template",
            90,
            MergeStrategy::Override,
            "## Instruction\nrun task",
        );

        let (prompt, _) = composer.compose();
        assert!(prompt.contains("context from binding"));
        assert!(prompt.contains("## Instruction"));
    }

    struct TestContributor {
        slot: &'static str,
        label: &'static str,
        order: i32,
        content: String,
    }

    impl ContextContributor for TestContributor {
        fn contribute(&self, _input: &ContributorInput<'_>) -> Contribution {
            Contribution::fragments_only(vec![ContextFragment {
                slot: self.slot,
                label: self.label,
                order: self.order,
                strategy: MergeStrategy::Append,
                content: self.content.clone(),
            }])
        }
    }

    #[test]
    fn extra_contributor_fragments_are_included() {
        let task = Task::new(uuid::Uuid::new_v4(), "test task".into(), "desc".into());
        let story = Story::new(
            uuid::Uuid::new_v4(),
            "test-backend".into(),
            "test story".into(),
            "story desc".into(),
        );
        let project = Project::new("test project".into(), "desc".into(), "test-backend".into());

        let mcp_contributor = TestContributor {
            slot: "mcp_config",
            label: "mcp_task_tools",
            order: 85,
            content: "## MCP: agentdash-task-tools\n- url: http://localhost:3001/mcp/task/abc\n- scope: task\n可通过此 MCP Server 更新 Task 状态".to_string(),
        };

        let registry = ContextContributorRegistry::with_builtins();
        let result = build_task_agent_context(
            TaskAgentBuildInput {
                task: &task,
                story: &story,
                project: &project,
                workspace: None,
                address_space: None,
                effective_agent_type: None,
                phase: TaskExecutionPhase::Start,
                override_prompt: None,
                additional_prompt: None,
                extra_contributors: vec![Box::new(mcp_contributor)],
            },
            &registry,
        )
        .expect("should build context");

        assert!(
            result
                .source_summary
                .iter()
                .any(|s| s.contains("mcp_task_tools")),
            "source_summary 应包含 MCP 贡献者标签"
        );
        assert!(
            result.mcp_servers.is_empty(),
            "TestContributor 不产出 McpServer，mcp_servers 应为空"
        );
    }

    #[test]
    fn mcp_context_contributor_produces_acp_mcp_server_and_fragment() {
        let config = McpInjectionConfig::for_task(
            "http://localhost:3001",
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        );
        let contributor = McpContextContributor::new(config);

        let task = Task::new(uuid::Uuid::new_v4(), "t".into(), "d".into());
        let story = Story::new(uuid::Uuid::new_v4(), "b".into(), "s".into(), "d".into());
        let project = Project::new("p".into(), "d".into(), "b".into());

        let input = ContributorInput {
            task: &task,
            story: &story,
            project: &project,
            workspace: None,
            phase: TaskExecutionPhase::Start,
            override_prompt: None,
            additional_prompt: None,
        };

        let contribution = contributor.contribute(&input);

        assert_eq!(contribution.context_fragments.len(), 1);
        assert_eq!(contribution.context_fragments[0].slot, "mcp_config");
        assert_eq!(contribution.context_fragments[0].label, "mcp_task_tools");
        assert!(
            contribution.context_fragments[0]
                .content
                .contains("## MCP: ")
        );

        assert_eq!(
            contribution.mcp_servers.len(),
            1,
            "应产出 1 个 ACP McpServer"
        );
        let server_json = serde_json::to_value(&contribution.mcp_servers[0]).unwrap();
        assert_eq!(server_json["type"], "http");
        assert!(server_json["url"].as_str().unwrap().contains("/mcp/task/"));
    }

    #[test]
    fn declared_sources_are_included_in_context_prompt() {
        let mut task = Task::new(uuid::Uuid::new_v4(), "task".into(), "desc".into());
        task.agent_binding.context_sources = vec![ContextSourceRef {
            kind: ContextSourceKind::ManualText,
            locator: "请严格遵守接口约束".to_string(),
            label: Some("task constraint".to_string()),
            slot: ContextSlot::Constraints,
            priority: 50,
            required: true,
            max_chars: None,
            delivery: ContextDelivery::Resource,
        }];

        let mut story = Story::new(
            uuid::Uuid::new_v4(),
            "backend".into(),
            "story".into(),
            "story desc".into(),
        );
        story.context.source_refs = vec![ContextSourceRef {
            kind: ContextSourceKind::ManualText,
            locator: "这是 Story 级需求摘要".to_string(),
            label: Some("story requirements".to_string()),
            slot: ContextSlot::Requirements,
            priority: 100,
            required: true,
            max_chars: None,
            delivery: ContextDelivery::Resource,
        }];

        let project = Project::new("proj".into(), "desc".into(), "backend".into());
        let workspace = Workspace {
            id: uuid::Uuid::new_v4(),
            project_id: project.id,
            backend_id: "backend".into(),
            name: "ws".into(),
            container_ref: ".".into(),
            workspace_type: WorkspaceType::Static,
            status: WorkspaceStatus::Ready,
            git_config: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let registry = ContextContributorRegistry::with_builtins();
        let result = build_task_agent_context(
            TaskAgentBuildInput {
                task: &task,
                story: &story,
                project: &project,
                workspace: Some(&workspace),
                address_space: None,
                effective_agent_type: None,
                phase: TaskExecutionPhase::Start,
                override_prompt: None,
                additional_prompt: None,
                extra_contributors: vec![],
            },
            &registry,
        )
        .expect("should build context");

        let context_block = result.prompt_blocks[0]["resource"]["text"]
            .as_str()
            .expect("resource block text");
        assert!(context_block.contains("这是 Story 级需求摘要"));
        assert!(context_block.contains("请严格遵守接口约束"));
    }

    #[test]
    fn address_space_summary_is_included_in_task_context() {
        let task = Task::new(uuid::Uuid::new_v4(), "task".into(), "desc".into());
        let story = Story::new(
            uuid::Uuid::new_v4(),
            "backend".into(),
            "story".into(),
            "story desc".into(),
        );
        let project = Project::new("proj".into(), "desc".into(), "backend".into());
        let registry = ContextContributorRegistry::with_builtins();
        let address_space = ExecutionAddressSpace {
            mounts: vec![agentdash_executor::ExecutionMount {
                id: "spec".to_string(),
                provider: "inline_fs".to_string(),
                backend_id: String::new(),
                root_ref: "context://inline/spec".to_string(),
                capabilities: vec![
                    agentdash_executor::ExecutionMountCapability::Read,
                    agentdash_executor::ExecutionMountCapability::List,
                ],
                default_write: false,
                display_name: "规范容器".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: Some("spec".to_string()),
        };

        let result = build_task_agent_context(
            TaskAgentBuildInput {
                task: &task,
                story: &story,
                project: &project,
                workspace: None,
                address_space: Some(&address_space),
                effective_agent_type: Some("PI_AGENT"),
                phase: TaskExecutionPhase::Start,
                override_prompt: None,
                additional_prompt: None,
                extra_contributors: vec![],
            },
            &registry,
        )
        .expect("should build context");

        let context_block = result.prompt_blocks[0]["resource"]["text"]
            .as_str()
            .expect("resource block text");
        assert!(context_block.contains("## Address Space"));
        assert!(context_block.contains("`spec`: 规范容器"));
        assert!(context_block.contains("## Tool Visibility"));
        assert!(context_block.contains("`mounts_list`"));
    }

    #[test]
    fn snapshot_builder_keeps_directory_shape() {
        let snapshot = build_workspace_snapshot_from_entries(
            "/workspace/demo",
            None,
            &[
                FileEntryRelay {
                    path: "Cargo.toml".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                },
                FileEntryRelay {
                    path: "src/main.rs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                },
                FileEntryRelay {
                    path: "src/lib.rs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                },
                FileEntryRelay {
                    path: "tests/e2e/story.rs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                },
            ],
            None,
        );

        assert!(snapshot.contains("Rust"));
        assert!(snapshot.contains("- src/"));
        assert!(snapshot.contains("- src/main.rs"));
        assert!(snapshot.contains("- tests/e2e/"));
    }

    #[test]
    fn file_locator_rejects_parent_dir() {
        let err = normalize_source_locator_path("../secret.txt").expect_err("应拒绝父级目录");
        assert!(err.contains(".."));
    }
}
