use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

use crate::operation::OperationRef;

use super::{
    DEFINITION_FORMAT_V1, INTERACTION_CONTRACT_V1, InteractionCommandRequest, InteractionError,
    InteractionResult, ResolvedInteractionCommand, SourceBundle,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum InteractionOwner {
    User(String),
    Project(Uuid),
}

impl InteractionOwner {
    pub fn validate(&self) -> InteractionResult<()> {
        match self {
            Self::User(user_id) if user_id.trim().is_empty() => {
                Err(InteractionError::InvalidField {
                    field: "owner.user_id",
                    reason: "user id 不能为空",
                })
            }
            Self::Project(project_id) if project_id.is_nil() => {
                Err(InteractionError::InvalidField {
                    field: "owner.project_id",
                    reason: "project id 不能为空",
                })
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionDefinitionKind {
    Canvas,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionDefinitionStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionDefinitionAccess {
    pub can_view: bool,
    pub can_edit_source: bool,
    pub can_publish: bool,
    pub can_manage_shared: bool,
    pub can_copy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandActorPolicy {
    Direct,
    HumanOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "handler", rename_all = "snake_case")]
pub enum PlatformCommandHandler {
    StatePatchV1,
}

impl PlatformCommandHandler {
    pub fn version(&self) -> u16 {
        1
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionCommandDefinition {
    pub command_key: String,
    pub handler: PlatformCommandHandler,
    pub actor_policy: CommandActorPolicy,
    pub payload_schema: Value,
    pub state_patch_v1: Option<super::StatePatchV1Contract>,
    pub operation_effect: Option<InteractionOperationEffectDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionOperationEffectDefinition {
    pub operation_ref: OperationRef,
}

pub const AGENT_STATE_PROJECTION_V1: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionAgentProjection {
    pub version: u16,
    pub allowed_state_paths: Vec<String>,
}

impl Default for InteractionAgentProjection {
    fn default() -> Self {
        Self {
            version: AGENT_STATE_PROJECTION_V1,
            allowed_state_paths: Vec::new(),
        }
    }
}

impl InteractionAgentProjection {
    pub fn validate(&self) -> InteractionResult<()> {
        if self.version != AGENT_STATE_PROJECTION_V1 {
            return Err(InteractionError::InvalidField {
                field: "agent_projection.version",
                reason: "只支持 V1 Agent state projection",
            });
        }
        let mut unique = HashSet::new();
        for path in &self.allowed_state_paths {
            if !valid_json_pointer(path) {
                return Err(InteractionError::InvalidField {
                    field: "agent_projection.allowed_state_paths",
                    reason: "必须是非根 canonical JSON Pointer",
                });
            }
            if !unique.insert(path) {
                return Err(InteractionError::InvalidField {
                    field: "agent_projection.allowed_state_paths",
                    reason: "JSON Pointer 必须唯一",
                });
            }
        }
        Ok(())
    }

    pub fn project(&self, state: &Value) -> InteractionResult<BTreeMap<String, Value>> {
        self.validate()?;
        self.allowed_state_paths
            .iter()
            .map(|path| {
                state
                    .pointer(path)
                    .cloned()
                    .map(|value| (path.clone(), value))
                    .ok_or(InteractionError::InvalidField {
                        field: "agent_projection.allowed_state_paths",
                        reason: "JSON Pointer 在 current state 中不存在",
                    })
            })
            .collect()
    }
}

fn valid_json_pointer(path: &str) -> bool {
    !path.is_empty()
        && path.starts_with('/')
        && path.split('/').skip(1).all(|token| {
            let bytes = token.as_bytes();
            let mut index = 0;
            while index < bytes.len() {
                if bytes[index] == b'~'
                    && (index + 1 >= bytes.len() || !matches!(bytes[index + 1], b'0' | b'1'))
                {
                    return false;
                }
                index += if bytes[index] == b'~' { 2 } else { 1 };
            }
            true
        })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentBinding {
    pub binding_key: String,
    pub component_ref: String,
    pub component_abi_version: u16,
    pub props: Value,
    #[serde(default)]
    pub event_bindings: Vec<ComponentEventBinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentEventBinding {
    pub event_type: String,
    pub payload_schema: Value,
    pub target: InteractionActionTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionActionTarget {
    PlatformCommand {
        command_key: String,
    },
    Operation {
        operation_ref: OperationRef,
    },
    OperationScript {
        language: String,
        host_api_version: u16,
        source: OperationScriptSource,
        requested_operations: Vec<OperationRef>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationScriptSource {
    Inline { source: String },
    SourceFile { path: String },
}

/// Definition-level action callable by Canvas source without requiring an Extension UI component.
///
/// The browser submits only `action_key` and payload. The immutable definition revision owns the
/// exact command/Operation/OperationScript target and its allowlist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionActionBinding {
    pub action_key: String,
    pub payload_schema: Value,
    pub target: InteractionActionTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceSlotKind {
    Resource,
    Artifact,
    Provider,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceSlotDefinition {
    pub slot_key: String,
    pub kind: ResourceSlotKind,
    pub required: bool,
    pub contract: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionLineageKind {
    PublishedFrom,
    CopiedFrom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionLineage {
    pub kind: DefinitionLineageKind,
    pub source_definition_id: Uuid,
    pub source_revision_id: Uuid,
    pub source_bundle_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub owner: InteractionOwner,
    pub kind: InteractionDefinitionKind,
    pub current_revision_id: Uuid,
    pub status: InteractionDefinitionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionDefinitionRevision {
    pub definition_id: Uuid,
    pub revision_id: Uuid,
    pub revision_number: u64,
    pub project_id: Uuid,
    pub owner: InteractionOwner,
    pub kind: InteractionDefinitionKind,
    pub definition_format_version: u16,
    pub interaction_contract_version: u16,
    /// Stable authoring/VFS identity for this Canvas definition.
    ///
    /// The identity survives immutable source revisions and is distinct from
    /// the definition/revision UUIDs used by persistence and runtime pinning.
    pub authoring_mount_id: String,
    pub title: String,
    pub description: String,
    pub source_bundle: SourceBundle,
    pub initial_state: Value,
    pub state_schema: Value,
    pub agent_projection: InteractionAgentProjection,
    #[serde(default)]
    pub command_definitions: Vec<InteractionCommandDefinition>,
    #[serde(default)]
    pub component_bindings: Vec<ComponentBinding>,
    #[serde(default)]
    pub action_bindings: Vec<InteractionActionBinding>,
    #[serde(default)]
    pub resource_slots: Vec<ResourceSlotDefinition>,
    pub lineage: Option<DefinitionLineage>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

impl InteractionDefinitionRevision {
    #[allow(clippy::too_many_arguments)]
    pub fn new_canvas_v1(
        definition_id: Uuid,
        revision_number: u64,
        project_id: Uuid,
        owner: InteractionOwner,
        title: impl Into<String>,
        description: impl Into<String>,
        source_bundle: SourceBundle,
        initial_state: Value,
        state_schema: Value,
        created_by: impl Into<String>,
    ) -> InteractionResult<Self> {
        let revision = Self {
            definition_id,
            revision_id: Uuid::new_v4(),
            revision_number,
            project_id,
            owner,
            kind: InteractionDefinitionKind::Canvas,
            definition_format_version: DEFINITION_FORMAT_V1,
            interaction_contract_version: INTERACTION_CONTRACT_V1,
            authoring_mount_id: canvas_authoring_mount_id(definition_id),
            title: title.into(),
            description: description.into(),
            source_bundle,
            initial_state,
            state_schema,
            agent_projection: InteractionAgentProjection::default(),
            command_definitions: Vec::new(),
            component_bindings: Vec::new(),
            action_bindings: Vec::new(),
            resource_slots: Vec::new(),
            lineage: None,
            created_by: created_by.into(),
            created_at: Utc::now(),
        };
        revision.validate()?;
        Ok(revision)
    }

    pub fn validate(&self) -> InteractionResult<()> {
        if self.definition_id.is_nil()
            || self.revision_id.is_nil()
            || self.revision_number == 0
            || self.project_id.is_nil()
        {
            return Err(InteractionError::InvalidField {
                field: "definition_revision.identity",
                reason: "definition/revision id 与 revision number 必须有效",
            });
        }
        if self.definition_format_version != DEFINITION_FORMAT_V1
            || self.interaction_contract_version != INTERACTION_CONTRACT_V1
        {
            return Err(InteractionError::InvalidField {
                field: "definition_revision.version",
                reason: "只支持 V1 definition 与 interaction contract",
            });
        }
        self.owner.validate()?;
        validate_project_owner(self.project_id, &self.owner)?;
        normalize_canvas_authoring_mount_id(&self.authoring_mount_id)?;
        require_non_empty("definition_revision.title", &self.title)?;
        require_non_empty("definition_revision.created_by", &self.created_by)?;
        self.source_bundle.verify_digest()?;
        self.agent_projection.validate()?;
        validate_unique_keys(
            "command_definitions.command_key",
            self.command_definitions
                .iter()
                .map(|definition| definition.command_key.as_str()),
        )?;
        validate_unique_keys(
            "component_bindings.binding_key",
            self.component_bindings
                .iter()
                .map(|binding| binding.binding_key.as_str()),
        )?;
        validate_unique_keys(
            "action_bindings.action_key",
            self.action_bindings
                .iter()
                .map(|binding| binding.action_key.as_str()),
        )?;
        validate_unique_keys(
            "resource_slots.slot_key",
            self.resource_slots
                .iter()
                .map(|slot| slot.slot_key.as_str()),
        )?;
        self.validate_nested_contracts()
    }

    fn validate_nested_contracts(&self) -> InteractionResult<()> {
        let command_keys = self
            .command_definitions
            .iter()
            .map(|definition| definition.command_key.as_str())
            .collect::<std::collections::HashSet<_>>();
        for command in &self.command_definitions {
            let contract =
                command
                    .state_patch_v1
                    .as_ref()
                    .ok_or(InteractionError::InvalidField {
                        field: "command_definitions.state_patch_v1",
                        reason: "state_patch_v1 handler 必须声明 patch contract",
                    })?;
            contract.validate_contract()?;
            if let Some(effect) = &command.operation_effect {
                effect.operation_ref.validate().map_err(|error| {
                    InteractionError::InvalidOperationRef {
                        reason: error.to_string(),
                    }
                })?;
            }
        }
        for component in &self.component_bindings {
            require_non_empty("component_bindings.component_ref", &component.component_ref)?;
            if component.component_abi_version == 0 {
                return Err(InteractionError::InvalidField {
                    field: "component_bindings.component_abi_version",
                    reason: "ABI version 必须大于 0",
                });
            }
            validate_unique_keys(
                "component_bindings.event_type",
                component
                    .event_bindings
                    .iter()
                    .map(|event| event.event_type.as_str()),
            )?;
            for event in &component.event_bindings {
                match &event.target {
                    InteractionActionTarget::PlatformCommand { command_key }
                        if !command_keys.contains(command_key.as_str()) =>
                    {
                        return Err(InteractionError::InvalidField {
                            field: "component_bindings.target.command_key",
                            reason: "event 必须引用同 revision 内存在的 command",
                        });
                    }
                    InteractionActionTarget::Operation { operation_ref } => {
                        operation_ref.validate().map_err(|error| {
                            InteractionError::InvalidOperationRef {
                                reason: error.to_string(),
                            }
                        })?;
                    }
                    InteractionActionTarget::OperationScript {
                        language,
                        host_api_version,
                        source,
                        requested_operations,
                    } => {
                        if language != "rhai_v1"
                            || *host_api_version != 1
                            || requested_operations.is_empty()
                        {
                            return Err(InteractionError::InvalidField {
                                field: "component_bindings.target.operation_script",
                                reason: "必须声明 rhai_v1、host API V1 与至少一个 exact OperationRef",
                            });
                        }
                        for operation_ref in requested_operations {
                            operation_ref.validate().map_err(|error| {
                                InteractionError::InvalidOperationRef {
                                    reason: error.to_string(),
                                }
                            })?;
                        }
                        match source {
                            OperationScriptSource::Inline { source }
                                if source.trim().is_empty() =>
                            {
                                return Err(InteractionError::InvalidField {
                                    field: "component_bindings.target.operation_script.source",
                                    reason: "inline Rhai source 不能为空",
                                });
                            }
                            OperationScriptSource::SourceFile { path }
                                if !path.ends_with(".rhai")
                                    || !self
                                        .source_bundle
                                        .files
                                        .iter()
                                        .any(|file| file.path == *path) =>
                            {
                                return Err(InteractionError::InvalidField {
                                    field: "component_bindings.target.operation_script.source_file",
                                    reason: "必须引用当前 immutable SourceBundle 中的 .rhai 文件",
                                });
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        for action in &self.action_bindings {
            require_non_empty("action_bindings.action_key", &action.action_key)?;
            self.validate_action_target(&action.target, &command_keys, "action_bindings.target")?;
        }
        if let Some(lineage) = &self.lineage {
            if lineage.source_definition_id.is_nil()
                || lineage.source_revision_id.is_nil()
                || lineage.source_definition_id == self.definition_id
            {
                return Err(InteractionError::InvalidField {
                    field: "definition_revision.lineage",
                    reason: "lineage 必须引用其它 definition 的 exact revision",
                });
            }
            validate_sha256(
                "definition_revision.lineage.source_bundle_digest",
                &lineage.source_bundle_digest,
            )?;
        }
        Ok(())
    }

    fn validate_action_target(
        &self,
        target: &InteractionActionTarget,
        command_keys: &std::collections::HashSet<&str>,
        field: &'static str,
    ) -> InteractionResult<()> {
        match target {
            InteractionActionTarget::PlatformCommand { command_key }
                if !command_keys.contains(command_key.as_str()) =>
            {
                Err(InteractionError::InvalidField {
                    field,
                    reason: "action 必须引用同 revision 内存在的 command",
                })
            }
            InteractionActionTarget::Operation { operation_ref } => operation_ref
                .validate()
                .map_err(|error| InteractionError::InvalidOperationRef {
                    reason: error.to_string(),
                }),
            InteractionActionTarget::OperationScript {
                language,
                host_api_version,
                source,
                requested_operations,
            } => {
                if language != "rhai_v1"
                    || *host_api_version != 1
                    || requested_operations.is_empty()
                {
                    return Err(InteractionError::InvalidField {
                        field,
                        reason: "必须声明 rhai_v1、host API V1 与至少一个 exact OperationRef",
                    });
                }
                for operation_ref in requested_operations {
                    operation_ref.validate().map_err(|error| {
                        InteractionError::InvalidOperationRef {
                            reason: error.to_string(),
                        }
                    })?;
                }
                match source {
                    OperationScriptSource::Inline { source } if source.trim().is_empty() => {
                        Err(InteractionError::InvalidField {
                            field,
                            reason: "inline Rhai source 不能为空",
                        })
                    }
                    OperationScriptSource::SourceFile { path }
                        if !path.ends_with(".rhai")
                            || !self
                                .source_bundle
                                .files
                                .iter()
                                .any(|file| file.path == *path) =>
                    {
                        Err(InteractionError::InvalidField {
                            field,
                            reason: "必须引用当前 immutable SourceBundle 中的 .rhai 文件",
                        })
                    }
                    _ => Ok(()),
                }
            }
            InteractionActionTarget::PlatformCommand { .. } => Ok(()),
        }
    }

    pub fn into_initial_definition(self) -> InteractionResult<(InteractionDefinition, Self)> {
        if self.revision_number != 1 {
            return Err(InteractionError::InvalidField {
                field: "definition_revision.revision_number",
                reason: "initial definition revision 必须为 1",
            });
        }
        let now = self.created_at;
        let definition = InteractionDefinition {
            id: self.definition_id,
            project_id: self.project_id,
            owner: self.owner.clone(),
            kind: self.kind,
            current_revision_id: self.revision_id,
            status: InteractionDefinitionStatus::Active,
            created_at: now,
            updated_at: now,
        };
        definition.validate()?;
        Ok((definition, self))
    }

    pub fn resolve_command(
        &self,
        request: InteractionCommandRequest,
    ) -> InteractionResult<ResolvedInteractionCommand> {
        let definition = self
            .command_definitions
            .iter()
            .find(|definition| definition.command_key == request.command_key)
            .ok_or_else(|| InteractionError::NotFound {
                entity: "interaction_command_definition",
                id: request.command_key.clone(),
            })?;
        request.enforce_actor_policy(definition.actor_policy)?;
        Ok(ResolvedInteractionCommand {
            request,
            handler: definition.handler.clone(),
            actor_policy: definition.actor_policy,
        })
    }
}

pub fn canvas_authoring_mount_id(definition_id: Uuid) -> String {
    format!("cvs-{definition_id}")
}

pub fn normalize_canvas_authoring_mount_id(raw: &str) -> InteractionResult<String> {
    let value = raw.trim();
    if !value.starts_with("cvs-")
        || value.len() <= "cvs-".len()
        || value["cvs-".len()..].starts_with("cvs-")
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '/' | '\\' | ':'))
    {
        return Err(InteractionError::InvalidField {
            field: "definition_revision.authoring_mount_id",
            reason: "必须是带 `cvs-` 前缀且不含空白或路径分隔符的稳定标识",
        });
    }
    Ok(value.to_owned())
}

impl InteractionDefinition {
    pub fn validate(&self) -> InteractionResult<()> {
        if self.id.is_nil() || self.current_revision_id.is_nil() || self.project_id.is_nil() {
            return Err(InteractionError::InvalidField {
                field: "interaction_definition.identity",
                reason: "definition/project/current revision id 必须有效",
            });
        }
        self.owner.validate()?;
        validate_project_owner(self.project_id, &self.owner)
    }
}

fn validate_project_owner(project_id: Uuid, owner: &InteractionOwner) -> InteractionResult<()> {
    if matches!(owner, InteractionOwner::Project(owner_id) if *owner_id != project_id) {
        Err(InteractionError::InvalidField {
            field: "interaction_definition.project_owner",
            reason: "Project owner id 必须等于 definition project_id",
        })
    } else {
        Ok(())
    }
}

fn require_non_empty(field: &'static str, value: &str) -> InteractionResult<()> {
    if value.trim().is_empty() {
        Err(InteractionError::InvalidField {
            field,
            reason: "不能为空",
        })
    } else {
        Ok(())
    }
}

fn validate_unique_keys<'a>(
    field: &'static str,
    values: impl Iterator<Item = &'a str>,
) -> InteractionResult<()> {
    let mut seen = std::collections::HashSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(InteractionError::InvalidField {
                field,
                reason: "key 不能为空",
            });
        }
        if !seen.insert(value) {
            return Err(InteractionError::InvalidField {
                field,
                reason: "key 必须唯一",
            });
        }
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> InteractionResult<()> {
    let valid = value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.chars().all(|character| character.is_ascii_hexdigit())
    });
    if valid {
        Ok(())
    } else {
        Err(InteractionError::InvalidDigest { field })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{SourceBundle, SourceFile, SourceSandboxConfig, StatePatchV1Contract};

    fn source_bundle() -> SourceBundle {
        SourceBundle::new(
            "src/main.tsx",
            vec![SourceFile::new("src/main.tsx", "export {};", None).expect("source")],
            SourceSandboxConfig::default(),
        )
        .expect("bundle")
    }

    #[test]
    fn initial_definition_pins_v1_contracts() {
        let definition_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let revision = InteractionDefinitionRevision::new_canvas_v1(
            definition_id,
            1,
            project_id,
            InteractionOwner::Project(project_id),
            "Dashboard",
            "",
            source_bundle(),
            serde_json::json!({}),
            serde_json::json!({"type": "object"}),
            "user-1",
        )
        .expect("revision");

        let (definition, revision) = revision.into_initial_definition().expect("definition");
        assert_eq!(definition.current_revision_id, revision.revision_id);
        assert_eq!(revision.definition_format_version, DEFINITION_FORMAT_V1);
        assert_eq!(
            revision.interaction_contract_version,
            INTERACTION_CONTRACT_V1
        );
    }

    #[test]
    fn definition_rejects_duplicate_command_keys() {
        let mut revision = InteractionDefinitionRevision::new_canvas_v1(
            Uuid::new_v4(),
            1,
            Uuid::new_v4(),
            InteractionOwner::User("user-1".to_string()),
            "Personal",
            "",
            source_bundle(),
            serde_json::json!({}),
            serde_json::json!({}),
            "user-1",
        )
        .expect("revision");
        let command = InteractionCommandDefinition {
            command_key: "set_value".to_string(),
            handler: PlatformCommandHandler::StatePatchV1,
            actor_policy: CommandActorPolicy::Direct,
            payload_schema: serde_json::json!({}),
            state_patch_v1: Some(
                StatePatchV1Contract::new(vec!["/value".to_string()], 10, 1024).expect("contract"),
            ),
            operation_effect: None,
        };
        revision.command_definitions = vec![command.clone(), command];

        assert!(matches!(
            revision.validate(),
            Err(InteractionError::InvalidField {
                field: "command_definitions.command_key",
                ..
            })
        ));
    }

    #[test]
    fn command_handler_is_resolved_from_pinned_definition() {
        let mut revision = InteractionDefinitionRevision::new_canvas_v1(
            Uuid::new_v4(),
            1,
            Uuid::new_v4(),
            InteractionOwner::User("user-1".into()),
            "Personal",
            "",
            source_bundle(),
            serde_json::json!({}),
            serde_json::json!({}),
            "user-1",
        )
        .expect("revision");
        revision
            .command_definitions
            .push(InteractionCommandDefinition {
                command_key: "set_value".into(),
                handler: PlatformCommandHandler::StatePatchV1,
                actor_policy: CommandActorPolicy::Direct,
                payload_schema: serde_json::json!({}),
                state_patch_v1: Some(
                    StatePatchV1Contract::new(vec!["/value".into()], 1, 1024).expect("contract"),
                ),
                operation_effect: None,
            });
        let resolved = revision
            .resolve_command(InteractionCommandRequest {
                instance_id: Uuid::new_v4(),
                command_id: Uuid::new_v4(),
                command_key: "set_value".into(),
                payload: serde_json::json!([]),
                expected_state_revision: 0,
                actor: crate::interaction::InteractionActor::Human {
                    user_id: "user-1".into(),
                },
                origin: crate::interaction::InteractionCommandOrigin::UserWorkshop,
                attachment_id: None,
            })
            .expect("resolved command");
        assert_eq!(resolved.handler, PlatformCommandHandler::StatePatchV1);
    }

    #[test]
    fn project_owner_must_match_workshop_project() {
        let error = InteractionDefinitionRevision::new_canvas_v1(
            Uuid::new_v4(),
            1,
            Uuid::new_v4(),
            InteractionOwner::Project(Uuid::new_v4()),
            "Shared",
            "",
            source_bundle(),
            serde_json::json!({}),
            serde_json::json!({}),
            "user-1",
        )
        .expect_err("project owner mismatch");
        assert!(matches!(
            error,
            InteractionError::InvalidField {
                field: "interaction_definition.project_owner",
                ..
            }
        ));
    }

    #[test]
    fn agent_projection_exposes_only_allowlisted_state_paths() {
        let projection = InteractionAgentProjection {
            version: AGENT_STATE_PROJECTION_V1,
            allowed_state_paths: vec!["/board/title".into(), "/items/0/id".into()],
        };
        let projected = projection
            .project(&serde_json::json!({
                "board": {"title": "Sprint", "secret": "hidden"},
                "items": [{"id": "item-1", "token": "hidden"}]
            }))
            .expect("projection");

        assert_eq!(projected["/board/title"], serde_json::json!("Sprint"));
        assert_eq!(projected["/items/0/id"], serde_json::json!("item-1"));
        assert_eq!(projected.len(), 2);
    }

    #[test]
    fn agent_projection_rejects_root_and_invalid_escape() {
        for path in ["", "/secret~2token"] {
            let projection = InteractionAgentProjection {
                version: AGENT_STATE_PROJECTION_V1,
                allowed_state_paths: vec![path.into()],
            };
            assert!(projection.validate().is_err());
        }
    }

    #[test]
    fn component_operation_script_source_must_be_pinned_rhai() {
        let project_id = Uuid::new_v4();
        let mut revision = InteractionDefinitionRevision::new_canvas_v1(
            Uuid::new_v4(),
            1,
            project_id,
            InteractionOwner::Project(project_id),
            "Scripted",
            "",
            SourceBundle::new(
                "src/main.tsx",
                vec![
                    SourceFile::new("src/main.tsx", "export {};", None).expect("source"),
                    SourceFile::new("actions/load.rhai", "return input;", None).expect("script"),
                ],
                SourceSandboxConfig::default(),
            )
            .expect("bundle"),
            serde_json::json!({}),
            serde_json::json!({"type": "object"}),
            "user-1",
        )
        .expect("revision");
        let operation_ref =
            OperationRef::new("extension", "metrics", "load", 1).expect("operation ref");
        revision.component_bindings.push(ComponentBinding {
            binding_key: "metrics".into(),
            component_ref: "metrics-card".into(),
            component_abi_version: 1,
            props: serde_json::json!({}),
            event_bindings: vec![ComponentEventBinding {
                event_type: "refresh".into(),
                payload_schema: serde_json::json!({"type": "object"}),
                target: InteractionActionTarget::OperationScript {
                    language: "rhai_v1".into(),
                    host_api_version: 1,
                    source: OperationScriptSource::SourceFile {
                        path: "actions/load.rhai".into(),
                    },
                    requested_operations: vec![operation_ref],
                },
            }],
        });
        assert!(revision.validate().is_ok());

        let InteractionActionTarget::OperationScript { source, .. } =
            &mut revision.component_bindings[0].event_bindings[0].target
        else {
            panic!("script target");
        };
        *source = OperationScriptSource::SourceFile {
            path: "actions/missing.rhai".into(),
        };
        assert!(revision.validate().is_err());
    }

    #[test]
    fn canvas_action_does_not_require_extension_component_artifact() {
        let project_id = Uuid::new_v4();
        let mut revision = InteractionDefinitionRevision::new_canvas_v1(
            Uuid::new_v4(),
            1,
            project_id,
            InteractionOwner::Project(project_id),
            "Skills",
            "",
            SourceBundle::new(
                "src/main.tsx",
                vec![
                    SourceFile::new("src/main.tsx", "export {};", None).expect("source"),
                    SourceFile::new("actions/load-skills.rhai", "return input;", None)
                        .expect("script"),
                ],
                SourceSandboxConfig::default(),
            )
            .expect("bundle"),
            serde_json::json!({}),
            serde_json::json!({"type": "object"}),
            "user-1",
        )
        .expect("revision");
        revision.action_bindings.push(InteractionActionBinding {
            action_key: "skills.refresh".into(),
            payload_schema: serde_json::json!({"type": "object"}),
            target: InteractionActionTarget::OperationScript {
                language: "rhai_v1".into(),
                host_api_version: 1,
                source: OperationScriptSource::SourceFile {
                    path: "actions/load-skills.rhai".into(),
                },
                requested_operations: vec![
                    OperationRef::new("platform", "vfs", "fs_glob", 1).expect("glob"),
                    OperationRef::new("platform", "vfs", "fs_read", 1).expect("read"),
                ],
            },
        });

        assert!(revision.component_bindings.is_empty());
        assert!(revision.validate().is_ok());
    }
}
