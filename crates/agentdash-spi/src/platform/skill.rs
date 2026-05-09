/// Skill 引用 — 跨层共享的 skill 元数据值对象
///
/// 由 Application 层扫描 SKILL.md 文件后构建，
/// 再由 session 的 `skill_surface` ContextFrame 注入给模型按需加载。
use std::path::PathBuf;

/// 已发现并验证通过的 skill 引用（仅元数据，不含正文）
#[derive(Debug, Clone)]
pub struct SkillRef {
    /// skill 名称（小写字母/数字/连字符，最多 64 字符）
    pub name: String,
    /// skill 一行描述（最多 1024 字符）
    pub description: String,
    /// SKILL.md 文件的绝对路径
    pub file_path: PathBuf,
    /// skill 所在目录（相对路径解析基准，即 SKILL.md 的父目录）
    pub base_dir: PathBuf,
    /// 为 true 时不出现在 `skill_surface` 的模型可见 skill 列表，
    /// 仅允许用户通过 /skill:name 显式触发
    pub disable_model_invocation: bool,
}
