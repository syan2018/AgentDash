use agentdash_agent_protocol::{
    ContextDeliveryChannel, ContextDeliveryStatus, ContextFrameKind, ContextFrameSection,
    ContextFrameSource, ContextMessageRole, ProjectGuidelineEntry, RuntimeContextFragmentEntry,
    RuntimeMemoryDiagnosticEntry, RuntimeMemoryInventoryMode, RuntimeMemorySourceEntry,
};
use agentdash_agent_runtime_contract::{ContextBlock, InstructionChannel, SemanticStrength};
use serde::{Deserialize, Serialize};

use super::ContextFrameFacts;

const WINDOWS_POWERSHELL_TEXT_OUTPUT_NOTE: &str = "Windows shell: the real OS shell is PowerShell. Compose commands with PowerShell syntax, not bash-only operators like && or || true. Some commands such as Get-Location and Get-ChildItem return objects; for non-interactive tools or scripts that need stable text, explicitly select string fields and emit strings, for example Write-Output (Get-Location).Path or Get-ChildItem | ForEach-Object { Write-Output $_.FullName }. Prefer dedicated VFS file tools for inspect/read/search. Interactive terminals still rely on real PTY/stdout bytes.";

const ASSIGNMENT_CONTEXT_SLOTS: &[&str] = &[
    "task",
    "story",
    "project",
    "workspace",
    "initial_context",
    "persona",
    "required_context",
    "workflow",
    "workflow_context",
    "story_context",
    "declared_source",
    "static_fragment",
    "requirements",
    "constraints",
    "constraint",
    "codebase",
    "references",
    "instruction",
    "instruction_append",
];

