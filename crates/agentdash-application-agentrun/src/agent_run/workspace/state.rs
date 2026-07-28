/// Projects only Product lifecycle state into the Workspace shell.
///
/// Runtime execution and turn coordinates belong to `AgentRuntimeView`.
pub fn derive_workspace_delivery_status(agent_status: &str) -> String {
    match agent_status {
        "completed" => "completed",
        "failed" => "failed",
        "cancelled" | "interrupted" => "interrupted",
        "lost" => "lost",
        _ => "ready",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_product_state_only_projects_product_delivery() {
        assert_eq!(derive_workspace_delivery_status("active"), "ready");
    }

    #[test]
    fn terminal_product_state_remains_visible_in_workspace_shell() {
        assert_eq!(derive_workspace_delivery_status("failed"), "failed");
    }
}
