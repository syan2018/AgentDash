use std::path::Path;

/// 从工作目录扫描 SKILL.md 文件，生成 slash command 列表。
///
/// 遍历本地 `.agents/skills/` 和 `skills/` 目录的一级子目录，
/// 解析 SKILL.md 的 frontmatter 提取 name 和 description。
pub(crate) fn discover_skill_slash_commands(mount_root: &Path) -> Vec<serde_json::Value> {
    let mut commands = Vec::new();
    let scan_dirs = [
        mount_root.join(".agents").join("skills"),
        mount_root.join("skills"),
    ];

    for dir in &scan_dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            if !entry.file_type().map_or(false, |ft| ft.is_dir()) {
                continue;
            }

            let skill_md = entry.path().join("SKILL.md");
            let content = match std::fs::read_to_string(&skill_md) {
                Ok(content) => content,
                Err(_) => continue,
            };

            let fm = parse_skill_frontmatter(&content).unwrap_or_default();
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let name = fm
                .name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| dir_name.clone());
            let description = fm.description.unwrap_or_default();

            commands.push(serde_json::json!({
                "name": name,
                "description": description,
            }));
        }
    }

    commands
}

#[derive(Default, serde::Deserialize)]
struct SkillSlashCommandFrontmatter {
    name: Option<String>,
    description: Option<String>,
}

/// 解析 SKILL.md frontmatter
fn parse_skill_frontmatter(content: &str) -> Option<SkillSlashCommandFrontmatter> {
    let content = content.trim_start_matches('\u{feff}');
    if !content.starts_with("---") {
        return None;
    }
    let after_open = &content[3..];
    let close_pos = after_open.find("\n---")?;
    let yaml_str = &after_open[..close_pos];
    serde_yaml::from_str(yaml_str).ok()
}
