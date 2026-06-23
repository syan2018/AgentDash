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
        if capability_delta.is_empty() {
            return None;
        }
        let delta = Self {
            added: capability_delta.added.clone(),
            removed: capability_delta.removed.clone(),
            effective: effective_capabilities.iter().cloned().collect(),
        };
        Some(Box::new(delta))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_capability_key_delta_does_not_create_section() {
        let effective = BTreeSet::from(["file_read".to_string()]);

        let section = CapabilityKeyDimensionDelta::from_delta(
            &SetDelta::default(),
            &effective,
            Some(&CapabilityStateDelta::default()),
        );

        assert!(section.is_none());
    }

    #[test]
    fn capability_key_delta_with_additions_creates_section() {
        let effective = BTreeSet::from(["file_read".to_string()]);
        let delta = SetDelta {
            added: vec!["file_read".to_string()],
            removed: Vec::new(),
        };

        let section = CapabilityKeyDimensionDelta::from_delta(&delta, &effective, None)
            .expect("non-empty capability key delta should render");

        assert!(section.has_changes());
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
