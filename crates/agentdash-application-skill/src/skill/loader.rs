/// Skill 目录扫描与加载
///
/// 通过 VFS 的 relay service 扫描所有 mount 的约定 skill 目录，
/// 使用 `list()` 遍历子目录，`read_text()` 读取 SKILL.md。
use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_application_vfs::{BUILTIN_SKILL_RULES, VfsService, discover_mount_files};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_platform_spi::{AuthIdentity, SkillRef, Vfs};

use super::{MAX_NAME_LENGTH, SkillDiagnostic, SkillFrontmatter, parse_skill_file};

// ─── 公共 API ──────────────────────────────────────────────────────────────

/// 加载结果
#[derive(Debug, Default)]
pub struct LoadSkillsResult {
    pub skills: Vec<SkillRef>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

/// 扫描插件提供的本地文件系统目录中的 skill
///
/// 对每个目录，遍历一级子目录并查找 SKILL.md，解析规则与 mount 扫描一致。
/// 不经过 VFS mount 系统，直接使用 `std::fs`。
pub fn load_skills_from_local_dirs(
    dirs: &[PathBuf],
    existing_names: &HashMap<String, String>,
) -> LoadSkillsResult {
    let mut result = LoadSkillsResult::default();
    let mut name_map: HashMap<String, String> = existing_names.clone();

    for dir in dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue, // 目录不存在或不可读，静默跳过
        };

        for entry in entries.filter_map(|e| e.ok()) {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }

            let skill_md = entry.path().join("SKILL.md");
            let content = match std::fs::read_to_string(&skill_md) {
                Ok(c) => c,
                Err(_) => continue, // 无 SKILL.md，跳过
            };

            let (fm, _body) = parse_skill_file(&content);
            let fm = fm.unwrap_or_default();

            let parent_dir_name = entry.file_name().to_string_lossy().to_string();

            let name = fm
                .name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| parent_dir_name.clone());

            let mut diags = Vec::new();
            diags.extend(validate_and_collect(
                &name,
                &parent_dir_name,
                &fm,
                &skill_md.to_string_lossy(),
            ));

            if !diags.is_empty() {
                result.diagnostics.extend(diags);
                continue;
            }

            let key = skill_md.to_string_lossy().to_string();
            if let Some(existing) = name_map.get(&name) {
                result.diagnostics.push(SkillDiagnostic {
                    name: name.clone(),
                    message: format!(
                        "skill \"{}\" 与 {} 冲突（plugin 路径），忽略 {}",
                        name, existing, key
                    ),
                    file_path: skill_md,
                });
            } else {
                name_map.insert(name.clone(), key);
                result.skills.push(SkillRef {
                    name,
                    description: fm.description.unwrap_or_default(),
                    file_path: skill_md,
                    base_dir: entry.path(),
                    disable_model_invocation: fm.disable_model_invocation,
                });
            }
        }
    }

    diag!(
        Info,
        Subsystem::Skill,
        dir_count = dirs.len(),
        skill_count = result.skills.len(),
        diagnostic_count = result.diagnostics.len(),
        "discovery: 本地目录 skill 加载完成"
    );
    result
}

