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

/// Session 构建阶段传递给动态 skill provider 的通用上下文。
///
/// 公开主仓只传递抽象事实；具体目录推导、组织策略和默认暴露策略由 provider
/// 自己实现。
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
}
