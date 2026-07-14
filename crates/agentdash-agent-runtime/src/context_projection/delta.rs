use std::collections::BTreeSet;

/// Deterministic set delta used by surface projection families.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextSetDelta<T: Ord> {
    pub added: BTreeSet<T>,
    pub removed: BTreeSet<T>,
}

impl<T: Ord + Clone> ContextSetDelta<T> {
    #[must_use]
    pub fn between(previous: &BTreeSet<T>, target: &BTreeSet<T>) -> Self {
        Self {
            added: target.difference(previous).cloned().collect(),
            removed: previous.difference(target).cloned().collect(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Normalized business-surface delta. Frame-family projectors consume this single result rather
/// than independently re-deriving changes from driver DTOs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextSurfaceDelta {
    pub capability_keys: ContextSetDelta<String>,
    pub tool_paths: ContextSetDelta<String>,
    pub mcp_servers: ContextSetDelta<String>,
    pub skills: ContextSetDelta<String>,
    pub companions: ContextSetDelta<String>,
}

impl ContextSurfaceDelta {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.capability_keys.is_empty()
            && self.tool_paths.is_empty()
            && self.mcp_servers.is_empty()
            && self.skills.is_empty()
            && self.companions.is_empty()
    }
}
