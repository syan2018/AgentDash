use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSurfaceQueryPurpose {
    pub component: String,
}

impl RuntimeSurfaceQueryPurpose {
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
        }
    }

    pub fn resource_surface() -> Self {
        Self::new("agent_run_resource_surface")
    }
}

impl From<&str> for RuntimeSurfaceQueryPurpose {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRuntimeAddress {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunTerminalLaunchTarget {
    pub backend_id: String,
    pub mount_root_ref: String,
}