/// Immutable Application facts used to build the bootstrap presentation of one AgentFrame.
///
/// The complete assignment fragments live here rather than in `FrameContextBundleSummary`, so a
/// compiled surface can be replayed without consulting a mutable product read model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BootstrapContextFacts {
    pub include_startup_context: bool,
    pub identity: IdentityContextFacts,
    pub user: Option<UserContextFacts>,
    pub environment: EnvironmentContextFacts,
    pub guidelines: GuidelinesContextFacts,
    pub memory: MemoryContextFacts,
    pub assignment: AssignmentContextFacts,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityContextFacts {
    pub base_system_prompt: String,
    pub agent_identity_markdown: Option<String>,
    pub agent_system_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserContextFacts {
    pub user_id: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub groups: Vec<String>,
    pub provider: Option<String>,
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentContextFacts {
    pub date_utc: String,
    pub platform: String,
    pub arch: String,
    pub model_id: Option<String>,
    pub executor: String,
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuidelinesContextFacts {
    pub user_preferences: Vec<String>,
    pub discovered_guidelines: Vec<DiscoveredGuidelineFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredGuidelineFacts {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryContextFacts {
    pub sources: Vec<MemorySourceFacts>,
    pub diagnostics: Vec<MemoryDiagnosticFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySourceFacts {
    pub provider_key: String,
    pub source_key: String,
    pub display_name: String,
    pub source_uri: String,
    pub index_uri: String,
    pub mount_id: String,
    pub scope: String,
    pub capabilities: Vec<String>,
    pub index_status: String,
    pub trust_level: String,
    pub revision: String,
    pub summary: Option<String>,
    pub bounded_index_content: Option<String>,
    pub context_usage_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDiagnosticFacts {
    pub provider_key: String,
    pub code: String,
    pub message: String,
    pub source_key: Option<String>,
    pub uri: Option<String>,
    pub context_usage_kind: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentContextFacts {
    pub phase_tag: Option<String>,
    pub apply_mode: Option<String>,
    pub fragments: Vec<AssignmentFragmentFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentFragmentFacts {
    pub slot: String,
    pub label: String,
    pub order: i32,
    pub runtime_agent_scope: bool,
    pub source: String,
    pub content: String,
    pub context_usage_kind: Option<String>,
}

/// Bootstrap facts are split because main's durable insertion order places the initial
/// capability frame before assignment, while stable frames follow both of them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BootstrapContextProjectionFacts {
    pub stable_frames: Vec<ContextFrameFacts>,
    pub assignment_frame: Option<ContextFrameFacts>,
}

/// Driver-facing projection of the same immutable bootstrap facts used by presentation.
///
/// The model projection intentionally carries typed instruction/context entries. It never reads
/// a projected `ContextFrame`, so presentation serialization cannot become a model-input API.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BootstrapModelContextProjection {
    pub instructions: Vec<BootstrapModelInstruction>,
    pub context: Vec<BootstrapModelContextEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapModelInstruction {
    pub key: String,
    pub priority: i32,
    pub channel: InstructionChannel,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootstrapModelContextEntry {
    pub key: String,
    pub priority: i32,
    pub blocks: Vec<ContextBlock>,
    pub minimum_strength: SemanticStrength,
}

impl BootstrapContextFacts {
    #[must_use]
    pub fn project(&self) -> BootstrapContextProjectionFacts {
        if !self.include_startup_context {
            return BootstrapContextProjectionFacts::default();
        }

        let stable_frames = [
            build_identity_context_frame(&self.identity),
            self.user.as_ref().and_then(build_user_context_frame),
            build_environment_context_frame(&self.environment),
            build_guidelines_context_frame(&self.guidelines),
            build_memory_context_frame(&self.memory),
        ]
        .into_iter()
        .flatten()
        .collect();

        BootstrapContextProjectionFacts {
            stable_frames,
            assignment_frame: build_assignment_context_frame(&self.assignment),
        }
    }

    /// Materialize the connector/model context without depending on presentation frame objects.
    #[must_use]
    pub fn project_model_context(&self) -> BootstrapModelContextProjection {
        if !self.include_startup_context {
            return BootstrapModelContextProjection::default();
        }

        let mut instructions = Vec::new();
        for (key, priority, content) in [
            ("identity", 600, render_identity_context(&self.identity)),
            (
                "user_context",
                500,
                self.user.as_ref().and_then(render_user_context),
            ),
            (
                "environment",
                400,
                render_environment_context(&self.environment),
            ),
            (
                "system_guidelines",
                300,
                render_guidelines_context(&self.guidelines),
            ),
        ] {
            if let Some(content) = content {
                instructions.push(BootstrapModelInstruction {
                    key: format!("bootstrap:{key}"),
                    priority,
                    channel: InstructionChannel::System,
                    content,
                });
            }
        }

        let mut context = Vec::new();
        let assignment_fragments = assignment_runtime_fragments(&self.assignment);
        if !assignment_fragments.is_empty() {
            context.push(BootstrapModelContextEntry {
                key: "bootstrap:assignment_context".to_string(),
                priority: 200,
                blocks: vec![ContextBlock::Instruction {
                    text: render_assignment_context(&assignment_fragments),
                }],
                // Context-capable drivers retain exact fidelity; opaque Codex-compatible drivers
                // may still carry the same typed block through additional context.
                minimum_strength: SemanticStrength::ObservedOnly,
            });
        }
        if !self.memory.sources.is_empty() || !self.memory.diagnostics.is_empty() {
            context.push(BootstrapModelContextEntry {
                key: "bootstrap:memory_context".to_string(),
                priority: 100,
                blocks: vec![ContextBlock::Instruction {
                    text: render_memory_context(&self.memory),
                }],
                minimum_strength: SemanticStrength::ObservedOnly,
            });
        }

        BootstrapModelContextProjection {
            instructions,
            context,
        }
    }
}

#[must_use]
pub fn build_identity_context_frame(input: &IdentityContextFacts) -> Option<ContextFrameFacts> {
    let base_prompt = input.base_system_prompt.trim();
    let agent_identity = input
        .agent_identity_markdown
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let agent_prompt = input
        .agent_system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if base_prompt.is_empty() && agent_identity.is_none() && agent_prompt.is_none() {
        return None;
    }

    let mut fragments = Vec::new();
    if !base_prompt.is_empty() {
        fragments.push(RuntimeContextFragmentEntry {
            slot: "identity".to_string(),
            label: "identity_system_prompt".to_string(),
            source: "connector".to_string(),
            content: format!("## System Prompt\n{base_prompt}"),
            context_usage_kind: None,
        });
    }
    if agent_identity.is_some() || agent_prompt.is_some() {
        let mut lines = vec!["## Agent Identity".to_string()];
        if let Some(identity) = agent_identity {
            let identity = strip_agent_identity_heading(identity);
            if !identity.is_empty() {
                lines.push(identity.to_string());
            }
        }
        if let Some(prompt) = agent_prompt {
            lines.push(String::new());
            lines.push(prompt.to_string());
        }
        fragments.push(RuntimeContextFragmentEntry {
            slot: "identity".to_string(),
            label: "identity_agent_profile".to_string(),
            source: "project_agent".to_string(),
            content: lines.join("\n"),
            context_usage_kind: None,
        });
    }
    let rendered_text = render_identity_context(input)
        .expect("non-empty identity fragments always produce model context");
    Some(stable_system_frame(
        ContextFrameKind::Identity,
        rendered_text,
        vec![ContextFrameSection::Identity {
            title: "Identity".to_string(),
            summary: "Connector 的全局 system prompt、ProjectAgent 固定身份与 agent-level system prompt。"
                .to_string(),
            fragments,
        }],
    ))
}

#[must_use]
pub fn build_user_context_frame(input: &UserContextFacts) -> Option<ContextFrameFacts> {
    let rendered_text = render_user_context(input)?;
    Some(stable_system_frame(
        ContextFrameKind::UserContext,
        rendered_text,
        vec![ContextFrameSection::UserContext {
            title: "User Context".to_string(),
            summary: "操作者身份信息（人类用户）。".to_string(),
            user_id: Some(input.user_id.clone()),
            display_name: input.display_name.clone(),
            email: input.email.clone(),
            groups: input.groups.clone(),
            provider: input.provider.clone(),
            extra: input.extra.clone(),
        }],
    ))
}

#[must_use]
pub fn build_environment_context_frame(
    input: &EnvironmentContextFacts,
) -> Option<ContextFrameFacts> {
    let rendered_text = render_environment_context(input)?;
    let model_id = input
        .model_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let working_directory = input
        .working_directory
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let is_windows = input.platform.eq_ignore_ascii_case("windows");
    let summary = if is_windows {
        format!(
            "{} {} | {} | PowerShell text output note",
            input.platform, input.arch, input.date_utc
        )
    } else {
        format!("{} {} | {}", input.platform, input.arch, input.date_utc)
    };
    Some(stable_system_frame(
        ContextFrameKind::Environment,
        rendered_text,
        vec![ContextFrameSection::Environment {
            title: "Environment".to_string(),
            summary,
            date: Some(input.date_utc.clone()),
            platform: Some(format!("{} {}", input.platform, input.arch)),
            model_id,
            executor: Some(input.executor.clone()),
            working_directory,
        }],
    ))
}

#[must_use]
pub fn build_guidelines_context_frame(input: &GuidelinesContextFacts) -> Option<ContextFrameFacts> {
    let preferences = input
        .user_preferences
        .iter()
        .map(|preference| preference.trim().to_string())
        .filter(|preference| !preference.is_empty())
        .collect::<Vec<_>>();
    let entries = input
        .discovered_guidelines
        .iter()
        .filter(|guideline| !guideline.content.trim().is_empty())
        .map(|guideline| ProjectGuidelineEntry {
            path: guideline.path.clone(),
            content: guideline.content.clone(),
        })
        .collect::<Vec<_>>();
    if preferences.is_empty() && entries.is_empty() {
        return None;
    }

    let mut sections = Vec::new();
    if !preferences.is_empty() {
        sections.push(ContextFrameSection::UserPreferences {
            title: "User Preferences".to_string(),
            summary: "用户级偏好设置。".to_string(),
            items: preferences,
        });
    }
    if !entries.is_empty() {
        sections.push(ContextFrameSection::ProjectGuidelines {
            title: "Project Guidelines".to_string(),
            summary: "工作区中发现的项目级指引文件。".to_string(),
            entries,
        });
    }
    let rendered_text = render_guidelines_context(input)
        .expect("non-empty guideline sections always produce model context");
    Some(stable_system_frame(
        ContextFrameKind::SystemGuidelines,
        rendered_text,
        sections,
    ))
}

#[must_use]
pub fn build_memory_context_frame(input: &MemoryContextFacts) -> Option<ContextFrameFacts> {
    if input.sources.is_empty() && input.diagnostics.is_empty() {
        return None;
    }
    let sources = input
        .sources
        .iter()
        .map(runtime_memory_source_entry)
        .collect();
    let diagnostics = input
        .diagnostics
        .iter()
        .map(runtime_memory_diagnostic_entry)
        .collect();
    Some(ContextFrameFacts {
        kind: ContextFrameKind::MemoryContext,
        source: ContextFrameSource::RuntimeContextUpdate,
        phase_node: None,
        apply_mode: None,
        delivery_status: ContextDeliveryStatus::PreparedForConnector,
        delivery_channel: ContextDeliveryChannel::TurnStart,
        message_role: ContextMessageRole::User,
        rendered_text: render_memory_context(input),
        sections: vec![ContextFrameSection::MemoryInventory {
            title: "Memory Context".to_string(),
            summary: "Runtime-discovered memory source inventory and index pointers.".to_string(),
            mode: RuntimeMemoryInventoryMode::Snapshot,
            sources,
            diagnostics,
            added_sources: Vec::new(),
            removed_sources: Vec::new(),
            changed_sources: Vec::new(),
        }],
    })
}

#[must_use]
pub fn build_assignment_context_frame(input: &AssignmentContextFacts) -> Option<ContextFrameFacts> {
    let fragments = assignment_runtime_fragments(input);
    if fragments.is_empty() {
        return None;
    }
    let phase_tag = input.phase_tag.as_deref().unwrap_or("bootstrap");
    Some(ContextFrameFacts {
        kind: ContextFrameKind::AssignmentContext,
        source: ContextFrameSource::RuntimeContextUpdate,
        phase_node: Some(phase_tag.to_string()),
        apply_mode: input.apply_mode.clone(),
        delivery_status: ContextDeliveryStatus::QueuedForTransformContext,
        delivery_channel: ContextDeliveryChannel::TurnStart,
        message_role: ContextMessageRole::User,
        rendered_text: render_assignment_context(&fragments),
        sections: vec![ContextFrameSection::AssignmentContext {
            title: "Assignment Context".to_string(),
            summary: format!(
                "当前任务上下文已注入，本 frame 汇聚 {} 个上下文片段。",
                fragments.len()
            ),
            fragments,
        }],
    })
}

fn render_identity_context(input: &IdentityContextFacts) -> Option<String> {
    let base_prompt = input.base_system_prompt.trim();
    let agent_identity = input
        .agent_identity_markdown
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let agent_prompt = input
        .agent_system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut fragments = Vec::new();
    if !base_prompt.is_empty() {
        fragments.push(format!("## System Prompt\n{base_prompt}"));
    }
    if agent_identity.is_some() || agent_prompt.is_some() {
        let mut lines = vec!["## Agent Identity".to_string()];
        if let Some(identity) = agent_identity {
            let identity = strip_agent_identity_heading(identity);
            if !identity.is_empty() {
                lines.push(identity.to_string());
            }
        }
        if let Some(prompt) = agent_prompt {
            lines.push(String::new());
            lines.push(prompt.to_string());
        }
        fragments.push(lines.join("\n"));
    }
    (!fragments.is_empty()).then(|| format!("# Identity\n\n{}", fragments.join("\n\n")))
}

fn render_user_context(input: &UserContextFacts) -> Option<String> {
    if input.user_id.starts_with("system:") {
        return None;
    }
    let mut lines = vec!["## User Context".to_string()];
    if let Some(display_name) = &input.display_name {
        lines.push(format!("- Name: {display_name}"));
    }
    lines.push(format!("- User ID: {}", input.user_id));
    if let Some(email) = &input.email {
        lines.push(format!("- Email: {email}"));
    }
    if !input.groups.is_empty() {
        lines.push(format!("- Groups: {}", input.groups.join(", ")));
    }
    if let Some(provider) = &input.provider {
        lines.push(format!("- Provider: {provider}"));
    }
    if let Some(extra) = input.extra.as_object() {
        for (key, value) in extra {
            let value = value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| value.to_string());
            lines.push(format!("- {key}: {value}"));
        }
    }
    Some(lines.join("\n"))
}

fn render_environment_context(input: &EnvironmentContextFacts) -> Option<String> {
    if input.date_utc.is_empty() {
        return None;
    }
    let mut lines = vec![
        "## Environment".to_string(),
        format!("- Date: {} (UTC)", input.date_utc),
        format!("- Platform: {} {}", input.platform, input.arch),
    ];
    if let Some(model_id) = input.model_id.as_deref().filter(|value| !value.is_empty()) {
        lines.push(format!("- Model: {model_id}"));
    }
    lines.push(format!("- Executor: {}", input.executor));
    if let Some(working_directory) = input
        .working_directory
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("- Working directory: {working_directory}"));
    }
    if input.platform.eq_ignore_ascii_case("windows") {
        lines.push(format!("- {WINDOWS_POWERSHELL_TEXT_OUTPUT_NOTE}"));
    }
    Some(lines.join("\n"))
}

fn render_guidelines_context(input: &GuidelinesContextFacts) -> Option<String> {
    let preferences = input
        .user_preferences
        .iter()
        .map(|preference| preference.trim())
        .filter(|preference| !preference.is_empty())
        .map(|preference| format!("- {preference}"))
        .collect::<Vec<_>>();
    let guidelines = input
        .discovered_guidelines
        .iter()
        .filter(|guideline| !guideline.content.trim().is_empty())
        .map(|guideline| format!("### {}\n\n{}", guideline.path, guideline.content))
        .collect::<Vec<_>>();
    let mut sections = Vec::new();
    if !preferences.is_empty() {
        sections.push(format!("## User Preferences\n\n{}", preferences.join("\n")));
    }
    if !guidelines.is_empty() {
        sections.push(format!(
            "## Project Guidelines\n\n{}",
            guidelines.join("\n\n")
        ));
    }
    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

fn assignment_runtime_fragments(
    input: &AssignmentContextFacts,
) -> Vec<RuntimeContextFragmentEntry> {
    let mut fragments = input
        .fragments
        .iter()
        .filter(|fragment| fragment.runtime_agent_scope)
        .filter(|fragment| ASSIGNMENT_CONTEXT_SLOTS.contains(&fragment.slot.as_str()))
        .filter(|fragment| !fragment.content.trim().is_empty())
        .collect::<Vec<_>>();
    fragments.sort_by_key(|fragment| fragment.order);
    fragments
        .into_iter()
        .map(|fragment| RuntimeContextFragmentEntry {
            slot: fragment.slot.clone(),
            label: fragment.label.clone(),
            source: fragment.source.clone(),
            content: fragment.content.clone(),
            context_usage_kind: fragment.context_usage_kind.clone(),
        })
        .collect()
}

fn stable_system_frame(
    kind: ContextFrameKind,
    rendered_text: String,
    sections: Vec<ContextFrameSection>,
) -> ContextFrameFacts {
    ContextFrameFacts {
        kind,
        source: ContextFrameSource::RuntimeContextUpdate,
        phase_node: None,
        apply_mode: None,
        delivery_status: ContextDeliveryStatus::PreparedForConnector,
        delivery_channel: ContextDeliveryChannel::ConnectorContext,
        message_role: ContextMessageRole::System,
        rendered_text,
        sections,
    }
}

fn strip_agent_identity_heading(content: &str) -> &str {
    content
        .strip_prefix("## Agent Identity")
        .or_else(|| content.strip_prefix("# Agent Identity"))
        .map(str::trim)
        .unwrap_or(content)
}

fn render_assignment_context(fragments: &[RuntimeContextFragmentEntry]) -> String {
    let mut sections = vec![
        "# Assignment Context".to_string(),
        "以下上下文片段已在本轮对话开始前注入，用于约束任务目标、工作流要求与项目规则。"
            .to_string(),
    ];
    for fragment in fragments {
        let label = if fragment.label.trim().is_empty() {
            fragment.slot.as_str()
        } else {
            fragment.label.as_str()
        };
        sections.push(format!(
            "## {} (`{}`)\nsource: `{}`\n\n{}",
            label,
            fragment.slot,
            fragment.source,
            demote_markdown_headings(fragment.content.trim())
        ));
    }
    sections.join("\n\n")
}

fn demote_markdown_headings(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            let heading_marks = line
                .chars()
                .take_while(|character| *character == '#')
                .count();
            if (1..6).contains(&heading_marks) && line.chars().nth(heading_marks) == Some(' ') {
                format!("#{line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn runtime_memory_source_entry(source: &MemorySourceFacts) -> RuntimeMemorySourceEntry {
    RuntimeMemorySourceEntry {
        provider_key: source.provider_key.clone(),
        source_key: source.source_key.clone(),
        display_name: source.display_name.clone(),
        source_uri: source.source_uri.clone(),
        index_uri: source.index_uri.clone(),
        mount_id: source.mount_id.clone(),
        scope: source.scope.clone(),
        index_status: source.index_status.clone(),
        trust_level: source.trust_level.clone(),
        revision: source.revision.clone(),
        summary: source.summary.clone(),
        context_usage_kind: source.context_usage_kind.clone(),
    }
}

fn runtime_memory_diagnostic_entry(
    diagnostic: &MemoryDiagnosticFacts,
) -> RuntimeMemoryDiagnosticEntry {
    RuntimeMemoryDiagnosticEntry {
        provider_key: diagnostic.provider_key.clone(),
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        source_key: diagnostic.source_key.clone(),
        uri: diagnostic.uri.clone(),
        context_usage_kind: diagnostic.context_usage_kind.clone(),
    }
}

fn render_memory_context(facts: &MemoryContextFacts) -> String {
    let mut parts = vec![
        "## Memory Context".to_string(),
        [
            "Policy:",
            "- Treat memory as historical claims; verify code, configuration, paths, and external facts before acting on them.",
            "- Use the listed index to select relevant topic files, then read topic bodies through VFS only when needed.",
            "- If the user says to ignore memory, do not use, cite, compare, or mention memory.",
        ]
        .join("\n"),
        render_memory_inventory(facts),
    ];
    if let Some(default_source) = facts
        .sources
        .iter()
        .find(|source| source.source_uri == "agent://")
        .or_else(|| facts.sources.first())
    {
        parts.push(format!(
            "Default source: `{}`\nDefault index: `{}`",
            default_source.source_uri, default_source.index_uri
        ));
    }
    let bounded_indexes = facts
        .sources
        .iter()
        .filter(|source| source.index_status == "present")
        .filter_map(|source| {
            let content = source.bounded_index_content.as_deref()?.trim();
            (!content.is_empty()).then(|| {
                format!(
                    "### Bounded Index: `{}`\n\n```markdown\n{}\n```",
                    source.index_uri, content
                )
            })
        })
        .collect::<Vec<_>>();
    if !bounded_indexes.is_empty() {
        parts.push(bounded_indexes.join("\n\n"));
    }
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_memory_inventory(facts: &MemoryContextFacts) -> String {
    let mut lines = vec!["Sources:".to_string()];
    for source in &facts.sources {
        lines.push(format!(
            "- {}: source `{}`, index `{}`, scope {}, status {}, capabilities [{}]",
            source.display_name,
            source.source_uri,
            source.index_uri,
            source.scope,
            source.index_status,
            source.capabilities.join(", ")
        ));
    }
    if !facts.diagnostics.is_empty() {
        lines.push("Diagnostics:".to_string());
        for diagnostic in &facts.diagnostics {
            let uri = diagnostic
                .uri
                .as_deref()
                .map(|uri| format!(" `{uri}`"))
                .unwrap_or_default();
            lines.push(format!(
                "- {}{}: {}",
                diagnostic.code, uri, diagnostic.message
            ));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_bootstrap_matches_main_family_and_payload_order() {
        let facts = BootstrapContextFacts {
            include_startup_context: true,
            identity: IdentityContextFacts {
                base_system_prompt: "base identity".to_string(),
                agent_identity_markdown: Some("## Agent Identity\n- preset: `general`".to_string()),
                agent_system_prompt: Some("agent rules".to_string()),
            },
            user: Some(UserContextFacts {
                user_id: "u-123".to_string(),
                display_name: Some("Zhang San".to_string()),
                email: None,
                groups: vec!["Backend Team".to_string()],
                provider: Some("oidc".to_string()),
                extra: serde_json::json!({"tenant": "agentdash"}),
            }),
            environment: EnvironmentContextFacts {
                date_utc: "2026-07-01".to_string(),
                platform: "linux".to_string(),
                arch: "x86_64".to_string(),
                executor: "PI_AGENT".to_string(),
                ..Default::default()
            },
            guidelines: GuidelinesContextFacts {
                user_preferences: vec![" 使用中文 ".to_string()],
                discovered_guidelines: vec![DiscoveredGuidelineFacts {
                    path: "AGENTS.md".to_string(),
                    content: "项目约定".to_string(),
                }],
            },
            memory: MemoryContextFacts {
                sources: vec![MemorySourceFacts {
                    provider_key: "builtin.project_agent_memory".to_string(),
                    source_key: "agent".to_string(),
                    display_name: "Agent Memory".to_string(),
                    source_uri: "agent://".to_string(),
                    index_uri: "agent://MEMORY.md".to_string(),
                    mount_id: "agent".to_string(),
                    scope: "agent".to_string(),
                    capabilities: vec!["read".to_string(), "write".to_string()],
                    index_status: "present".to_string(),
                    trust_level: "first_party".to_string(),
                    revision: "rev-1".to_string(),
                    summary: None,
                    bounded_index_content: Some("- [Decision](topics/decision.md)".to_string()),
                    context_usage_kind: Some("memory".to_string()),
                }],
                diagnostics: Vec::new(),
            },
            assignment: AssignmentContextFacts {
                fragments: vec![AssignmentFragmentFacts {
                    slot: "task".to_string(),
                    label: "Task".to_string(),
                    order: 10,
                    runtime_agent_scope: true,
                    source: "task".to_string(),
                    content: "## Task\nRestore frames".to_string(),
                    context_usage_kind: Some("system_developer".to_string()),
                }],
                ..Default::default()
            },
        };

        let projected = facts.project();
        assert_eq!(
            projected
                .stable_frames
                .iter()
                .map(|frame| frame.kind)
                .collect::<Vec<_>>(),
            vec![
                ContextFrameKind::Identity,
                ContextFrameKind::UserContext,
                ContextFrameKind::Environment,
                ContextFrameKind::SystemGuidelines,
                ContextFrameKind::MemoryContext,
            ]
        );
        assert_eq!(
            projected.assignment_frame.as_ref().map(|frame| frame.kind),
            Some(ContextFrameKind::AssignmentContext)
        );
        assert!(
            projected.stable_frames[0]
                .rendered_text
                .contains("## Agent Identity\n- preset: `general`\n\nagent rules")
        );
        assert!(
            projected.stable_frames[4]
                .rendered_text
                .contains("Default source: `agent://`")
        );

        let model = facts.project_model_context();
        assert_eq!(
            model
                .instructions
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>(),
            vec![
                "bootstrap:identity",
                "bootstrap:user_context",
                "bootstrap:environment",
                "bootstrap:system_guidelines",
            ]
        );
        assert_eq!(
            model
                .instructions
                .iter()
                .map(|entry| entry.content.as_str())
                .collect::<Vec<_>>(),
            projected.stable_frames[..4]
                .iter()
                .map(|frame| frame.rendered_text.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            model
                .context
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>(),
            vec!["bootstrap:assignment_context", "bootstrap:memory_context"]
        );
        let model_context_text = model
            .context
            .iter()
            .map(|entry| match entry.blocks.as_slice() {
                [ContextBlock::Instruction { text }] => text.as_str(),
                blocks => panic!("expected one typed instruction block, got {blocks:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            model_context_text,
            vec![
                projected
                    .assignment_frame
                    .as_ref()
                    .expect("assignment")
                    .rendered_text
                    .as_str(),
                projected.stable_frames[4].rendered_text.as_str(),
            ]
        );
    }

    #[test]
    fn main_suppression_rules_are_independent() {
        let disabled = BootstrapContextFacts::default().project();
        assert!(disabled.stable_frames.is_empty());
        assert!(disabled.assignment_frame.is_none());
        assert_eq!(
            BootstrapContextFacts::default().project_model_context(),
            BootstrapModelContextProjection::default()
        );

        assert!(build_identity_context_frame(&IdentityContextFacts::default()).is_none());
        assert!(
            build_user_context_frame(&UserContextFacts {
                user_id: "system:routine".to_string(),
                display_name: None,
                email: None,
                groups: Vec::new(),
                provider: None,
                extra: serde_json::Value::Null,
            })
            .is_none()
        );
        assert!(build_environment_context_frame(&EnvironmentContextFacts::default()).is_none());
        assert!(
            build_guidelines_context_frame(&GuidelinesContextFacts {
                user_preferences: vec!["  ".to_string()],
                discovered_guidelines: vec![DiscoveredGuidelineFacts {
                    path: "AGENTS.md".to_string(),
                    content: " \n".to_string(),
                }],
            })
            .is_none()
        );
        assert!(build_memory_context_frame(&MemoryContextFacts::default()).is_none());
        assert!(
            build_assignment_context_frame(&AssignmentContextFacts {
                fragments: vec![AssignmentFragmentFacts {
                    slot: "tool".to_string(),
                    label: String::new(),
                    order: 0,
                    runtime_agent_scope: true,
                    source: "test".to_string(),
                    content: "not assignment".to_string(),
                    context_usage_kind: None,
                }],
                ..Default::default()
            })
            .is_none()
        );
    }
}
