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

use std::path::PathBuf;

use serde::Deserialize;

// ─── 常量 ──────────────────────────────────────────────────────────────────

const MAX_NAME_LENGTH: usize = 64;

// ─── Frontmatter ──────────────────────────────────────────────────────────

/// SKILL.md frontmatter 的反序列化结构
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct SkillFrontmatter {
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
pub(crate) fn parse_skill_file(content: &str) -> (Option<SkillFrontmatter>, String) {
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
}