/// 通过 VFS service 从所有 mount 扫描 skill（主入口）
///
/// 使用通用 `discover_mount_files` 底层机制发现 SKILL.md，
/// 再对每个文件做 frontmatter 解析 + 验证。
pub async fn load_skills_from_vfs(
    service: &VfsService,
    vfs: &Vfs,
    identity: Option<&AuthIdentity>,
) -> LoadSkillsResult {
    let mut result = LoadSkillsResult::default();
    let mut name_map: HashMap<String, String> = HashMap::new();

    for file in discover_builtin_skill_files(service, vfs, identity).await {
        let (fm, _body) = parse_skill_file(&file.content);
        let fm = fm.unwrap_or_default();

        let parent_dir_name = file
            .path
            .rsplit('/')
            .nth(1) // SKILL.md 的父目录名
            .unwrap_or(&file.path)
            .rsplit('/')
            .next()
            .unwrap_or(&file.path)
            .to_string();

        let name = fm
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| parent_dir_name.clone());

        let skill_md_path = file.path.clone();
        let mut diags = Vec::new();
        diags.extend(validate_and_collect(
            &name,
            &parent_dir_name,
            &fm,
            &skill_md_path,
        ));

        if !diags.is_empty() {
            result.diagnostics.extend(diags);
            continue;
        }

        let key = format!("{}://{}", file.mount_id, skill_md_path);
        if let Some(existing) = name_map.get(&name) {
            result.diagnostics.push(SkillDiagnostic {
                name: name.clone(),
                message: format!("skill \"{}\" 与 {} 冲突，忽略 {}", name, existing, key),
                file_path: PathBuf::from(&key),
            });
        } else {
            let parent_path = file.path.rsplit_once('/').map(|(p, _)| p).unwrap_or(".");
            name_map.insert(name.clone(), key);
            result.skills.push(SkillRef {
                name,
                description: fm.description.unwrap_or_default(),
                file_path: PathBuf::from(format!("{}://{}", file.mount_id, skill_md_path)),
                base_dir: PathBuf::from(format!("{}://{}", file.mount_id, parent_path)),
                disable_model_invocation: fm.disable_model_invocation,
            });
        }
    }

    diag!(
        Info,
        Subsystem::Skill,
        skill_count = result.skills.len(),
        diagnostic_count = result.diagnostics.len(),
        "discovery: VFS skill 加载完成"
    );
    result
}

#[derive(Debug)]
struct DiscoveredSkillFile {
    mount_id: String,
    path: String,
    content: String,
}

/// 复用共享 mount 扫描器发现 builtin SKILL.md。
///
/// mount policy / list / read / size-limit / 空内容过滤都由
/// `agentdash-application-vfs::discover_mount_files` + `BUILTIN_SKILL_RULES` 提供，
/// skill crate 只做 `DiscoveredMountFile -> DiscoveredSkillFile` 的 adapter 转换。
async fn discover_builtin_skill_files(
    service: &VfsService,
    vfs: &Vfs,
    identity: Option<&AuthIdentity>,
) -> Vec<DiscoveredSkillFile> {
    discover_mount_files(service, vfs, BUILTIN_SKILL_RULES, identity)
        .await
        .files
        .into_iter()
        .map(|file| DiscoveredSkillFile {
            mount_id: file.mount_id,
            path: file.path,
            content: file.content,
        })
        .collect()
}

fn validate_and_collect(
    name: &str,
    parent_dir_name: &str,
    fm: &SkillFrontmatter,
    path: &str,
) -> Vec<SkillDiagnostic> {
    let mut diags = Vec::new();

    if name != parent_dir_name {
        diags.push(SkillDiagnostic {
            name: name.to_string(),
            message: format!("name \"{name}\" 与父目录名 \"{parent_dir_name}\" 不一致"),
            file_path: PathBuf::from(path),
        });
    }
    if name.len() > MAX_NAME_LENGTH {
        diags.push(SkillDiagnostic {
            name: name.to_string(),
            message: format!(
                "name 超过 {MAX_NAME_LENGTH} 字符（当前 {} 字符）",
                name.len()
            ),
            file_path: PathBuf::from(path),
        });
    }
    if !name
        .chars()
        .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
    {
        diags.push(SkillDiagnostic {
            name: name.to_string(),
            message: "name 只能包含小写字母、数字和连字符".to_string(),
            file_path: PathBuf::from(path),
        });
    }
    match fm.description.as_deref() {
        None | Some("") => diags.push(SkillDiagnostic {
            name: name.to_string(),
            message: "description 为必填项".to_string(),
            file_path: PathBuf::from(path),
        }),
        _ => {}
    }

    diags
}

