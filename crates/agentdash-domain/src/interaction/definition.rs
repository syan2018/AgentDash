use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentBinding {
    pub binding_key: String,
    pub component_ref: String,
    pub component_abi_version: u16,
    pub props: Value,
    #[serde(default)]
    pub event_commands: Vec<ComponentEventCommandBinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentEventCommandBinding {
    pub event_type: String,
    pub payload_schema: Value,
    pub command_key: String,
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
    pub owner: InteractionOwner,
    pub kind: InteractionDefinitionKind,
    pub definition_format_version: u16,
    pub interaction_contract_version: u16,
    pub title: String,
    pub description: String,
    pub source_bundle: SourceBundle,
    pub initial_state: Value,
    pub state_schema: Value,
    #[serde(default)]
    pub command_definitions: Vec<InteractionCommandDefinition>,
    #[serde(default)]
    pub component_bindings: Vec<ComponentBinding>,
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
            owner,
            kind: InteractionDefinitionKind::Canvas,
            definition_format_version: DEFINITION_FORMAT_V1,
            interaction_contract_version: INTERACTION_CONTRACT_V1,
            title: title.into(),
            description: description.into(),
            source_bundle,
            initial_state,
            state_schema,
            command_definitions: Vec::new(),
            component_bindings: Vec::new(),
            resource_slots: Vec::new(),
            lineage: None,
            created_by: created_by.into(),
            created_at: Utc::now(),
        };
        revision.validate()?;
        Ok(revision)
    }

    pub fn validate(&self) -> InteractionResult<()> {
        if self.definition_id.is_nil() || self.revision_id.is_nil() || self.revision_number == 0 {
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
        require_non_empty("definition_revision.title", &self.title)?;
        require_non_empty("definition_revision.created_by", &self.created_by)?;
        self.source_bundle.verify_digest()?;
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
                    .event_commands
                    .iter()
                    .map(|event| event.event_type.as_str()),
            )?;
            if component
                .event_commands
                .iter()
                .any(|event| !command_keys.contains(event.command_key.as_str()))
            {
                return Err(InteractionError::InvalidField {
                    field: "component_bindings.command_key",
                    reason: "event 必须引用同 revision 内存在的 command",
                });
            }
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
            owner: self.owner.clone(),
            kind: self.kind,
            current_revision_id: self.revision_id,
            status: InteractionDefinitionStatus::Active,
            created_at: now,
            updated_at: now,
        };
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
        let revision = InteractionDefinitionRevision::new_canvas_v1(
            definition_id,
            1,
            InteractionOwner::Project(Uuid::new_v4()),
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
}
