use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_spi::context::capability::{SessionBaselineCapabilities, SkillEntry};
use agentdash_spi::{DiscoveredGuideline, SessionMcpServer, Vfs};

use crate::context::mount_file_discovery::{BUILTIN_GUIDELINE_RULES, discover_mount_files};
use crate::vfs::VfsService;

use super::baseline_capabilities::build_session_baseline_capabilities;
use super::types::CapabilityState;

#[derive(Clone, Copy)]
pub struct SessionCapabilityProjectionInput<'a> {
    pub vfs_service: Option<&'a VfsService>,
    pub active_vfs: Option<&'a Vfs>,
    pub extra_skill_dirs: &'a [PathBuf],
    pub diagnostics_label: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct SessionCapabilityProjection {
    pub session_capabilities: SessionBaselineCapabilities,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
}

pub async fn derive_session_capability_projection(
    input: SessionCapabilityProjectionInput<'_>,
) -> SessionCapabilityProjection {
    let session_capabilities = derive_session_skill_baseline(input)
        .await
        .unwrap_or_default();
    let discovered_guidelines = match (input.vfs_service, input.active_vfs) {
        (Some(vfs_service), Some(active_vfs)) => {
            derive_session_guidelines(vfs_service, active_vfs, input.diagnostics_label).await
        }
        _ => Vec::new(),
    };

    SessionCapabilityProjection {
        session_capabilities,
        discovered_guidelines,
    }
}

pub async fn derive_session_skill_baseline(
    input: SessionCapabilityProjectionInput<'_>,
) -> Option<SessionBaselineCapabilities> {
    let mut skills =
        if let (Some(vfs_service), Some(active_vfs)) = (input.vfs_service, input.active_vfs) {
            let result = crate::skill::load_skills_from_vfs(vfs_service, active_vfs).await;
            log_skill_diagnostics(input.diagnostics_label, "vfs", &result.diagnostics);
            result.skills
        } else {
            Vec::new()
        };

    if !input.extra_skill_dirs.is_empty() {
        let existing_names: HashMap<String, String> = skills
            .iter()
            .map(|skill| {
                (
                    skill.name.clone(),
                    skill.file_path.to_string_lossy().to_string(),
                )
            })
            .collect();
        let result =
            crate::skill::load_skills_from_local_dirs(input.extra_skill_dirs, &existing_names);
        log_skill_diagnostics(input.diagnostics_label, "plugin", &result.diagnostics);
        skills.extend(result.skills);
    }

    if input.vfs_service.is_none() && input.extra_skill_dirs.is_empty() {
        return None;
    }

    Some(build_session_baseline_capabilities(&skills))
}

pub async fn derive_session_guidelines(
    vfs_service: &VfsService,
    active_vfs: &Vfs,
    diagnostics_label: &'static str,
) -> Vec<DiscoveredGuideline> {
    let guideline_result =
        discover_mount_files(vfs_service, active_vfs, BUILTIN_GUIDELINE_RULES).await;
    for diag in &guideline_result.diagnostics {
        tracing::warn!(
            label = diagnostics_label,
            rule_key = %diag.rule_key,
            mount_id = %diag.mount_id,
            path = %diag.path,
            "guideline 发现诊断: {}",
            diag.message
        );
    }

    guideline_result
        .files
        .into_iter()
        .map(|file| DiscoveredGuideline {
            file_name: file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&file.path)
                .to_string(),
            mount_id: file.mount_id,
            path: file.path,
            content: file.content,
        })
        .collect()
}

pub fn normalize_capability_state_dimensions(
    state: &mut CapabilityState,
    active_vfs: Option<Vfs>,
    mcp_servers: Vec<SessionMcpServer>,
    session_capabilities: &SessionBaselineCapabilities,
) {
    state.vfs.active = active_vfs;
    state.tool.mcp_servers = mcp_servers;
    state.skill.skills = session_capabilities.skills.clone();
}

pub fn merge_live_vfs_skill_entries(
    existing: &[SkillEntry],
    refreshed_vfs_skills: Vec<SkillEntry>,
) -> Vec<SkillEntry> {
    let mut merged = refreshed_vfs_skills;
    for skill in existing {
        if skill.file_path.contains("://") {
            continue;
        }
        if !merged.iter().any(|item| item.name == skill.name) {
            merged.push(skill.clone());
        }
    }
    merged
}

fn log_skill_diagnostics(
    diagnostics_label: &'static str,
    source: &'static str,
    diagnostics: &[crate::skill::SkillDiagnostic],
) {
    for diag in diagnostics {
        tracing::warn!(
            label = diagnostics_label,
            source,
            skill_name = %diag.name,
            path = %diag.file_path.display(),
            "skill 诊断: {}",
            diag.message
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str, file_path: &str) -> SkillEntry {
        SkillEntry {
            name: name.to_string(),
            description: String::new(),
            file_path: file_path.to_string(),
            disable_model_invocation: false,
        }
    }

    #[test]
    fn live_vfs_skill_merge_replaces_uri_skills_and_preserves_local_skills() {
        let existing = vec![
            skill("old-vfs", "main://skills/old-vfs/SKILL.md"),
            skill("plugin-skill", "/plugins/plugin-skill/SKILL.md"),
        ];
        let refreshed = vec![skill("new-vfs", "cvs-demo://skills/new-vfs/SKILL.md")];

        let merged = merge_live_vfs_skill_entries(&existing, refreshed);

        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|item| item.name == "new-vfs"));
        assert!(merged.iter().any(|item| item.name == "plugin-skill"));
        assert!(!merged.iter().any(|item| item.name == "old-vfs"));
    }
}
