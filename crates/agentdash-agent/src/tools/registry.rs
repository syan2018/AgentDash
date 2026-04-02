use std::collections::HashMap;
use std::sync::Arc;

use crate::types::{AgentTool, DynAgentTool};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, DynAgentTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T>(&mut self, tool: T)
    where
        T: AgentTool + 'static,
    {
        self.register_dyn(Arc::new(tool));
    }

    pub fn register_dyn(&mut self, tool: DynAgentTool) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<DynAgentTool> {
        self.tools.get(name).cloned()
    }

    pub fn list(&self) -> Vec<ToolInfo> {
        let mut items = self
            .tools
            .values()
            .map(|tool| ToolInfo {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.name.cmp(&right.name));
        items
    }

    pub fn all(&self) -> Vec<DynAgentTool> {
        let mut items = self.tools.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| left.name().cmp(right.name()));
        items
    }
}

