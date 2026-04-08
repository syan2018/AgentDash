/// Skill 目录扫描与加载
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use agentdash_spi::{AddressSpace, MountCapability, SkillRef};

use super::{SkillDiagnostic, parse_skill_ref};

// ─── 公共 API ──────────────────────────────────────────────────────────────

/// 加载结果
#[derive(Debug, Default)]
pub struct LoadSkillsResult {
    pub skills: Vec<SkillRef>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

/// 计算当前会话的 skill 扫描目录列表（按优先级排序）
pub fn skill_scan_dirs(workspace_root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // 项目级（优先级最高）
    dirs.push(workspace_root.join(".agents").join("skills"));
    dirs.push(workspace_root.join("skills"));
    // 用户全局（跨平台 home dir）
    if let Some(home) = home_dir() {
        dirs.push(home.join(".agents").join("skills"));
    }
    dirs
}

/// 从工作区根目录 + address space mounts 加载 skill（完整入口）
///
/// 扫描顺序（优先级从高到低）：
/// 1. workspace_root 下的 `.agents/skills/` 和 `skills/`
/// 2. 用户全局 `~/.agents/skills/`
/// 3. address space 中可本地访问的 mount（含 Read + List 能力）
pub fn load_skills_for_workspace(
    workspace_root: &Path,
    address_space: Option<&AddressSpace>,
) -> LoadSkillsResult {
    let mut dirs = skill_scan_dirs(workspace_root);

    // 追加 address space 中可本地访问的 mount 路径
    if let Some(as_ref) = address_space {
        for mount in &as_ref.mounts {
            let has_read = mount.capabilities.contains(&MountCapability::Read);
            let has_list = mount.capabilities.contains(&MountCapability::List);
            if !has_read || !has_list {
                continue;
            }

            // 尝试将 mount root_ref 作为本地路径（跳过远程/虚拟 mount）
            let root = PathBuf::from(&mount.root_ref);
            if !root.is_absolute() || !root.exists() {
                tracing::trace!(
                    mount_id = %mount.id,
                    root_ref = %mount.root_ref,
                    "mount root_ref 不是本地可访问路径，跳过 skill 扫描"
                );
                continue;
            }

            // 避免与 workspace_root 已扫描的路径重复
            let agents_skills = root.join(".agents").join("skills");
            let skills = root.join("skills");
            if !dirs.contains(&agents_skills) {
                dirs.push(agents_skills);
            }
            if !dirs.contains(&skills) {
                dirs.push(skills);
            }
        }
    }

    load_skills(&dirs)
}

/// 从多个目录加载 skill，自动去重（同名 first-wins）
fn load_skills(scan_dirs: &[PathBuf]) -> LoadSkillsResult {
    let mut result = LoadSkillsResult::default();
    let mut name_map: HashMap<String, PathBuf> = HashMap::new();

    for dir in scan_dirs {
        if !dir.exists() {
            continue;
        }
        let (found, mut diags) = scan_skills_from_dir(dir);
        result.diagnostics.append(&mut diags);

        for skill in found {
            if let Some(existing_path) = name_map.get(&skill.name) {
                result.diagnostics.push(SkillDiagnostic {
                    name: skill.name.clone(),
                    message: format!(
                        "skill \"{}\" 与 {} 冲突，忽略 {}",
                        skill.name,
                        existing_path.display(),
                        skill.file_path.display()
                    ),
                    file_path: skill.file_path,
                });
            } else {
                name_map.insert(skill.name.clone(), skill.file_path.clone());
                result.skills.push(skill);
            }
        }
    }
    result
}

// ─── 目录扫描 ─────────────────────────────────────────────────────────────

/// 扫描单个目录，递归查找 SKILL.md
///
/// - 含 SKILL.md 的目录为 skill 根，不继续递归
/// - 跳过隐藏目录（以 `.` 开头）
fn scan_skills_from_dir(dir: &Path) -> (Vec<SkillRef>, Vec<SkillDiagnostic>) {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();
    collect_recursive(dir, true, &mut skills, &mut diagnostics);
    (skills, diagnostics)
}

fn collect_recursive(
    dir: &Path,
    is_root: bool,
    skills: &mut Vec<SkillRef>,
    diagnostics: &mut Vec<SkillDiagnostic>,
) {
    if !dir.is_dir() {
        return;
    }
    // 跳过隐藏子目录
    if !is_root {
        if let Some(name) = dir.file_name() {
            if name.to_string_lossy().starts_with('.') {
                return;
            }
        }
    }

    let skill_md = dir.join("SKILL.md");
    if skill_md.exists() {
        match parse_skill_ref(&skill_md) {
            Ok(skill) => skills.push(skill),
            Err(mut diags) => diagnostics.append(&mut diags),
        }
        return; // skill 根不继续递归
    }

    // 递归子目录
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            diagnostics.push(SkillDiagnostic {
                name: String::new(),
                message: format!("读取目录 {} 失败: {err}", dir.display()),
                file_path: dir.to_path_buf(),
            });
            return;
        }
    };
    let mut subdirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    subdirs.sort(); // 保证跨平台一致顺序
    for subdir in subdirs {
        collect_recursive(&subdir, false, skills, diagnostics);
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

// ─── 测试 ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_skill(dir: &Path, name: &str, desc: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: \"{desc}\"\n---\n\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn scan_single_skill() {
        let tmp = TempDir::new().unwrap();
        write_skill(tmp.path(), "code-review", "代码审查");
        let (skills, diags) = scan_skills_from_dir(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "code-review");
        assert!(diags.is_empty());
    }

    #[test]
    fn scan_multiple_skills() {
        let tmp = TempDir::new().unwrap();
        write_skill(tmp.path(), "code-review", "代码审查");
        write_skill(tmp.path(), "doc-gen", "文档生成");
        let (skills, _) = scan_skills_from_dir(tmp.path());
        assert_eq!(skills.len(), 2);
    }

    #[test]
    fn scan_skips_hidden_dirs() {
        let tmp = TempDir::new().unwrap();
        write_skill(tmp.path(), "visible", "可见");
        let hidden = tmp.path().join(".hidden");
        fs::create_dir_all(&hidden).unwrap();
        write_skill(&hidden, "hidden", "隐藏");
        let (skills, _) = scan_skills_from_dir(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "visible");
    }

    #[test]
    fn skill_root_stops_recursion() {
        let tmp = TempDir::new().unwrap();
        let outer = tmp.path().join("outer");
        fs::create_dir_all(&outer).unwrap();
        fs::write(
            outer.join("SKILL.md"),
            "---\nname: outer\ndescription: \"outer\"\n---\n",
        )
        .unwrap();
        let inner = outer.join("inner");
        fs::create_dir_all(&inner).unwrap();
        fs::write(
            inner.join("SKILL.md"),
            "---\nname: inner\ndescription: \"inner\"\n---\n",
        )
        .unwrap();
        let (skills, _) = scan_skills_from_dir(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "outer");
    }

    #[test]
    fn dedup_first_wins() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        write_skill(tmp1.path(), "foo", "first");
        write_skill(tmp2.path(), "foo", "second");
        let result = load_skills(&[tmp1.path().to_path_buf(), tmp2.path().to_path_buf()]);
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].description, "first");
        assert_eq!(result.diagnostics.len(), 1);
    }

    #[test]
    fn load_empty_workspace() {
        let tmp = TempDir::new().unwrap();
        let result = load_skills_for_workspace(tmp.path(), None);
        assert!(result.skills.is_empty());
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn address_space_mount_adds_scan_dirs() {
        use agentdash_spi::Mount;

        let tmp_ws = TempDir::new().unwrap();
        let tmp_mount = TempDir::new().unwrap();
        // 在 mount root 的 skills/ 下放一个 skill
        write_skill(&tmp_mount.path().join("skills"), "mount-skill", "来自 mount");

        let as_ref = AddressSpace {
            mounts: vec![Mount {
                id: "extra".to_string(),
                provider: "relay_fs".to_string(),
                backend_id: "local".to_string(),
                root_ref: tmp_mount.path().to_string_lossy().to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "extra mount".to_string(),
                metadata: serde_json::Value::Null,
            }],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
        };
        let result = load_skills_for_workspace(tmp_ws.path(), Some(&as_ref));
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].name, "mount-skill");
    }
}
