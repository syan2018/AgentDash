use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// provider-scoped skill identity。
///
/// `local_name` 只在单个 provider 内唯一；跨 provider 同名 skill 通过
/// `capability_key` 区分。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkillCapabilityId {
    pub provider_key: String,
    pub local_name: String,
}

impl SkillCapabilityId {
    pub fn new(provider_key: impl Into<String>, local_name: impl Into<String>) -> Self {
        Self {
            provider_key: provider_key.into(),
            local_name: local_name.into(),
        }
    }

    pub fn capability_key(&self) -> String {
        skill_capability_key(&self.provider_key, &self.local_name)
    }
}

pub fn skill_capability_key(provider_key: &str, local_name: &str) -> String {
    format!("{provider_key}/{local_name}")
}

/// Skill 默认上下文暴露策略。
///
/// 这不是权限系统：`ExplicitOnly` 只表示不默认序列化进模型上下文，
/// Agent 仍可通过显式路径、目录探索或 provider 工具使用该 skill。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillContextExposure {
    #[default]
    DefaultExposed,
    ExplicitOnly,
}

impl SkillContextExposure {
    pub fn is_default_exposed(self) -> bool {
        matches!(self, Self::DefaultExposed)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillDiscoveryOwnerKind {
    #[default]
    Unknown,
    Project,
    Story,
    Task,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkillDiscoveryUserContext {
    pub user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Dynamic skill provider 声明的 VFS 文件发现规则。
///
/// 规则只描述“在允许自动发现的 mount 中扫描什么”，不扩大 mount 自身的
/// discovery 权限；宿主仍必须先按 mount metadata / provider cost policy 决定
/// 是否允许扫描该 mount。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkillDiscoveryVfsRule {
    pub key: String,
    #[serde(default)]
    pub file_names: Vec<String>,
    #[serde(default)]
    pub exact_paths: Vec<String>,
    #[serde(default)]
    pub scan_prefixes: Vec<String>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_files: Option<usize>,
    pub max_size_bytes: u64,
}

impl SkillDiscoveryVfsRule {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            file_names: Vec::new(),
            exact_paths: Vec::new(),
            scan_prefixes: Vec::new(),
            recursive: false,
            max_depth: None,
            max_files: None,
            max_size_bytes: 64 * 1024,
        }
    }
}

/// 宿主通过 VFS 扫描后交给 dynamic skill provider 的文件内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkillDiscoveryVfsFile {
    pub rule_key: String,
    pub mount_id: String,
    /// 相对 mount 根的规范化路径。
    pub path: String,
    pub content: String,
}

/// Session 构建阶段传递给动态 skill provider 的通用上下文。
///
/// 公开主仓只传递抽象事实；具体目录推导、组织策略和默认暴露策略由 provider
/// 自己实现。需要访问 workspace 文件时，provider 应声明 VFS discovery rules，
/// 由宿主完成 mount 受控读取后再调用 `discover_from_vfs`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkillDiscoveryContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_identity_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_identity_payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_binding_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_facts: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub owner_kind: SkillDiscoveryOwnerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<SkillDiscoveryUserContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DiscoveredSkill {
    pub local_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub description: String,
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<String>,
    #[serde(default)]
    pub exposure: SkillContextExposure,
    #[serde(default)]
    pub disable_model_invocation: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkillDiscoveryCluster {
    pub provider_key: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_count: Option<usize>,
    #[serde(default)]
    pub skills: Vec<DiscoveredSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkillDiscoveryDiagnostic {
    pub provider_key: String,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SkillDiscoveryOutput {
    #[serde(default)]
    pub clusters: Vec<SkillDiscoveryCluster>,
    #[serde(default)]
    pub diagnostics: Vec<SkillDiscoveryDiagnostic>,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillDiscoveryError {
    #[error("skill discovery provider `{provider_key}` failed: {message}")]
    ProviderFailed {
        provider_key: String,
        message: String,
    },
}

#[async_trait]
pub trait SkillDiscoveryProvider: Send + Sync {
    fn provider_key(&self) -> &str;

    /// 声明 provider 需要宿主通过 active VFS 扫描的文件。
    ///
    /// 返回空列表表示 provider 不声明 VFS-first 文件规则。返回非空列表时，
    /// 宿主应先通过 VFS 扫描文件，再调用 `discover_from_vfs`。
    fn vfs_discovery_rules(&self) -> Vec<SkillDiscoveryVfsRule> {
        Vec::new()
    }

    /// VFS-first discovery 入口。
    ///
    /// 默认实现调用 `discover(context)`。声明了 VFS rules 的 provider 应覆盖此方法。
    async fn discover_from_vfs(
        &self,
        context: SkillDiscoveryContext,
        _files: Vec<SkillDiscoveryVfsFile>,
    ) -> Result<SkillDiscoveryOutput, SkillDiscoveryError> {
        self.discover(context).await
    }

    async fn discover(
        &self,
        context: SkillDiscoveryContext,
    ) -> Result<SkillDiscoveryOutput, SkillDiscoveryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_scoped_skill_capability_key_uses_provider_and_local_name() {
        let id = SkillCapabilityId::new("workspace", "config-edit");
        assert_eq!(id.capability_key(), "workspace/config-edit");
        assert_eq!(
            skill_capability_key("copilot", "config-edit"),
            "copilot/config-edit"
        );
    }

    #[test]
    fn explicit_only_is_context_exposure_not_unavailability() {
        assert!(SkillContextExposure::DefaultExposed.is_default_exposed());
        assert!(!SkillContextExposure::ExplicitOnly.is_default_exposed());
    }

    #[test]
    fn vfs_rule_defaults_are_bounded() {
        let rule = SkillDiscoveryVfsRule::new("skills");

        assert_eq!(rule.key, "skills");
        assert!(rule.file_names.is_empty());
        assert!(rule.exact_paths.is_empty());
        assert!(rule.scan_prefixes.is_empty());
        assert!(!rule.recursive);
        assert_eq!(rule.max_depth, None);
        assert_eq!(rule.max_files, None);
        assert_eq!(rule.max_size_bytes, 64 * 1024);
    }
}