// ─── 测试 ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_application_vfs::{
        ListOptions, ListResult, MountError, MountOperationContext, MountProvider,
        MountProviderRegistry, PROVIDER_INLINE_FS, ReadResult, RuntimeFileEntry, SearchQuery,
        SearchResult,
    };
    use agentdash_platform_spi::{Mount, MountCapability};
    use tokio::sync::Mutex;

    use super::*;

    #[test]
    fn validate_catches_bad_name() {
        let fm = SkillFrontmatter {
            name: Some("Bad-Name".to_string()),
            description: Some("desc".to_string()),
            disable_model_invocation: false,
        };
        let diags = validate_and_collect("Bad-Name", "Bad-Name", &fm, "test.md");
        assert!(diags.iter().any(|d| d.message.contains("小写字母")));
    }

    #[test]
    fn validate_catches_missing_description() {
        let fm = SkillFrontmatter {
            name: Some("foo".to_string()),
            description: None,
            disable_model_invocation: false,
        };
        let diags = validate_and_collect("foo", "foo", &fm, "test.md");
        assert!(diags.iter().any(|d| d.message.contains("description")));
    }

    struct IdentityCaptureProvider {
        calls: Mutex<Vec<(String, Option<String>)>>,
    }

    impl IdentityCaptureProvider {
        async fn captured_calls(&self) -> Vec<(String, Option<String>)> {
            self.calls.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl MountProvider for IdentityCaptureProvider {
        fn provider_id(&self) -> &str {
            PROVIDER_INLINE_FS
        }

        fn supported_capabilities(&self) -> Vec<&str> {
            vec!["read", "list"]
        }

        async fn read_text(
            &self,
            _mount: &Mount,
            path: &str,
            ctx: &MountOperationContext,
        ) -> Result<ReadResult, MountError> {
            self.calls.lock().await.push((
                "read_text".to_string(),
                ctx.identity
                    .as_ref()
                    .map(|identity| identity.user_id.clone()),
            ));
            Ok(ReadResult::new(
                path,
                "---\nname: demo\ndescription: Demo skill\n---\n",
            ))
        }

        async fn write_text(
            &self,
            _mount: &Mount,
            _path: &str,
            _content: &str,
            _ctx: &MountOperationContext,
        ) -> Result<(), MountError> {
            Err(MountError::NotSupported("identity test".to_string()))
        }

        async fn list(
            &self,
            _mount: &Mount,
            options: &ListOptions,
            ctx: &MountOperationContext,
        ) -> Result<ListResult, MountError> {
            self.calls.lock().await.push((
                format!("list:{}", options.path),
                ctx.identity
                    .as_ref()
                    .map(|identity| identity.user_id.clone()),
            ));
            let entries = match options.path.as_str() {
                ".agents/skills" => Vec::new(),
                "skills" => vec![RuntimeFileEntry::dir("skills/demo")],
                _ => Vec::new(),
            };
            Ok(ListResult { entries })
        }

        async fn search_text(
            &self,
            _mount: &Mount,
            _query: &SearchQuery,
            _ctx: &MountOperationContext,
        ) -> Result<SearchResult, MountError> {
            Err(MountError::NotSupported("identity test".to_string()))
        }
    }

    #[tokio::test]
    async fn builtin_vfs_skill_discovery_passes_identity_to_list_and_read() {
        let provider = Arc::new(IdentityCaptureProvider {
            calls: Mutex::new(Vec::new()),
        });
        let mut registry = MountProviderRegistry::new();
        registry.register(provider.clone());
        let service = VfsService::new(Arc::new(registry));
        let vfs = Vfs {
            mounts: vec![Mount {
                id: "main".to_string(),
                provider: PROVIDER_INLINE_FS.to_string(),
                backend_id: "backend".to_string(),
                root_ref: "inline://root".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "Main".to_string(),
                metadata: serde_json::json!({}),
            }],
            default_mount_id: Some("main".to_string()),
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        };
        let identity = AuthIdentity::system_routine("builtin-skill-identity");

        let result = load_skills_from_vfs(&service, &vfs, Some(&identity)).await;

        assert_eq!(result.skills.len(), 1);
        assert_eq!(
            provider.captured_calls().await,
            vec![
                (
                    "list:.agents/skills".to_string(),
                    Some(identity.user_id.clone())
                ),
                ("list:skills".to_string(), Some(identity.user_id.clone())),
                ("read_text".to_string(), Some(identity.user_id.clone())),
            ]
        );
    }
}
