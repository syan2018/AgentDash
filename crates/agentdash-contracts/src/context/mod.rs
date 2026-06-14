use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VfsCapabilityDto {
    Read,
    Write,
    List,
    Search,
    Exec,
    Watch,
}

impl From<agentdash_domain::common::MountCapability> for VfsCapabilityDto {
    fn from(value: agentdash_domain::common::MountCapability) -> Self {
        match value {
            agentdash_domain::common::MountCapability::Read => Self::Read,
            agentdash_domain::common::MountCapability::Write => Self::Write,
            agentdash_domain::common::MountCapability::List => Self::List,
            agentdash_domain::common::MountCapability::Search => Self::Search,
            agentdash_domain::common::MountCapability::Exec => Self::Exec,
            agentdash_domain::common::MountCapability::Watch => Self::Watch,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ContextContainerFile {
    pub path: String,
    pub content: String,
}

impl From<agentdash_domain::context_container::ContextContainerFile> for ContextContainerFile {
    fn from(value: agentdash_domain::context_container::ContextContainerFile) -> Self {
        Self {
            path: value.path,
            content: value.content,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextContainerProvider {
    InlineFiles {
        files: Vec<ContextContainerFile>,
    },
    ExternalService {
        service_id: String,
        root_ref: String,
    },
}

impl From<agentdash_domain::context_container::ContextContainerProvider>
    for ContextContainerProvider
{
    fn from(value: agentdash_domain::context_container::ContextContainerProvider) -> Self {
        match value {
            agentdash_domain::context_container::ContextContainerProvider::InlineFiles {
                files,
            } => Self::InlineFiles {
                files: files.into_iter().map(ContextContainerFile::from).collect(),
            },
            agentdash_domain::context_container::ContextContainerProvider::ExternalService {
                service_id,
                root_ref,
            } => Self::ExternalService {
                service_id,
                root_ref,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ContextContainerDefinition {
    pub mount_id: String,
    pub display_name: String,
    pub provider: ContextContainerProvider,
    pub capabilities: Vec<VfsCapabilityDto>,
    pub default_write: bool,
}

impl From<agentdash_domain::context_container::ContextContainerDefinition>
    for ContextContainerDefinition
{
    fn from(value: agentdash_domain::context_container::ContextContainerDefinition) -> Self {
        Self {
            mount_id: value.mount_id,
            display_name: value.display_name,
            provider: ContextContainerProvider::from(value.provider),
            capabilities: value
                .capabilities
                .into_iter()
                .map(VfsCapabilityDto::from)
                .collect(),
            default_write: value.default_write,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKind {
    ManualText,
    File,
    ProjectSnapshot,
    HttpFetch,
    McpResource,
    EntityRef,
}

impl From<agentdash_domain::context_source::ContextSourceKind> for ContextSourceKind {
    fn from(value: agentdash_domain::context_source::ContextSourceKind) -> Self {
        match value {
            agentdash_domain::context_source::ContextSourceKind::ManualText => Self::ManualText,
            agentdash_domain::context_source::ContextSourceKind::File => Self::File,
            agentdash_domain::context_source::ContextSourceKind::ProjectSnapshot => {
                Self::ProjectSnapshot
            }
            agentdash_domain::context_source::ContextSourceKind::HttpFetch => Self::HttpFetch,
            agentdash_domain::context_source::ContextSourceKind::McpResource => Self::McpResource,
            agentdash_domain::context_source::ContextSourceKind::EntityRef => Self::EntityRef,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextSlot {
    Requirements,
    Constraints,
    Codebase,
    References,
    InstructionAppend,
}

impl From<agentdash_domain::context_source::ContextSlot> for ContextSlot {
    fn from(value: agentdash_domain::context_source::ContextSlot) -> Self {
        match value {
            agentdash_domain::context_source::ContextSlot::Requirements => Self::Requirements,
            agentdash_domain::context_source::ContextSlot::Constraints => Self::Constraints,
            agentdash_domain::context_source::ContextSlot::Codebase => Self::Codebase,
            agentdash_domain::context_source::ContextSlot::References => Self::References,
            agentdash_domain::context_source::ContextSlot::InstructionAppend => {
                Self::InstructionAppend
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextDelivery {
    Inline,
    Resource,
    Lazy,
}

impl From<agentdash_domain::context_source::ContextDelivery> for ContextDelivery {
    fn from(value: agentdash_domain::context_source::ContextDelivery) -> Self {
        match value {
            agentdash_domain::context_source::ContextDelivery::Inline => Self::Inline,
            agentdash_domain::context_source::ContextDelivery::Resource => Self::Resource,
            agentdash_domain::context_source::ContextDelivery::Lazy => Self::Lazy,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ContextSourceRef {
    pub kind: ContextSourceKind,
    pub locator: String,
    pub label: Option<String>,
    pub slot: ContextSlot,
    pub priority: i32,
    pub required: bool,
    pub max_chars: Option<usize>,
    pub delivery: ContextDelivery,
}

impl From<agentdash_domain::context_source::ContextSourceRef> for ContextSourceRef {
    fn from(value: agentdash_domain::context_source::ContextSourceRef) -> Self {
        Self {
            kind: ContextSourceKind::from(value.kind),
            locator: value.locator,
            label: value.label,
            slot: ContextSlot::from(value.slot),
            priority: value.priority,
            required: value.required,
            max_chars: value.max_chars,
            delivery: ContextDelivery::from(value.delivery),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct SessionRequiredContextBlock {
    pub title: String,
    pub content: String,
}

impl From<agentdash_domain::session_composition::SessionRequiredContextBlock>
    for SessionRequiredContextBlock
{
    fn from(value: agentdash_domain::session_composition::SessionRequiredContextBlock) -> Self {
        Self {
            title: value.title,
            content: value.content,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct SessionComposition {
    pub persona_label: Option<String>,
    pub persona_prompt: Option<String>,
    pub workflow_steps: Vec<String>,
    pub required_context_blocks: Vec<SessionRequiredContextBlock>,
}

impl From<agentdash_domain::session_composition::SessionComposition> for SessionComposition {
    fn from(value: agentdash_domain::session_composition::SessionComposition) -> Self {
        Self {
            persona_label: value.persona_label,
            persona_prompt: value.persona_prompt,
            workflow_steps: value.workflow_steps,
            required_context_blocks: value
                .required_context_blocks
                .into_iter()
                .map(SessionRequiredContextBlock::from)
                .collect(),
        }
    }
}
