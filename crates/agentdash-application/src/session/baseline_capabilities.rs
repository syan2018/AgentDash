use agentdash_spi::context::capability::{
    SessionBaselineCapabilities, SkillCapabilityEntry, SkillEntry, SkillProviderCluster,
};
use agentdash_spi::platform::skill::SkillRef;
use agentdash_spi::{SkillContextExposure, SkillDiscoveryDiagnostic, skill_capability_key};

pub const WORKSPACE_SKILL_PROVIDER_KEY: &str = "workspace";
pub const INTEGRATION_STATIC_SKILL_PROVIDER_KEY: &str = "integration-static";

/// 从已发现 skills 构建统一的 session baseline capabilities。
///
/// Companion agents 已迁移到 `CapabilityState.companion` 维度（由 Resolver 直接产出），
/// 不再通过 hook snapshot markdown 解析。
pub fn build_session_baseline_capabilities(skills: &[SkillRef]) -> SessionBaselineCapabilities {
    let clusters = skills_to_provider_clusters(
        WORKSPACE_SKILL_PROVIDER_KEY,
        "Workspace Skills",
        Some("Skills discovered from the active workspace.".to_string()),
        Some("当前 workspace 中声明的 skills。".to_string()),
        None,
        skills,
    );
    build_session_baseline_capabilities_from_clusters(clusters, Vec::new())
}

pub fn build_session_baseline_capabilities_from_clusters(
    skill_clusters: Vec<SkillProviderCluster>,
    skill_diagnostics: Vec<SkillDiscoveryDiagnostic>,
) -> SessionBaselineCapabilities {
    let skills = skill_clusters
        .iter()
        .flat_map(|cluster| cluster.default_exposed_skills.iter())
        .filter(|skill| skill.exposure.is_default_exposed())
        .map(SkillEntry::from_capability_entry)
        .collect();

    SessionBaselineCapabilities {
        skills,
        skill_clusters,
        skill_diagnostics,
    }
}

pub fn skills_to_provider_clusters(
    provider_key: &str,
    display_name: &str,
    model_summary: Option<String>,
    ui_summary: Option<String>,
    inventory_hint: Option<String>,
    skills: &[SkillRef],
) -> Vec<SkillProviderCluster> {
    if skills.is_empty() {
        return Vec::new();
    }

    vec![SkillProviderCluster {
        provider_key: provider_key.to_string(),
        display_name: display_name.to_string(),
        model_summary,
        ui_summary,
        inventory_hint,
        inventory_count: Some(skills.len()),
        default_exposed_skills: skills
            .iter()
            .map(|skill| skill_ref_to_capability_entry(provider_key, skill))
            .collect(),
    }]
}

pub fn skill_ref_to_capability_entry(provider_key: &str, skill: &SkillRef) -> SkillCapabilityEntry {
    SkillCapabilityEntry {
        capability_key: skill_capability_key(provider_key, &skill.name),
        provider_key: provider_key.to_string(),
        local_name: skill.name.clone(),
        display_name: None,
        description: skill.description.clone(),
        file_path: skill.file_path.to_string_lossy().to_string(),
        base_dir: Some(skill.base_dir.to_string_lossy().to_string()),
        exposure: SkillContextExposure::DefaultExposed,
        disable_model_invocation: skill.disable_model_invocation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_capabilities_from_skills() {
        let skills = vec![SkillRef {
            name: "test-skill".to_string(),
            description: "A test skill".to_string(),
            file_path: "/workspace/skills/test/SKILL.md".into(),
            base_dir: "/workspace/skills/test".into(),
            disable_model_invocation: false,
        }];

        let caps = build_session_baseline_capabilities(&skills);
        assert_eq!(caps.skills.len(), 1);
        assert_eq!(caps.skills[0].name, "test-skill");
        assert_eq!(caps.skills[0].capability_key, "workspace/test-skill");
        assert_eq!(caps.skill_clusters.len(), 1);
        assert!(!caps.is_empty());
    }

    #[test]
    fn build_capabilities_without_skills() {
        let caps = build_session_baseline_capabilities(&[]);
        assert!(caps.is_empty());
    }

    #[test]
    fn explicit_only_skill_stays_out_of_default_flat_surface() {
        let cluster = SkillProviderCluster {
            provider_key: "provider-a".to_string(),
            display_name: "Provider A".to_string(),
            default_exposed_skills: vec![SkillCapabilityEntry {
                exposure: SkillContextExposure::ExplicitOnly,
                ..SkillCapabilityEntry::new("provider-a", "manual", "desc", "/manual/SKILL.md")
            }],
            ..Default::default()
        };

        let caps = build_session_baseline_capabilities_from_clusters(vec![cluster], Vec::new());

        assert!(caps.skills.is_empty());
        assert_eq!(caps.skill_clusters.len(), 1);
        assert_eq!(
            caps.skill_clusters[0].default_exposed_skills[0].exposure,
            SkillContextExposure::ExplicitOnly
        );
    }
}
