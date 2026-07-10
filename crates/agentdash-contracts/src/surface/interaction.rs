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
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub closed_at: Option<String>,
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
