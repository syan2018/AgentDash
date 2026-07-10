use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum InteractionOwnerDto {
    User(String),
    Project(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InteractionDefinitionStatusDto {
    Active,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CanvasDefinitionListScopeDto {
    All,
    Mine,
    Shared,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct InteractionDefinitionAccessDto {
    pub can_view: bool,
    pub can_edit_source: bool,
    pub can_publish: bool,
    pub can_manage_shared: bool,
    pub can_copy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InteractionSourceFileDto {
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InteractionSourceSandboxDto {
    #[serde(default)]
    pub libraries: Vec<String>,
    #[serde(default)]
    pub import_map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InteractionSourceBundleDto {
    pub format_version: u16,
    pub entry_file: String,
    pub files: Vec<InteractionSourceFileDto>,
    pub sandbox: InteractionSourceSandboxDto,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InteractionDefinitionLineageKindDto {
    PublishedFrom,
    CopiedFrom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InteractionDefinitionLineageDto {
    pub kind: InteractionDefinitionLineageKindDto,
    pub source_definition_id: String,
    pub source_revision_id: String,
    pub source_bundle_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CanvasDefinitionDto {
    pub definition_id: String,
    pub project_id: String,
    pub owner: InteractionOwnerDto,
    pub status: InteractionDefinitionStatusDto,
    pub current_revision_id: String,
    pub revision_number: u64,
    pub definition_format_version: u16,
    pub interaction_contract_version: u16,
    pub title: String,
    pub description: String,
    pub source_bundle: InteractionSourceBundleDto,
    pub initial_state: Value,
    pub state_schema: Value,
    #[serde(default)]
    pub command_definitions: Vec<InteractionCommandDefinitionDto>,
    #[serde(default)]
    pub component_bindings: Vec<InteractionComponentBindingDto>,
    #[serde(default)]
    pub resource_slots: Vec<InteractionResourceSlotDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lineage: Option<InteractionDefinitionLineageDto>,
    pub access: InteractionDefinitionAccessDto,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct ListCanvasDefinitionsQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope: Option<CanvasDefinitionListScopeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateCanvasDefinitionRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub source_bundle: InteractionSourceBundleDto,
    #[serde(default)]
    pub initial_state: Value,
    #[serde(default)]
    pub state_schema: Value,
    #[serde(default)]
    pub command_definitions: Vec<InteractionCommandDefinitionDto>,
    #[serde(default)]
    pub component_bindings: Vec<InteractionComponentBindingDto>,
    #[serde(default)]
    pub resource_slots: Vec<InteractionResourceSlotDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionSourceFileChangeDto {
    Upsert { file: InteractionSourceFileDto },
    Delete { path: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct InteractionSourceChangesetDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entry_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sandbox: Option<InteractionSourceSandboxDto>,
    #[serde(default)]
    pub file_changes: Vec<InteractionSourceFileChangeDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct CommitCanvasDefinitionRequest {
    pub base_revision_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub changeset: InteractionSourceChangesetDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub command_definitions: Option<Vec<InteractionCommandDefinitionDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub component_bindings: Option<Vec<InteractionComponentBindingDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resource_slots: Option<Vec<InteractionResourceSlotDto>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InteractionCommandActorPolicyDto {
    Direct,
    HumanOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionOperationRefDto {
    pub namespace: String,
    pub provider_key: String,
    pub operation_key: String,
    pub contract_version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionStatePatchV1ContractDto {
    pub allowed_paths: Vec<String>,
    pub max_operations: u64,
    pub max_state_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionCommandDefinitionDto {
    pub command_key: String,
    pub actor_policy: InteractionCommandActorPolicyDto,
    pub payload_schema: Value,
    pub state_patch_v1: InteractionStatePatchV1ContractDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub operation_effect: Option<InteractionOperationRefDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionComponentEventBindingDto {
    pub event_type: String,
    pub payload_schema: Value,
    pub command_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionComponentBindingDto {
    pub binding_key: String,
    pub component_ref: String,
    pub component_abi_version: u16,
    pub props: Value,
    #[serde(default)]
    pub event_commands: Vec<InteractionComponentEventBindingDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InteractionResourceSlotKindDto {
    Resource,
    Artifact,
    Provider,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionResourceSlotDto {
    pub slot_key: String,
    pub kind: InteractionResourceSlotKindDto,
    pub required: bool,
    pub contract: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct DistributeCanvasDefinitionRequest {
    pub source_revision_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ArchiveInteractionDefinitionResponse {
    pub definition_id: String,
    pub status: InteractionDefinitionStatusDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionInstanceDto {
    pub instance_id: String,
    pub owner: InteractionOwnerDto,
    pub definition_id: String,
    pub definition_revision_id: String,
    pub interaction_contract_version: u16,
    pub state: Value,
    pub state_revision: u64,
    pub status: String,
    #[serde(default)]
    pub pinned_artifacts: Vec<InteractionPinnedArtifactDto>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionPinnedArtifactDto {
    pub artifact_ref: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionRuntimeBindingTargetDto {
    Resource {
        resource_ref: String,
        version_ref: String,
    },
    Artifact {
        artifact_ref: String,
        digest: String,
    },
    Provider {
        provider_ref: String,
        contract_version: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionRuntimeBindingDto {
    pub binding_id: String,
    pub slot_key: String,
    pub target: InteractionRuntimeBindingTargetDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionInstanceViewDto {
    pub instance: InteractionInstanceDto,
    pub runtime_bindings: Vec<InteractionRuntimeBindingDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateInteractionInstanceRequestDto {
    pub definition_revision_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CloseInteractionInstanceRequestDto {
    pub expected_state_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionCommandRequestDto {
    pub command_id: String,
    pub command_key: String,
    pub payload: Value,
    pub expected_state_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InteractionCommandResponseDto {
    pub instance: InteractionInstanceDto,
    pub event_id: String,
    pub event_sequence: u64,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationWorkshopContextDto {
    Project,
    Canvas { definition_id: String },
    Interaction { instance_id: String },
    ExtensionPanel { installation_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationWorkshopSurfaceRequestDto {
    pub context: OperationWorkshopContextDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationWorkshopDescriptorDto {
    pub operation_ref: InteractionOperationRefDto,
    pub title: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub output_schema: Value,
    pub effect: String,
    pub replay_policy: String,
    pub required_capabilities: Vec<String>,
    pub ready: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationWorkshopSurfaceDto {
    pub authority_revision: String,
    pub operations: Vec<OperationWorkshopDescriptorDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationWorkshopInvokeRequestDto {
    pub context: OperationWorkshopContextDto,
    pub operation_ref: InteractionOperationRefDto,
    #[serde(default)]
    pub input: Value,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationWorkshopInvokeResponseDto {
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationScriptLimitsDto {
    pub timeout_ms: u32,
    pub max_source_bytes: u32,
    pub max_input_bytes: u32,
    pub max_output_bytes: u32,
    pub max_rhai_operations: u32,
    pub max_call_levels: u32,
    pub max_string_size: u32,
    pub max_array_size: u32,
    pub max_map_size: u32,
    pub max_operation_calls: u32,
    pub max_parallel_operations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationScriptProgramDto {
    pub language: String,
    pub host_api_version: u16,
    pub source: String,
    #[serde(default)]
    pub input: Value,
    pub requested_operations: Vec<InteractionOperationRefDto>,
    pub limits: OperationScriptLimitsDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationScriptPreflightTokenDto {
    pub plan_id: String,
    pub binding_digest: String,
    pub issued_at: String,
    pub expires_at: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationWorkshopScriptPreflightRequestDto {
    pub context: OperationWorkshopContextDto,
    pub program: OperationScriptProgramDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationWorkshopScriptPreflightResponseDto {
    pub token: OperationScriptPreflightTokenDto,
    pub source_digest: String,
    pub manifest_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationWorkshopScriptRunRequestDto {
    pub context: OperationWorkshopContextDto,
    pub program: OperationScriptProgramDto,
    pub token: OperationScriptPreflightTokenDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OperationWorkshopScriptRunResponseDto {
    pub outcome: Value,
}
