use std::collections::BTreeSet;

use agentdash_domain::common::Mount;
use agentdash_domain::workflow::MountDirective;
use agentdash_platform_spi::{CapabilityState, RuntimeMcpServer, Vfs};

#[derive(Debug, Clone, Default)]
pub struct KickoffPromptFragment {
    pub title_line: String,
    pub output_section: String,
    pub input_section: String,
}

#[derive(Debug, Clone)]
pub struct ActivityActivation {
    pub capability_state: CapabilityState,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub capability_keys: BTreeSet<String>,
    pub kickoff_prompt: KickoffPromptFragment,
    pub lifecycle_mount: Mount,
    pub lifecycle_vfs: Vfs,
    pub mount_directives: Vec<MountDirective>,
}
