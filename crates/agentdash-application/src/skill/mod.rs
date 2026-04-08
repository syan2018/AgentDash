/// Skill 文件发现与解析模块
///
/// 负责从约定目录扫描 SKILL.md 文件，解析 frontmatter 元数据，
/// 验证格式后返回 `SkillRef` 列表。
///
/// 扫描顺序（优先级从高到低）：
/// 1. `{mount_root_ref}/.agents/skills/`（Codex convention）
/// 2. `{mount_root_ref}/skills/`
/// 3. `~/.agents/skills/`（用户全局）
///
/// 同名 skill 按发现顺序 first-wins，冲突记录为 `SkillDiagnostic`。
mod loader;

pub use loader::{LoadSkillsResult, load_skills_from_address_space};

use std::path::{Path, PathBuf};

use agentdash_spi::SkillRef;
use serde::Deserialize;

// ─── 常量 ──────────────────────────────────────────────────────────────────

const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;

// ─── Frontmatter ──────────────────────────────────────────────────────────

/// SKILL.md frontmatter 的反序列化结构
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SkillFrontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "disable-model-invocation", default)]
    pub disable_model_invocation: bool,
}

/// skill 诊断信息（验证错误或冲突警告，不阻断加载）
#[derive(Debug, Clone)]
pub struct SkillDiagnostic {
    pub name: String,
    pub message: String,
    pub file_path: PathBuf,
}

// ─── 解析 ──────────────────────────────────────────────────────────────────

/// 从 SKILL.md 文本中拆分 frontmatter 和正文
pub fn parse_skill_file(content: &str) -> (Option<SkillFrontmatter>, String) {
    let content = content.trim_start_matches('\u{feff}');
    if !content.starts_with("---") {
        return (None, content.to_string());
    }
    let after_open = &content[3..];
    if let Some(close_pos) = after_open.find("\n---") {
        let yaml_str = &after_open[..close_pos];
        let rest = &after_open[close_pos + 4..];
        let body = rest.strip_prefix('\n').unwrap_or(rest).to_string();
        let fm = serde_yaml::from_str(yaml_str).ok();
        (fm, body)
    } else {
        (None, content.to_string())
    }
}

/// 将 SKILL.md 文件解析为 SkillRef，验证失败时返回诊断列表
pub fn parse_skill_ref(skill_md_path: &Path) -> Result<SkillRef, Vec<SkillDiagnostic>> {
    let base_dir = skill_md_path
        .parent()
        .unwrap_or(skill_md_path)
        .to_path_buf();
    let parent_dir_name = base_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let content = match std::fs::read_to_string(skill_md_path) {
        Ok(c) => c,
        Err(err) => {
            return Err(vec![SkillDiagnostic {
                name: parent_dir_name,
                message: format!("读取 SKILL.md 失败: {err}"),
                file_path: skill_md_path.to_path_buf(),
            }]);
        }
    };

    let (fm, _body) = parse_skill_file(&content);
    let fm = fm.unwrap_or_default();
    let name = fm
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| parent_dir_name.clone());

    let mut errors: Vec<SkillDiagnostic> = Vec::new();
    for msg in validate_name(&name, &parent_dir_name) {
        errors.push(SkillDiagnostic {
            name: name.clone(),
            message: msg,
            file_path: skill_md_path.to_path_buf(),
        });
    }
    for msg in validate_description(fm.description.as_deref()) {
        errors.push(SkillDiagnostic {
            name: name.clone(),
            message: msg,
            file_path: skill_md_path.to_path_buf(),
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(SkillRef {
        name,
        description: fm.description.unwrap_or_default(),
        file_path: skill_md_path.to_path_buf(),
        base_dir,
        disable_model_invocation: fm.disable_model_invocation,
    })
}

/// 展开 skill 内容为 `<skill>` 块（用于 /skill:name 触发时注入对话）
pub fn expand_skill_block(skill: &SkillRef, args: Option<&str>) -> String {
    let body = std::fs::read_to_string(&skill.file_path)
        .map(|content| {
            let (_fm, body) = parse_skill_file(&content);
            body.trim().to_string()
        })
        .unwrap_or_default();

    let block = format!(
        "<skill name=\"{}\" location=\"{}\">\nReferences are relative to {}.\n\n{}\n</skill>",
        escape_xml(&skill.name),
        escape_xml(&skill.file_path.to_string_lossy()),
        skill.base_dir.display(),
        body,
    );
    match args {
        Some(a) if !a.trim().is_empty() => format!("{block}\n\n{a}"),
        _ => block,
    }
}

// ─── 验证 ──────────────────────────────────────────────────────────────────

fn validate_name(name: &str, parent_dir_name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if name != parent_dir_name {
        errors.push(format!(
            "name \"{name}\" 与父目录名 \"{parent_dir_name}\" 不一致"
        ));
    }
    if name.len() > MAX_NAME_LENGTH {
        errors.push(format!(
            "name 超过 {MAX_NAME_LENGTH} 字符（当前 {} 字符）",
            name.len()
        ));
    }
    if !name
        .chars()
        .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
    {
        errors.push("name 只能包含小写字母、数字和连字符".to_string());
    }
    if name.starts_with('-') || name.ends_with('-') {
        errors.push("name 不能以连字符开头或结尾".to_string());
    }
    if name.contains("--") {
        errors.push("name 不能包含连续连字符".to_string());
    }
    errors
}

fn validate_description(description: Option<&str>) -> Vec<String> {
    let mut errors = Vec::new();
    match description {
        None | Some("") => errors.push("description 为必填项".to_string()),
        Some(desc) if desc.trim().is_empty() => errors.push("description 不能为空白".to_string()),
        Some(desc) if desc.len() > MAX_DESCRIPTION_LENGTH => errors.push(format!(
            "description 超过 {MAX_DESCRIPTION_LENGTH} 字符（当前 {} 字符）",
            desc.len()
        )),
        _ => {}
    }
    errors
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ─── 测试 ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_basic() {
        let content = "---\nname: foo\ndescription: bar\n---\n# Body";
        let (fm, body) = parse_skill_file(content);
        let fm = fm.expect("should parse");
        assert_eq!(fm.name.as_deref(), Some("foo"));
        assert_eq!(fm.description.as_deref(), Some("bar"));
        assert!(!fm.disable_model_invocation);
        assert_eq!(body.trim(), "# Body");
    }

    #[test]
    fn parse_frontmatter_disable_model_invocation() {
        let content = "---\nname: foo\ndescription: bar\ndisable-model-invocation: true\n---\n";
        let (fm, _) = parse_skill_file(content);
        assert!(fm.unwrap().disable_model_invocation);
    }

    #[test]
    fn parse_frontmatter_missing() {
        let (fm, body) = parse_skill_file("# No frontmatter");
        assert!(fm.is_none());
        assert_eq!(body.trim(), "# No frontmatter");
    }

    #[test]
    fn validate_name_valid() {
        assert!(validate_name("code-review", "code-review").is_empty());
    }

    #[test]
    fn validate_name_dir_mismatch() {
        assert!(!validate_name("foo", "bar").is_empty());
    }

    #[test]
    fn validate_name_uppercase_rejected() {
        let errors = validate_name("FooBar", "FooBar");
        assert!(errors.iter().any(|e| e.contains("小写字母")));
    }

    #[test]
    fn validate_name_consecutive_hyphens() {
        let errors = validate_name("foo--bar", "foo--bar");
        assert!(errors.iter().any(|e| e.contains("连续连字符")));
    }
}
