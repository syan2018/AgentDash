use std::collections::{BTreeMap, BTreeSet};

use agentdash_application_ports::lifecycle_surface_projection::{
    ActivityActivation, KickoffPromptFragment, RuntimeNodeArtifactScope,
    lifecycle_mount_overlay_for_surface,
};
use agentdash_domain::inline_file::{InlineFileOwnerKind, InlineFileRepository};
use agentdash_domain::workflow::{
    ActivityDefinition, AgentProcedureContract, ToolCapabilityDirective,
};
use agentdash_spi::{CapabilityScopeCtx, Vfs};
use uuid::Uuid;

use crate::capability::{
    AuthorityState, AvailableMcpPresets, CapabilityResolver, CapabilityResolverInput,
    CompanionContribution, CompanionSliceMode, ContextContributionSource, ContextContributions,
    McpCandidates, ToolContribution,
};
use crate::companion::skill_projection::{
    append_companion_system_skill_key, append_lifecycle_companion_system_projection,
    ensure_companion_system_skill_asset,
};
use crate::platform_config::PlatformConfig;
use crate::repository_set::RepositorySet;

pub(super) struct ActivityActivationInput<'a> {
    pub owner_ctx: CapabilityScopeCtx,
    pub active_activity: &'a ActivityDefinition,
    pub workflow_contract: Option<&'a AgentProcedureContract>,
    pub base_vfs: Option<&'a Vfs>,
    pub run_id: Uuid,
    pub orchestration_id: Uuid,
    pub node_path: &'a str,
    pub attempt: u32,
    pub lifecycle_key: &'a str,
    pub available_presets: AvailableMcpPresets,
    pub authority_state: AuthorityState,
    pub agent_tool_directives: Vec<ToolCapabilityDirective>,
    pub companion_slice_mode: Option<CompanionSliceMode>,
    pub baseline_override: Option<Vec<ToolCapabilityDirective>>,
    pub tool_directives: &'a [ToolCapabilityDirective],
    pub ready_port_keys: BTreeSet<String>,
    pub available_companions: Vec<agentdash_spi::context::capability::CompanionAgentEntry>,
}

pub(super) fn activate_activity_with_platform(
    input: &ActivityActivationInput<'_>,
    platform: &PlatformConfig,
) -> Result<ActivityActivation, String> {
    let baseline_directives = input.baseline_override.clone().unwrap_or_else(|| {
        input
            .workflow_contract
            .map(|contract| contract.capability_config.tool_directives.clone())
            .unwrap_or_default()
    });

    let mut combined_directives = baseline_directives;
    combined_directives.extend(input.tool_directives.iter().cloned());
    let has_active_workflow = input.workflow_contract.is_some();

    let writable_port_keys =
        agentdash_application_ports::lifecycle_surface_projection::writable_port_keys_for_activity(
            input.active_activity,
        );
    let lifecycle_surface =
        agentdash_application_ports::lifecycle_surface_projection::LifecycleMountSurface {
            run_id: input.run_id,
            orchestration_id: input.orchestration_id,
            node_path: input.node_path.to_string(),
            lifecycle_key: input.lifecycle_key.to_string(),
            attempt: input.attempt,
            writable_port_keys,
        };
    let lifecycle_mount = lifecycle_mount_overlay_for_surface(&lifecycle_surface)
        .mounts
        .into_iter()
        .next()
        .expect("lifecycle node overlay contains one mount");
    let lifecycle_vfs = Vfs {
        mounts: vec![lifecycle_mount.clone()],
        default_mount_id: None,
        source_project_id: None,
        source_story_id: None,
        links: Vec::new(),
    };
    let mount_directives = input
        .workflow_contract
        .map(|contract| contract.capability_config.mount_directives.clone())
        .unwrap_or_default();
    let effective_vfs =
        crate::agent_run::runtime_capability::compose_vfs_with_overlay_and_directives(
            input.base_vfs,
            &lifecycle_vfs,
            &mount_directives,
        );

    let mut contributions = Vec::new();
    if !input.agent_tool_directives.is_empty() {
        contributions.push(ContextContributions {
            source: ContextContributionSource::Agent,
            tool: Some(ToolContribution {
                directives: input.agent_tool_directives.clone(),
                has_active_workflow: false,
            }),
            companion: None,
        });
    }
    contributions.push(ContextContributions {
        source: ContextContributionSource::Workflow,
        tool: Some(ToolContribution {
            directives: combined_directives,
            has_active_workflow,
        }),
        companion: if input.available_companions.is_empty() {
            None
        } else {
            Some(CompanionContribution {
                available: input.available_companions.clone(),
            })
        },
    });

    let cap_input = CapabilityResolverInput {
        owner_ctx: input.owner_ctx.clone(),
        contributions,
        mcp_candidates: McpCandidates {
            presets: input.available_presets.clone(),
        },
        mcp_runtime_context: Some(crate::mcp_preset::McpRuntimeBindingContext {
            vfs: Some(&effective_vfs),
            backend_anchor: None,
        }),
        capability_context: None,
        authority_state: input.authority_state.clone(),
    };
    let mut cap_output = CapabilityResolver::resolve_checked(&cap_input, platform)?;
    if let Some(slice_mode) = input.companion_slice_mode {
        cap_output = CapabilityResolver::apply_companion_slice(cap_output, slice_mode);
    }

    let mut mcp_servers = cap_output.tool.mcp_servers.clone();
    dedupe_runtime_mcp_servers(&mut mcp_servers);
    let capability_keys = cap_output.capability_keys();
    let kickoff_prompt = build_kickoff_prompt_fragment(input);

    Ok(ActivityActivation {
        capability_state: cap_output,
        mcp_servers,
        capability_keys,
        kickoff_prompt,
        lifecycle_mount,
        lifecycle_vfs,
        mount_directives,
    })
}

