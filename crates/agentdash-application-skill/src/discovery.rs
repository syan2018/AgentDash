use agentdash_application_vfs::{
    DynamicMountFileDiscoveryRule, MountFileDiscoveryDiagnostic, VfsService,
    discover_dynamic_mount_files,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_spi::{
    AuthIdentity, SkillDiscoveryDiagnostic, SkillDiscoveryVfsFile, SkillDiscoveryVfsRule, Vfs,
};

pub use crate::skill::{
    LoadSkillsResult, SkillDiagnostic, load_skills_from_local_dirs, load_skills_from_vfs,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillVfsDiscoveryDiagnostic {
    pub rule_key: String,
    pub mount_id: String,
    pub path: String,
    pub message: String,
}

impl SkillVfsDiscoveryDiagnostic {
    pub fn into_skill_discovery_diagnostic(
        self,
        provider_key: impl Into<String>,
    ) -> SkillDiscoveryDiagnostic {
        SkillDiscoveryDiagnostic {
            provider_key: provider_key.into(),
            code: "vfs_discovery_scan".to_string(),
            message: self.message,
            local_name: None,
            file_path: Some(format!("{}://{}", self.mount_id, self.path)),
        }
    }
}

impl From<MountFileDiscoveryDiagnostic> for SkillVfsDiscoveryDiagnostic {
    fn from(diagnostic: MountFileDiscoveryDiagnostic) -> Self {
        Self {
            rule_key: diagnostic.rule_key,
            mount_id: diagnostic.mount_id,
            path: diagnostic.path,
            message: diagnostic.message,
        }
    }
}

/// 通过共享的 VFS mount 扫描器发现动态 skill provider 声明的文件。
///
/// mount policy / path normalization / read / list / size-limit / 空内容过滤 /
/// 诊断全部由 `agentdash-application-vfs::mount_file_discovery` 提供，这里只做
/// `SkillDiscoveryVfsRule -> DynamicMountFileDiscoveryRule` 与结果类型的 adapter 转换。
pub async fn discover_skill_vfs_files(
    service: &VfsService,
    vfs: &Vfs,
    rules: &[SkillDiscoveryVfsRule],
    identity: Option<&AuthIdentity>,
) -> (Vec<SkillDiscoveryVfsFile>, Vec<SkillVfsDiscoveryDiagnostic>) {
    let dynamic_rules = rules.iter().map(skill_rule_to_dynamic).collect::<Vec<_>>();
    let result = discover_dynamic_mount_files(service, vfs, &dynamic_rules, identity).await;

    let files = result
        .files
        .into_iter()
        .map(|file| SkillDiscoveryVfsFile {
            rule_key: file.rule_key,
            mount_id: file.mount_id,
            path: file.path,
            content: file.content,
        })
        .collect::<Vec<_>>();
    let diagnostics = result
        .diagnostics
        .into_iter()
        .map(SkillVfsDiscoveryDiagnostic::from)
        .collect::<Vec<_>>();

    diag!(
        Info,
        Subsystem::Skill,
        rule_count = rules.len(),
        file_count = files.len(),
        diagnostic_count = diagnostics.len(),
        "discovery: VFS 规则扫描完成"
    );
    (files, diagnostics)
}

fn skill_rule_to_dynamic(rule: &SkillDiscoveryVfsRule) -> DynamicMountFileDiscoveryRule {
    let key = rule.key.trim();
    DynamicMountFileDiscoveryRule {
        key: if key.is_empty() {
            "skill_discovery".to_string()
        } else {
            key.to_string()
        },
        file_names: rule.file_names.clone(),
        exact_paths: rule.exact_paths.clone(),
        scan_prefixes: rule.scan_prefixes.clone(),
        recursive: rule.recursive,
        max_depth: rule.max_depth,
        max_files: rule.max_files,
        max_size_bytes: rule.max_size_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_converts_to_skill_discovery_diagnostic() {
        let diagnostic = SkillVfsDiscoveryDiagnostic {
            rule_key: "skills".to_string(),
            mount_id: "workspace".to_string(),
            path: "skills/demo/SKILL.md".to_string(),
            message: "too large".to_string(),
        }
        .into_skill_discovery_diagnostic("provider");

        assert_eq!(diagnostic.provider_key, "provider");
        assert_eq!(diagnostic.code, "vfs_discovery_scan");
        assert_eq!(
            diagnostic.file_path.as_deref(),
            Some("workspace://skills/demo/SKILL.md")
        );
    }

    #[test]
    fn skill_rule_projects_all_fields_to_dynamic_rule() {
        let mut rule = SkillDiscoveryVfsRule::new("");
        rule.file_names = vec!["SKILL.md".to_string()];
        rule.exact_paths = vec!["skills/review/SKILL.md".to_string()];
        rule.scan_prefixes = vec!["skills".to_string()];
        rule.recursive = true;
        rule.max_depth = Some(3);
        rule.max_files = Some(10);
        rule.max_size_bytes = 4096;

        let dynamic = skill_rule_to_dynamic(&rule);

        assert_eq!(dynamic.key, "skill_discovery");
        assert_eq!(dynamic.file_names, vec!["SKILL.md".to_string()]);
        assert_eq!(dynamic.exact_paths, vec!["skills/review/SKILL.md".to_string()]);
        assert_eq!(dynamic.scan_prefixes, vec!["skills".to_string()]);
        assert!(dynamic.recursive);
        assert_eq!(dynamic.max_depth, Some(3));
        assert_eq!(dynamic.max_files, Some(10));
        assert_eq!(dynamic.max_size_bytes, 4096);
    }
}
