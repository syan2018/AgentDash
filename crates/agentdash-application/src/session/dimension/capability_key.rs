//! 能力 Key 维度 — 追踪 capability key 的增删与当前生效集合。

use std::collections::BTreeSet;

use agentdash_spi::hooks::ContextFrameSection;

use super::DimensionDelta;
use crate::capability::capability_description;
use crate::session::{CapabilityStateDelta, SetDelta};

#[derive(Debug, Clone)]
pub(crate) struct CapabilityKeyDimensionDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub effective: Vec<String>,
}

impl CapabilityKeyDimensionDelta {
    pub fn from_delta(
        capability_delta: &SetDelta,
        effective_capabilities: &BTreeSet<String>,
        _state_delta: Option<&CapabilityStateDelta>,
    ) -> Option<Box<dyn DimensionDelta>> {
        let delta = Self {
            added: capability_delta.added.clone(),
            removed: capability_delta.removed.clone(),
            effective: effective_capabilities.iter().cloned().collect(),
        };
        Some(Box::new(delta))
    }
}

impl DimensionDelta for CapabilityKeyDimensionDelta {
    fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty()
    }

    fn to_section(&self) -> ContextFrameSection {
        ContextFrameSection::CapabilityKeyDelta {
            added_capabilities: self.added.clone(),
            removed_capabilities: self.removed.clone(),
            effective_capabilities: self.effective.clone(),
        }
    }

    fn render_text(&self, phase_node: Option<&str>) -> String {
        let mut sections = vec![match phase_node {
            Some(node) => format!("## Capability State Update — Step Transition: {node}"),
            None => "## Capability State Update".to_string(),
        }];

        if !self.added.is_empty() {
            let mut block = vec!["### Added Capabilities".to_string()];
            for key in &self.added {
                let desc = capability_description(key);
                if desc.is_empty() {
                    block.push(format!("- **{key}**"));
                } else {
                    block.push(format!("- **{key}**: {desc}"));
                }
            }
            sections.push(block.join("\n"));
        }
        if !self.removed.is_empty() {
            let mut block = vec!["### Removed Capabilities".to_string()];
            for key in &self.removed {
                let desc = capability_description(key);
                if desc.is_empty() {
                    block.push(format!("- **{key}**（不再可用）"));
                } else {
                    block.push(format!("- **{key}**: {desc}（不再可用）"));
                }
            }
            sections.push(block.join("\n"));
        }

        let caps_block = if self.effective.is_empty() {
            "- （无）".to_string()
        } else {
            self.effective
                .iter()
                .map(|key| format!("- `{key}`"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        sections.push(format!("### Effective Capabilities\n{caps_block}"));

        sections.join("\n\n")
    }
}
