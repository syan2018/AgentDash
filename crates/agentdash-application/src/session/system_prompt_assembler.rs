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
use agentdash_spi::context_injection::RUNTIME_AGENT_CONTEXT_SLOTS;
use agentdash_spi::hooks::HookSessionRuntimeAccess;
use agentdash_spi::session_capabilities::SessionBaselineCapabilities;
use agentdash_spi::session_context_bundle::SessionContextBundle;
use agentdash_spi::{DiscoveredGuideline, Mount, MountCapability, Vfs};

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
    pub runtime_tools: &'a [DynAgentTool],
    pub mcp_servers: &'a [agent_client_protocol::McpServer],
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
        let agent_sp = input
            .agent_system_prompt
            .filter(|s| !s.trim().is_empty());

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
    if let Some(bundle) = input.context_bundle {
        let project_context = bundle.render_section(
            agentdash_spi::FragmentScope::RuntimeAgent,
            RUNTIME_AGENT_CONTEXT_SLOTS,
        );
        if !project_context.trim().is_empty() {
            sections.push(format!("## Project Context\n\n{project_context}"));
        }
    }

    // ── 2b. Companion Agents ──
    if let Some(caps) = input.session_capabilities {
        if !caps.companion_agents.is_empty() {
            let lines = caps
                .companion_agents
                .iter()
                .map(|a| {
                    format!(
                        "- **{}** (executor: `{}`): {}",
                        a.name, a.executor, a.display_name
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!(
                "## Companion Agents\n\n\
                 以下 agent 已关联到当前项目，可通过 `companion_request` 工具的 `agent_key` 参数按名称指定：\n\n\
                 {lines}"
            ));
        }
    }

    // ── 3. Workspace ──
    if let Some(vfs) = input.vfs {
        let mount_lines = vfs
            .mounts
            .iter()
            .map(describe_mount)
            .collect::<Vec<_>>()
            .join("\n");
        let default_mount = vfs
            .default_mount()
            .map(|m| m.id.as_str())
            .unwrap_or("main");
        sections.push(format!(
            "## Workspace\n\n当前会话可访问的 VFS 挂载如下：\n\n{mount_lines}\n\n默认 mount：`{default_mount}`"
        ));
    } else {
        sections.push("## Workspace\n\n（当前会话未配置 VFS。）".to_string());
    }

    // ── 4. Available Tools ──
    {
        let has_builtin = !input.runtime_tools.is_empty();
        let (platform_mcp_servers, user_mcp_servers): (Vec<_>, Vec<_>) = input
            .mcp_servers
            .iter()
            .partition(|server| is_platform_mcp_server(server));
        let has_platform_mcp = !platform_mcp_servers.is_empty();
        let has_user_mcp = !user_mcp_servers.is_empty();

        if has_builtin || has_platform_mcp || has_user_mcp {
            let mut tool_section =
                String::from("## Available Tools\n\n以下工具已注入当前会话，可直接调用：\n\n");

            if has_builtin || has_platform_mcp {
                tool_section.push_str("### Platform Tools\n\n");
                if has_builtin {
                    let builtin_lines = input
                        .runtime_tools
                        .iter()
                        .map(describe_builtin_tool)
                        .collect::<Vec<_>>()
                        .join("\n");
                    tool_section.push_str(&builtin_lines);
                    tool_section.push_str("\n\n");
                }
                if has_platform_mcp {
                    let lines = platform_mcp_servers
                        .iter()
                        .map(|s| describe_mcp_server(s))
                        .collect::<Vec<_>>()
                        .join("\n");
                    tool_section.push_str(
                        "以下平台 MCP Server 提供 Project/Story/Task/Workflow 级管理工具：\n\n",
                    );
                    tool_section.push_str(&lines);
                    tool_section.push_str("\n\n");
                }
            }

            if has_user_mcp {
                let lines = user_mcp_servers
                    .iter()
                    .map(|s| describe_mcp_server(s))
                    .collect::<Vec<_>>()
                    .join("\n");
                tool_section.push_str("### MCP Tools\n\n");
                tool_section
                    .push_str("以下 MCP Server 已注入当前会话，其工具可在需要时使用：\n\n");
                tool_section.push_str(&lines);
                tool_section.push_str("\n\n");
            }

            if has_builtin {
                render_path_conventions(&mut tool_section, input.vfs, input.working_directory);
            }
            sections.push(tool_section);
        }
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

fn extract_mcp_server_name(server: &agent_client_protocol::McpServer) -> String {
    serde_json::to_value(server)
        .ok()
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn is_platform_mcp_server(server: &agent_client_protocol::McpServer) -> bool {
    extract_mcp_server_name(server).starts_with("agentdash-")
}

fn describe_mcp_server(server: &agent_client_protocol::McpServer) -> String {
    let value = serde_json::to_value(server).unwrap_or_default();
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed-mcp");
    let url = value
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown-url");
    let server_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    format!("- {name} ({server_type}): {url}")
}

fn describe_builtin_tool(tool: &DynAgentTool) -> String {
    let description = tool.description().trim();
    let header = if description.is_empty() {
        format!("- **{}**", tool.name())
    } else {
        format!("- **{}**: {}", tool.name(), description)
    };
    let params = format_parameters_brief(&tool.parameters_schema());
    if params.is_empty() {
        header
    } else {
        format!("{header}\n{params}")
    }
}

fn format_parameters_brief(schema: &serde_json::Value) -> String {
    let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) else {
        return String::new();
    };
    if properties.is_empty() {
        return String::new();
    }
    let required: std::collections::HashSet<&str> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    properties
        .iter()
        .map(|(name, spec)| {
            let ty = extract_type_label(spec);
            let marker = if required.contains(name.as_str()) {
                ", required"
            } else {
                ""
            };
            let param_desc = spec
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            match param_desc {
                Some(d) => format!("  - `{name}` ({ty}{marker}): {d}"),
                None => format!("  - `{name}` ({ty}{marker})"),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_type_label(spec: &serde_json::Value) -> String {
    if let Some(ty) = spec.get("type").and_then(|v| v.as_str()) {
        return ty.to_string();
    }
    if spec.get("oneOf").is_some() || spec.get("anyOf").is_some() {
        return "union".to_string();
    }
    "any".to_string()
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

fn build_hook_runtime_sections(
    hook_session: &dyn HookSessionRuntimeAccess,
) -> Vec<String> {
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

fn render_path_conventions(tool_section: &mut String, vfs: Option<&Vfs>, working_directory: &Path) {
    if vfs.is_some() {
        tool_section.push_str(
            "**Path convention**: paths MUST use `mount_id://relative/path` format (e.g., `main://src/lib.rs`). \
            The mount prefix may be omitted when the session has exactly one mount. \
            Never put backend_id or absolute paths into tool arguments. \
            For shell_exec, `cwd` must also be relative to the mount root; use `main://.` for the current directory.\n\n",
        );
        tool_section.push_str(
            "**fs_apply_patch format**: uses Codex apply_patch syntax (**not** unified diff). \
            Starts with `*** Begin Patch`, ends with `*** End Patch`. \
            Each file operation MUST begin with `*** Add File: path` / `*** Update File: path` / `*** Delete File: path`. \
            For renaming, follow `Update File` with `*** Move to: new/path`. \
            Each hunk starts with `@@` (optionally followed by a context-anchor line); \
            lines within a hunk are prefixed with space (context) / `-` (remove) / `+` (add). \
            Paths may use `mount_id://path` to target a specific mount; paths without a prefix use the default mount.",
        );
    } else {
        let abs_hint = working_directory.display().to_string();
        tool_section.push_str(&format!(
            "**路径规范**：调用 read_file、list_directory、search、write_file、shell 等工作空间工具时，路径参数必须优先使用相对工作空间根目录的路径。如果要在当前目录执行 shell，请将 cwd 设为 `.`；如果要进入子目录，请传类似 `crates/agentdash-agent` 这样的相对路径；不要把 `{abs_hint}/...` 这类绝对路径直接写进工具参数。",
        ));
    }
}
