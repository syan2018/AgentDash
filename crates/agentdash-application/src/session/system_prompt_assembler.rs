//! System Prompt 组装器
//!
//! 在 Application 层完成 system prompt 全文本组装，
//! Connector 只收到最终的 `String`，不接触任何应用层概念。
//!
//! 四层 Identity Pipeline:
//!   Layer 0: base system prompt（内置或 settings override）
//!   Layer 1: agent-level system_prompt（executor_config，Append / Override）
//!   Layer 2: user preferences（settings 列表）
//!   Layer 3: project guidelines（AGENTS.md / MEMORY.md）

use std::path::Path;

use agentdash_agent_types::DynAgentTool;
use agentdash_domain::common::SystemPromptMode;
use agentdash_spi::context::bundle::SessionContextBundle;
use agentdash_spi::context::capability::SessionBaselineCapabilities;
use agentdash_spi::hooks::HookSessionRuntimeAccess;
use agentdash_spi::{DiscoveredGuideline, Mount, MountCapability, Vfs};

/// `Project Context` 渲染的业务上下文 slot。
///
/// VFS、runtime policy、MCP server 等运行时表面由本文件后续章节或 runtime
/// notice 统一渲染；这里排除这些 slot，避免同一 mount/tool/MCP 能力在 system
/// prompt 里重复出现。
const PROJECT_CONTEXT_SLOTS: &[&str] = &[
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
    "project_guidelines",
    "instruction",
    "instruction_append",
    "companion_agents",
];

/// assembler 的全部输入——只在 application 层组装，不穿透到 connector。
pub struct SystemPromptInput<'a> {
    pub base_system_prompt: &'a str,
    pub agent_system_prompt: Option<&'a str>,
    pub agent_system_prompt_mode: Option<SystemPromptMode>,
    pub user_preferences: &'a [String],
    pub discovered_guidelines: &'a [DiscoveredGuideline],
    pub context_bundle: Option<&'a SessionContextBundle>,
    pub session_capabilities: Option<&'a SessionBaselineCapabilities>,
    pub vfs: Option<&'a Vfs>,
    pub working_directory: &'a Path,
    /// 当前工具列表仅用于派生 skill 提示中的工具名，不在 system prompt 中渲染
    /// schema。完整工具 schema 统一由 runtime tool schema notice 注入。
    pub runtime_tools: &'a [DynAgentTool],
    pub mcp_servers: &'a [agentdash_spi::SessionMcpServer],
    pub hook_session: Option<&'a dyn HookSessionRuntimeAccess>,
}

/// 组装完整的 runtime system prompt 文本。
pub fn assemble_system_prompt(input: &SystemPromptInput) -> String {
    let tool_names: Vec<String> = input
        .runtime_tools
        .iter()
        .map(|tool| tool.name().to_string())
        .collect();
    let mut sections: Vec<String> = Vec::new();

    // ── 1. Identity: 四层提示合并 ──
    {
        let agent_sp = input.agent_system_prompt.filter(|s| !s.trim().is_empty());

        let mut identity = match (input.agent_system_prompt_mode, agent_sp) {
            (Some(SystemPromptMode::Override), Some(sp)) => sp.to_string(),
            (_, Some(sp)) => format!("{}\n\n{sp}", input.base_system_prompt),
            _ => input.base_system_prompt.to_string(),
        };

        if !input.user_preferences.is_empty() {
            let prefs = input
                .user_preferences
                .iter()
                .map(|p| format!("- {p}"))
                .collect::<Vec<_>>()
                .join("\n");
            identity.push_str(&format!("\n\n### User Preferences\n\n{prefs}"));
        }

        sections.push(format!("## Identity\n\n{identity}"));
    }

    // ── 1b. Project Guidelines: Layer 3（AGENTS.md / MEMORY.md） ──
    if !input.discovered_guidelines.is_empty() {
        let content = input
            .discovered_guidelines
            .iter()
            .map(|g| format!("### {}\n\n{}", g.path, g.content))
            .collect::<Vec<_>>()
            .join("\n\n");
        sections.push(format!("## Project Guidelines\n\n{content}"));
    }

    // ── 2. Project Context ──
    //
    // Companion agents 由 CapabilityState.companion 维度管理，Bundle 的
    // `companion_agents` slot 仍可承载 hook 注入的 fragment（如有）。
    if let Some(section) = input.context_bundle.and_then(render_runtime_section) {
        sections.push(section);
    }

    // ── 3. Workspace ──
    if let Some(vfs) = input.vfs {
        let mount_lines = vfs
            .mounts
            .iter()
            .map(describe_mount)
            .collect::<Vec<_>>()
            .join("\n");
        let default_mount = vfs.default_mount().map(|m| m.id.as_str()).unwrap_or("main");
        sections.push(format!(
            "## Workspace\n\n当前会话可访问的 VFS 挂载如下：\n\n{mount_lines}\n\n默认 mount：`{default_mount}`"
        ));
    } else {
        sections.push("## Workspace\n\n（当前会话未配置 VFS。）".to_string());
    }

    // ── 6. Hooks ──
    if let Some(hook_session) = input.hook_session {
        let hook_parts = build_hook_runtime_sections(hook_session);
        if !hook_parts.is_empty() {
            sections.push(format!("## Hooks\n\n{}", hook_parts.join("\n\n")));
        }
    }

    // ── 7. Skills ──
    if let Some(caps) = input.session_capabilities {
        if let Some(skills_block) = format_skills_from_capabilities(caps, &tool_names) {
            sections.push(skills_block);
        }
    }

    sections.join("\n\n")
}