pub(super) async fn load_scoped_port_output_map(
    repo: &dyn InlineFileRepository,
    scope: &RuntimeNodeArtifactScope,
) -> BTreeMap<String, String> {
    let prefix = scope.path_prefix();
    repo.list_files(
        InlineFileOwnerKind::LifecycleRun,
        scope.run_id,
        "port_outputs",
    )
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|file| {
        let port_key = file.path.strip_prefix(&prefix)?.to_string();
        if port_key.is_empty() || port_key.contains('/') {
            return None;
        }
        let content = file.into_text_content()?;
        (!content.trim().is_empty()).then_some((port_key, content))
    })
    .collect()
}

pub(super) async fn project_companion_system_skill_to_activation(
    repos: &RepositorySet,
    project_id: Uuid,
    activation: &mut ActivityActivation,
) -> Result<(), String> {
    ensure_companion_system_skill_asset(repos, project_id)
        .await
        .map_err(|error| error.to_string())?;
    let mut skill_asset_keys = Vec::new();
    append_companion_system_skill_key(&mut skill_asset_keys);
    append_lifecycle_companion_system_projection(
        &mut activation.lifecycle_vfs,
        project_id,
        &skill_asset_keys,
    );
    if let Some(mount) = activation
        .lifecycle_vfs
        .mounts
        .iter()
        .find(|mount| {
            mount.id == agentdash_application_ports::lifecycle_surface_projection::LIFECYCLE_MOUNT_ID
                && mount.provider
                    == agentdash_application_ports::lifecycle_surface_projection::PROVIDER_LIFECYCLE_VFS
        })
        .cloned()
    {
        activation.lifecycle_mount = mount;
    }
    Ok(())
}

fn build_kickoff_prompt_fragment(input: &ActivityActivationInput<'_>) -> KickoffPromptFragment {
    let node_key = &input.active_activity.key;
    let desc = input.active_activity.description.trim();
    let node_title = if desc.is_empty() {
        format!("`{node_key}`")
    } else {
        format!("`{node_key}`({desc})")
    };
    let title_line = format!(
        "你正在执行 lifecycle `{}` 的 node {}。",
        input.lifecycle_key, node_title
    );

    KickoffPromptFragment {
        title_line,
        output_section: render_output_section(&input.active_activity.output_ports),
        input_section: render_input_section(
            &input.active_activity.input_ports,
            &input.ready_port_keys,
        ),
    }
}

fn render_output_section(ports: &[agentdash_domain::workflow::OutputPortDefinition]) -> String {
    if ports.is_empty() {
        return String::new();
    }
    let items = ports
        .iter()
        .map(|port| {
            format!(
                "- `lifecycle://artifacts/{}` — {}",
                port.key, port.description
            )
        })
        .collect::<Vec<_>>();
    format!(
        "\n\n## 必须交付的产出\n请将以下产出通过 `write_file` 写入对应路径:\n{}\n\n**所有 output port 写入完成后**再调用 `complete_lifecycle_node`。",
        items.join("\n")
    )
}

fn render_input_section(
    ports: &[agentdash_domain::workflow::InputPortDefinition],
    ready_port_keys: &BTreeSet<String>,
) -> String {
    if ports.is_empty() {
        return String::new();
    }
    let items = ports
        .iter()
        .map(|port| {
            let status = if ready_port_keys.contains(&port.key) {
                "已就绪"
            } else {
                "未就绪"
            };
            format!("- **{}**({}) — {}", port.key, port.description, status)
        })
        .collect::<Vec<_>>();
    format!(
        "\n\n## 输入上下文\n以下是来自前驱节点的产出,可通过 `read_file` 读取:\n{}",
        items.join("\n")
    )
}

fn dedupe_runtime_mcp_servers(servers: &mut Vec<agentdash_spi::RuntimeMcpServer>) {
    let mut seen = BTreeSet::<String>::new();
    servers.retain(|server| seen.insert(server.name.clone()));
}
