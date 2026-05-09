use std::path::Path;

use agentdash_agent_types::DynAgentTool;
use agentdash_spi::context::capability::SessionBaselineCapabilities;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookSessionRuntimeAccess, HookTurnStartNotice,
    RuntimeEventSource, RuntimeSkillEntry, RuntimeWorkspaceMountEntry, SharedHookSessionRuntime,
};
use agentdash_spi::{Mount, MountCapability, Vfs};

use crate::session::context_frame::{self, ContextFramePayload};

#[derive(Debug, Clone)]
struct WorkspaceSurfaceFrame {
    working_directory: String,
    default_mount: Option<String>,
    mounts: Vec<RuntimeWorkspaceMountEntry>,
}

impl WorkspaceSurfaceFrame {
    fn new(vfs: &Vfs, working_directory: &Path) -> Self {
        Self {
            working_directory: working_directory.to_string_lossy().replace('\\', "/"),
            default_mount: vfs.default_mount().map(|mount| mount.id.clone()),
            mounts: vfs.mounts.iter().map(workspace_mount_entry).collect(),
        }
    }
}

impl ContextFramePayload for WorkspaceSurfaceFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("workspace-surface-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        "workspace_surface"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn delivery_status(&self) -> String {
        "queued_for_transform_context".to_string()
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::WorkspaceSurface {
            title: "Workspace Surface".to_string(),
            summary: format!(
                "当前工作目录与 {} 个 VFS 挂载已作为独立上下文帧注入。",
                self.mounts.len()
            ),
            working_directory: Some(self.working_directory.clone()),
            default_mount: self.default_mount.clone(),
            mounts: self.mounts.clone(),
        }]
    }

    fn rendered_text(&self) -> String {
        render_workspace_surface_text(self)
    }
}

#[derive(Debug, Clone)]
struct SkillSurfaceFrame {
    read_tool: Option<String>,
    skills: Vec<RuntimeSkillEntry>,
}

impl SkillSurfaceFrame {
    fn from_parts(
        caps: &SessionBaselineCapabilities,
        runtime_tools: &[DynAgentTool],
    ) -> Option<Self> {
        let skills = caps
            .visible_skills()
            .into_iter()
            .map(|skill| RuntimeSkillEntry {
                name: skill.name.clone(),
                description: skill.description.clone(),
                file_path: skill.file_path.clone(),
                disable_model_invocation: skill.disable_model_invocation,
            })
            .collect::<Vec<_>>();
        if skills.is_empty() {
            return None;
        }
        Some(Self {
            read_tool: read_tool_name(runtime_tools),
            skills,
        })
    }
}

impl ContextFramePayload for SkillSurfaceFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("skill-surface-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        "skill_surface"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn delivery_status(&self) -> String {
        "queued_for_transform_context".to_string()
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::SkillSurface {
            title: "Skill Surface".to_string(),
            summary: format!("当前有 {} 个可由模型按需加载的 skill。", self.skills.len()),
            read_tool: self.read_tool.clone(),
            skills: self.skills.clone(),
        }]
    }

    fn rendered_text(&self) -> String {
        render_skill_surface_text(self)
    }
}

#[derive(Debug, Clone)]
struct HookRuntimeSurfaceFrame {
    pending_action_count: usize,
}

impl HookRuntimeSurfaceFrame {
    fn from_runtime(hook_session: &dyn HookSessionRuntimeAccess) -> Self {
        Self {
            pending_action_count: hook_session.pending_actions().len(),
        }
    }
}

impl ContextFramePayload for HookRuntimeSurfaceFrame {
    fn id(&self, created_at_ms: i64) -> String {
        format!("hook-runtime-surface-{created_at_ms}")
    }

