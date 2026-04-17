/// Skill 目录扫描与加载
///
/// 通过 VFS 的 relay service 扫描所有 mount 的约定 skill 目录，
/// 使用 `list()` 遍历子目录，`read_text()` 读取 SKILL.md。
use std::collections::HashMap;
use std::path::PathBuf;

use agentdash_spi::{Vfs, MountCapability, SkillRef};

use crate::vfs::types::ResourceRef;
use crate::vfs::{ListOptions, RelayVfsService};

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

    result
}

/// 通过 VFS service 从所有 mount 扫描 skill（主入口）
///
/// 对每个有 Read + List 能力的 mount，扫描 `.agents/skills/` 和 `skills/` 目录。
pub async fn load_skills_from_vfs(
    service: &RelayVfsService,
    vfs: &Vfs,
) -> LoadSkillsResult {
    let mut result = LoadSkillsResult::default();
    let mut name_map: HashMap<String, String> = HashMap::new(); // name → "mount_id://path"

    for mount in &vfs.mounts {
        let has_read = mount.capabilities.contains(&MountCapability::Read);
        let has_list = mount.capabilities.contains(&MountCapability::List);
        if !has_read || !has_list {
            continue;
        }

        for skill_dir in [".agents/skills", "skills"] {
            let skills = scan_mount_skill_dir(service, vfs, &mount.id, skill_dir).await;

            for (skill, diags) in skills {
                result.diagnostics.extend(diags);
                if let Some(skill) = skill {
                    let key = format!("{}://{}", mount.id, skill.file_path.display());
                    if let Some(existing) = name_map.get(&skill.name) {
                        result.diagnostics.push(SkillDiagnostic {
                            name: skill.name.clone(),
                            message: format!(
                                "skill \"{}\" 与 {} 冲突，忽略 {}",
                                skill.name, existing, key
                            ),
                            file_path: skill.file_path,
                        });
                    } else {
                        name_map.insert(skill.name.clone(), key);
                        result.skills.push(skill);
                    }
                }
            }
        }
    }

    result
}

/// 扫描指定 mount 的某个 skill 目录
///
/// 列出一级子目录，对每个含 SKILL.md 的子目录解析 frontmatter。
async fn scan_mount_skill_dir(
    service: &RelayVfsService,
    vfs: &Vfs,
    mount_id: &str,
    skill_dir: &str,
) -> Vec<(Option<SkillRef>, Vec<SkillDiagnostic>)> {
    let mut results = Vec::new();

    // 列出 skill 目录下的一级子目录
    let list_result = service
        .list(
            vfs,
            mount_id,
            ListOptions {
                path: skill_dir.to_string(),
                pattern: None,
                recursive: false,
            },
            None,
            None,
        )
        .await;

    let entries = match list_result {
        Ok(r) => r.entries,
        Err(_) => return results, // 目录不存在或不可读，静默跳过
    };

    for entry in entries {
        if !entry.is_dir {
            continue;
        }

        // 尝试读取 SKILL.md
        let skill_md_path = format!("{}/SKILL.md", entry.path);
        let target = ResourceRef {
            mount_id: mount_id.to_string(),
            path: skill_md_path.clone(),
        };
        let content = match service.read_text(vfs, &target, None, None).await {
            Ok(r) => r.content,
            Err(_) => continue, // 无 SKILL.md，跳过
        };

        // 解析 frontmatter
        let (fm, _body) = parse_skill_file(&content);
        let fm = fm.unwrap_or_default();

        let parent_dir_name = entry
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&entry.path)
            .to_string();

        let name = fm
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| parent_dir_name.clone());

        let mut diags = Vec::new();
        // 验证
        diags.extend(validate_and_collect(
            &name,
            &parent_dir_name,
            &fm,
            &skill_md_path,
        ));

        if !diags.is_empty() {
            results.push((None, diags));
            continue;
        }

        // 构建 SkillRef——file_path 使用 mount URI 格式
        let skill_ref = SkillRef {
            name,
            description: fm.description.unwrap_or_default(),
            file_path: PathBuf::from(format!("{mount_id}://{skill_md_path}")),
            base_dir: PathBuf::from(format!("{mount_id}://{}", entry.path)),
            disable_model_invocation: fm.disable_model_invocation,
        };
        results.push((Some(skill_ref), Vec::new()));
    }

    results
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
}
