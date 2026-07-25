use serde_json::Value;

use crate::common::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentInputOrigin {
    User,
    System,
    Hook,
    Companion,
    Workflow,
}

impl AgentInputOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
            Self::Hook => "hook",
            Self::Companion => "companion",
            Self::Workflow => "workflow",
        }
    }
}

impl TryFrom<&str> for AgentInputOrigin {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            "hook" => Ok(Self::Hook),
            "companion" => Ok(Self::Companion),
            "workflow" => Ok(Self::Workflow),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_input.origin 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentInputSourceIdentity {
    pub namespace: String,
    pub kind: String,
    pub source_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub actor: String,
    pub route: Option<String>,
    pub display_label_key: String,
    pub metadata: Option<Value>,
}

impl AgentInputSourceIdentity {
    pub fn new(
        namespace: impl Into<String>,
        kind: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let namespace = namespace.into();
        let kind = kind.into();
        Self {
            display_label_key: format!("agent_input.source.{namespace}.{kind}"),
            namespace,
            kind,
            source_ref: None,
            correlation_ref: None,
            actor: actor.into(),
            route: None,
            metadata: None,
        }
    }

    pub fn with_source_ref(mut self, source_ref: impl Into<String>) -> Self {
        self.source_ref = Some(source_ref.into());
        self
    }

    pub fn with_correlation_ref(mut self, correlation_ref: impl Into<String>) -> Self {
        self.correlation_ref = Some(correlation_ref.into());
        self
    }

    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    pub fn with_display_label_key(mut self, display_label_key: impl Into<String>) -> Self {
        self.display_label_key = display_label_key.into();
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn dedup_fragment(&self) -> String {
        format!("{}:{}", self.namespace, self.kind)
    }

    pub fn composer() -> Self {
        Self::new("core", "composer", "user")
    }

    pub fn draft_start() -> Self {
        Self::new("core", "draft_start", "user")
    }

    pub fn hook_after_turn() -> Self {
        Self::new("core", "hook_after_turn", "system")
    }

    pub fn hook_before_stop() -> Self {
        Self::new("core", "hook_before_stop", "system")
    }

    pub fn hook_auto_resume() -> Self {
        Self::new("core", "hook_auto_resume", "system")
    }

    pub fn companion_parent_resume() -> Self {
        Self::new("companion", "parent_resume", "agent").with_route("parent")
    }

    pub fn workflow_orchestrator() -> Self {
        Self::new("workflow", "orchestrator", "system")
    }

    pub fn routine_trigger() -> Self {
        Self::new("routine", "trigger", "routine")
    }

    pub fn canvas_action() -> Self {
        Self::new("core", "canvas_action", "user")
    }
}