    fn kind(&self) -> &'static str {
        "hook_runtime_surface"
    }

    fn source(&self) -> RuntimeEventSource {
        RuntimeEventSource::RuntimeContextUpdate
    }

    fn delivery_status(&self) -> String {
        "queued_for_transform_context".to_string()
    }

    fn sections(&self) -> Vec<ContextFrameSection> {
        vec![ContextFrameSection::HookRuntimeSurface {
            title: "Hook Runtime Surface".to_string(),
            summary: "Hook Runtime 已启用；流程约束、pending action 与 stop gate 将通过后续 ContextFrame 持续注入。".to_string(),
            pending_action_count: self.pending_action_count,
        }]
    }

    fn rendered_text(&self) -> String {
        render_hook_runtime_surface_text(self)
    }
}

pub(crate) fn enqueue_workspace_surface_frame(
    hook_session: Option<&SharedHookSessionRuntime>,
    vfs: &Vfs,
    working_directory: &Path,
) -> Option<ContextFrame> {
    enqueue_frame(
        hook_session?,
        &WorkspaceSurfaceFrame::new(vfs, working_directory),
    )
}

pub(crate) fn enqueue_skill_surface_frame(
    hook_session: Option<&SharedHookSessionRuntime>,
    caps: &SessionBaselineCapabilities,
    runtime_tools: &[DynAgentTool],
) -> Option<ContextFrame> {
    let metadata = SkillSurfaceFrame::from_parts(caps, runtime_tools)?;
    enqueue_frame(hook_session?, &metadata)
}

pub(crate) fn enqueue_hook_runtime_surface_frame(
    hook_session: Option<&SharedHookSessionRuntime>,
) -> Option<ContextFrame> {
    let hook_session = hook_session?;
    let metadata = HookRuntimeSurfaceFrame::from_runtime(hook_session.as_ref());
    enqueue_frame(hook_session, &metadata)
}

fn enqueue_frame(
    hook_session: &SharedHookSessionRuntime,
    metadata: &impl ContextFramePayload,
) -> Option<ContextFrame> {
    let frame = context_frame::build_context_frame(metadata);
    if frame.rendered_text.trim().is_empty() {
        return None;
    }
    hook_session.enqueue_turn_start_notice(HookTurnStartNotice {
        id: frame.id.clone(),
        created_at_ms: frame.created_at_ms,
        source: RuntimeEventSource::RuntimeContextUpdate,
        content: frame.rendered_text.clone(),
        context_frame: Some(frame.clone()),
    });
    Some(frame)
}

fn workspace_mount_entry(mount: &Mount) -> RuntimeWorkspaceMountEntry {
    RuntimeWorkspaceMountEntry {
        id: mount.id.clone(),
        display_name: mount.display_name.clone(),
        provider: mount.provider.clone(),
        root_ref: mount.root_ref.clone(),
        capabilities: mount
            .capabilities
            .iter()
            .map(mount_capability_label)
            .map(ToString::to_string)
            .collect(),
    }
}

fn mount_capability_label(capability: &MountCapability) -> &'static str {
    match capability {
        MountCapability::Read => "read",
        MountCapability::Write => "write",
        MountCapability::List => "list",
        MountCapability::Search => "search",
        MountCapability::Exec => "exec",
        MountCapability::Watch => "watch",
    }
}

fn read_tool_name(runtime_tools: &[DynAgentTool]) -> Option<String> {
    runtime_tools
        .iter()
        .map(|tool| tool.name())
        .find(|name| *name == "fs_read" || *name == "read_file")
        .map(ToString::to_string)
}

fn render_workspace_surface_text(frame: &WorkspaceSurfaceFrame) -> String {
    let mut lines = vec![
        "## Workspace Surface".to_string(),
        format!("working_directory: `{}`", frame.working_directory),
    ];
    if let Some(default_mount) = frame.default_mount.as_deref() {
        lines.push(format!("default_mount: `{default_mount}`"));
    }
    lines.push(String::new());
    lines.push("### VFS Mounts".to_string());
    if frame.mounts.is_empty() {
        lines.push("当前会话未配置 VFS mount。".to_string());
    } else {
        for mount in &frame.mounts {
            lines.push(format!(
                "- `{}`: {} (provider={}, root_ref={}, capabilities=[{}])",
                mount.id,
                mount.display_name,
                mount.provider,
                mount.root_ref,
                mount.capabilities.join(", ")
            ));
        }
    }
    lines.join("\n")
}

