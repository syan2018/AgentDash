use agentdash_platform_spi::platform::tool_capability::{
    CAP_RELAY_MANAGEMENT, CAP_STORY_MANAGEMENT, CAP_WORKFLOW_MANAGEMENT,
};

pub fn capability_key_for_mcp_server_name(server_name: &str) -> String {
    match agent_facing_mcp_server_name(server_name).as_str() {
        "agentdash-relay-tools" => CAP_RELAY_MANAGEMENT.to_string(),
        "agentdash-story-tools" => CAP_STORY_MANAGEMENT.to_string(),
        "agentdash-workflow-tools" => CAP_WORKFLOW_MANAGEMENT.to_string(),
        other => format!("mcp:{other}"),
    }
}

pub fn namespaced_tool_name(server_name: &str, tool_name: &str) -> String {
    let agent_facing_server = agent_facing_mcp_server_name(server_name);
    format!(
        "mcp_{}_{}",
        sanitize_identifier(&agent_facing_server),
        sanitize_identifier(tool_name)
    )
}

pub fn agent_facing_mcp_server_name(server_name: &str) -> String {
    const PLATFORM_SCOPED_PREFIXES: &[(&str, &str)] = &[
        ("agentdash-story-tools-", "agentdash-story-tools"),
        ("agentdash-workflow-tools-", "agentdash-workflow-tools"),
    ];

    for (prefix, stable_name) in PLATFORM_SCOPED_PREFIXES {
        if server_name.starts_with(prefix) {
            return (*stable_name).to_string();
        }
    }

    server_name.to_string()
}

pub(crate) fn sanitize_identifier(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_name_hides_platform_scope_ids() {
        assert_eq!(
            namespaced_tool_name("agentdash-workflow-tools-8de613e7", "get_lifecycle"),
            "mcp_agentdash_workflow_tools_get_lifecycle"
        );
    }

    #[test]
    fn namespaced_name_keeps_custom_server_namespace() {
        assert_eq!(
            namespaced_tool_name("code-analyzer", "scan_repo"),
            "mcp_code_analyzer_scan_repo"
        );
    }

    #[test]
    fn platform_mcp_server_names_map_to_capability_keys() {
        assert_eq!(
            capability_key_for_mcp_server_name("agentdash-workflow-tools-8de613e7"),
            "workflow_management"
        );
        assert_eq!(
            capability_key_for_mcp_server_name("code-analyzer"),
            "mcp:code-analyzer"
        );
    }
}