/// 渲染 Bundle 的"Project Context"段落。
///
/// 返回 `None` 表示 Bundle 的 `RuntimeAgent` scope 下未产出任何可见内容。
pub fn render_runtime_section(bundle: &SessionContextBundle) -> Option<String> {
    let project_context = bundle.render_section(
        agentdash_spi::FragmentScope::RuntimeAgent,
        PROJECT_CONTEXT_SLOTS,
    );
    if project_context.trim().is_empty() {
        None
    } else {
        Some(format!("## Project Context\n\n{project_context}"))
    }
}

// ─── 渲染辅助函数（全部从 connector.rs / stream_mapper.rs 搬过来） ──────────

fn describe_mount(mount: &Mount) -> String {
    let capabilities = mount
        .capabilities
        .iter()
        .map(|c| match c {
            MountCapability::Read => "read",
            MountCapability::Write => "write",
            MountCapability::List => "list",
            MountCapability::Search => "search",
            MountCapability::Exec => "exec",
            MountCapability::Watch => "watch",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "- {}: {}（provider={}, root_ref={}, capabilities=[{}]）",
        mount.id, mount.display_name, mount.provider, mount.root_ref, capabilities
    )
}

fn format_skills_from_capabilities(
    caps: &SessionBaselineCapabilities,
    tool_names: &[String],
) -> Option<String> {
    let read_tool = tool_names
        .iter()
        .find(|t| *t == "fs_read" || *t == "read_file")?;

    let visible = caps.visible_skills();
    if visible.is_empty() {
        return None;
    }

    let mut lines = vec![
        format!(
            "The following skills provide specialized instructions for specific tasks.\n\
             Use the {read_tool} tool to load a skill's file when the task matches its description.\n\
             When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md)."
        ),
        String::new(),
        "<available_skills>".to_string(),
    ];
    for skill in &visible {
        lines.push("  <skill>".to_string());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
        lines.push(format!(
            "    <description>{}</description>",
            escape_xml(&skill.description)
        ));
        lines.push(format!(
            "    <location>{}</location>",
            escape_xml(&skill.file_path)
        ));
        lines.push("  </skill>".to_string());
    }
    lines.push("</available_skills>".to_string());
    Some(lines.join("\n"))
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn build_hook_runtime_sections(hook_session: &dyn HookSessionRuntimeAccess) -> Vec<String> {
    let mut sections = vec![
        "当前会话启用了 Hook Runtime。active workflow、流程约束、stop gate 与 pending action 等动态治理信息，会在每次 LLM 调用边界由 runtime 注入；这里不再重复展开它们的静态副本。".to_string(),
    ];
    let pending_actions = hook_session.pending_actions();
    if !pending_actions.is_empty() {
        sections.push(format!(
            "当前已有 {} 条待处理 hook action；请在后续动态注入消息中优先处理它们。",
            pending_actions.len()
        ));
    }
    sections
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use agentdash_agent_types::{
        AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
    };
    use agentdash_spi::{
        ContextFragment, McpTransportConfig, MergeStrategy, SessionContextBundle, SessionMcpServer,
    };
    use async_trait::async_trait;
    use serde_json::Value;
    use tokio_util::sync::CancellationToken;

    fn fragment(slot: &str, content: &str) -> ContextFragment {
        ContextFragment {
            slot: slot.to_string(),
            label: format!("test_{slot}"),
            order: 10,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "test".to_string(),
            content: content.to_string(),
        }
    }

    struct StubTool;

    #[async_trait]
    impl AgentTool for StubTool {
        fn name(&self) -> &str {
            "mcp_agentdash_workflow_tools_upsert_workflow_tool"
        }

        fn description(&self) -> &str {
            "创建或更新 Workflow 定义"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" }
                }
            })
        }

        async fn execute(
            &self,
            _tool_call_id: &str,
            _args: Value,
            _cancel: CancellationToken,
            _on_update: Option<ToolUpdateCallback>,
        ) -> Result<AgentToolResult, AgentToolError> {
            Ok(AgentToolResult {
                content: vec![ContentPart::text("ok")],
                is_error: false,
                details: None,
            })
        }
    }

    #[test]
    fn mcp_server_declarations_are_not_rendered_as_prompt_context() {
        let mcp_servers = vec![SessionMcpServer {
            name: "agentdash-workflow-tools".to_string(),
            transport: McpTransportConfig::Http {
                url: "http://127.0.0.1:3001/mcp/workflow/8de613e7-0000-0000-0000-000000000000"
                    .to_string(),
                headers: vec![],
            },
            uses_relay: false,
        }];
        let tools: Vec<DynAgentTool> = vec![];
        let prompt = assemble_system_prompt(&SystemPromptInput {
            base_system_prompt: "base",
            agent_system_prompt: None,
            agent_system_prompt_mode: None,
            user_preferences: &[],
            discovered_guidelines: &[],
            context_bundle: None,
            session_capabilities: None,
            vfs: None,
            working_directory: Path::new("."),
            runtime_tools: &tools,
            mcp_servers: &mcp_servers,
            hook_session: None,
        });

        assert!(!prompt.contains("agentdash-workflow-tools"));
        assert!(!prompt.contains("平台 MCP 管理工具"));
        assert!(!prompt.contains("/mcp/workflow/"));
        assert!(!prompt.contains("8de613e7"));
    }

    #[test]
    fn project_context_excludes_runtime_surface_slots() {
        let mut bundle = SessionContextBundle::new(uuid::Uuid::new_v4(), "task_start");
        bundle.merge([
            fragment("task", "## Task\n业务任务"),
            fragment("vfs", "## VFS\n重复 mount 摘要"),
            fragment("tools", "## Tool Visibility\n重复工具摘要"),
            fragment("runtime_policy", "## Runtime Policy\n重复 runtime 策略"),
            fragment("mcp_config", "## MCP\n重复 MCP 摘要"),
        ]);

        let rendered = render_runtime_section(&bundle).expect("project context");

        assert!(rendered.contains("业务任务"));
        assert!(!rendered.contains("重复 mount 摘要"));
        assert!(!rendered.contains("重复工具摘要"));
        assert!(!rendered.contains("重复 runtime 策略"));
        assert!(!rendered.contains("重复 MCP 摘要"));
    }

    #[test]
    fn system_prompt_does_not_render_tool_schema_or_available_tools() {
        let tools: Vec<DynAgentTool> = vec![Arc::new(StubTool)];

        let prompt = assemble_system_prompt(&SystemPromptInput {
            base_system_prompt: "base",
            agent_system_prompt: None,
            agent_system_prompt_mode: None,
            user_preferences: &[],
            discovered_guidelines: &[],
            context_bundle: None,
            session_capabilities: None,
            vfs: None,
            working_directory: Path::new("."),
            runtime_tools: &tools,
            mcp_servers: &[],
            hook_session: None,
        });

        assert!(!prompt.contains("## Available Tools"));
        assert!(!prompt.contains("mcp_agentdash_workflow_tools_upsert_workflow_tool"));
        assert!(!prompt.contains("创建或更新 Workflow 定义"));
    }
}