fn render_skill_surface_text(frame: &SkillSurfaceFrame) -> String {
    let mut lines = vec![
        "## Skill Surface".to_string(),
        "以下 skills 提供特定任务的专门说明。任务匹配时先读取对应 SKILL.md，再按其内容执行。"
            .to_string(),
    ];
    if let Some(read_tool) = frame.read_tool.as_deref() {
        lines.push(format!(
            "使用 `{read_tool}` 读取 skill 文件；相对路径以 SKILL.md 所在目录为基准。"
        ));
    } else {
        lines.push(
            "当前 provider request 未暴露可读取 skill 文件的工具；不要假设已加载 skill 正文。"
                .to_string(),
        );
    }
    lines.push(String::new());
    lines.push("<available_skills>".to_string());
    for skill in &frame.skills {
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
    lines.join("\n")
}

fn render_hook_runtime_surface_text(frame: &HookRuntimeSurfaceFrame) -> String {
    let mut lines = vec![
        "## Hook Runtime Surface".to_string(),
        "当前会话启用了 Hook Runtime。active workflow、流程约束、stop gate 与 pending action 等动态治理信息，会在 LLM 调用边界通过 ContextFrame 注入。".to_string(),
    ];
    if frame.pending_action_count > 0 {
        lines.push(format!(
            "pending_action_count: {}。后续 pending_action frame 中的要求优先处理。",
            frame.pending_action_count
        ));
    }
    lines.join("\n\n")
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_types::{
        AgentTool, AgentToolError, AgentToolResult, ContentPart, ToolUpdateCallback,
    };
    use agentdash_spi::{SkillEntry, Vfs};
    use async_trait::async_trait;
    use serde_json::Value;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    struct ReadTool;

    #[async_trait]
    impl AgentTool for ReadTool {
        fn name(&self) -> &str {
            "fs_read"
        }

        fn description(&self) -> &str {
            "read"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({})
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
    fn skill_surface_renders_from_capability_metadata() {
        let caps = SessionBaselineCapabilities {
            skills: vec![SkillEntry {
                name: "trellis-check".to_string(),
                description: "质量验证".to_string(),
                file_path: "/workspace/.agents/skills/trellis-check/SKILL.md".to_string(),
                disable_model_invocation: false,
            }],
        };
        let tools: Vec<DynAgentTool> = vec![Arc::new(ReadTool)];
        let frame = SkillSurfaceFrame::from_parts(&caps, &tools).expect("skill surface");

        assert_eq!(frame.read_tool.as_deref(), Some("fs_read"));
        assert!(frame.rendered_text().contains("<available_skills>"));
        assert!(matches!(
            frame.sections().first(),
            Some(ContextFrameSection::SkillSurface { skills, .. }) if skills.len() == 1
        ));
    }

    #[test]
    fn workspace_surface_is_independent_frame_kind() {
        let mut vfs = Vfs::default();
        vfs.default_mount_id = Some("workspace".to_string());
        vfs.mounts.push(Mount {
            id: "workspace".to_string(),
            provider: "local".to_string(),
            backend_id: "backend".to_string(),
            root_ref: "/repo".to_string(),
            capabilities: vec![MountCapability::Read, MountCapability::Write],
            default_write: true,
            display_name: "Workspace".to_string(),
            metadata: serde_json::Value::Null,
        });
        let frame = WorkspaceSurfaceFrame::new(&vfs, Path::new("/repo"));

        assert_eq!(frame.kind(), "workspace_surface");
        assert!(frame.rendered_text().contains("default_mount"));
        assert!(matches!(
            frame.sections().first(),
            Some(ContextFrameSection::WorkspaceSurface { mounts, .. }) if mounts.len() == 1
        ));
    }
}
