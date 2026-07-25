#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunDisplayTitle {
    pub value: String,
    pub source: String,
}

pub fn resolve_agent_run_display_title(
    workspace_title: Option<&str>,
    workspace_title_source: Option<&str>,
) -> AgentRunDisplayTitle {
    if let Some(value) = non_blank(workspace_title) {
        return AgentRunDisplayTitle {
            value: value.to_string(),
            source: non_blank(workspace_title_source)
                .unwrap_or("workspace")
                .to_string(),
        };
    }
    AgentRunDisplayTitle {
        value: "新会话".to_string(),
        source: "pending".to_string(),
    }
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_workspace_title_has_highest_priority() {
        assert_eq!(
            resolve_agent_run_display_title(Some("  显式名称  "), Some("user")),
            AgentRunDisplayTitle {
                value: "显式名称".to_string(),
                source: "user".to_string(),
            }
        );
    }

    #[test]
    fn uninitialized_product_title_stays_pending() {
        assert_eq!(
            resolve_agent_run_display_title(None, None),
            AgentRunDisplayTitle {
                value: "新会话".to_string(),
                source: "pending".to_string(),
            }
        );
    }

    #[test]
    fn missing_explicit_source_uses_workspace_provenance() {
        assert_eq!(
            resolve_agent_run_display_title(Some("名称"), Some(" ")),
            AgentRunDisplayTitle {
                value: "名称".to_string(),
                source: "workspace".to_string(),
            }
        );
    }
}
