use std::path::PathBuf;

use crate::tool_executor::ToolError;

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceRootGuard {
    workspace_roots_configured: bool,
    canonical_workspace_roots: Vec<PathBuf>,
}

impl WorkspaceRootGuard {
    pub(crate) fn new(workspace_roots: Vec<PathBuf>) -> Self {
        Self {
            workspace_roots_configured: !workspace_roots.is_empty(),
            canonical_workspace_roots: canonicalize_workspace_roots(workspace_roots),
        }
    }

    pub(crate) fn validate_workspace_root(
        &self,
        workspace_root: &str,
    ) -> Result<PathBuf, ToolError> {
        let trimmed = workspace_root.trim();
        if trimmed.is_empty() {
            return Err(ToolError::InvalidPath(
                "workspace root 不能为空".to_string(),
            ));
        }

        let ws_path = PathBuf::from(trimmed);
        let canonical = std::fs::canonicalize(&ws_path)
            .map_err(|_| ToolError::InvalidPath(workspace_root.to_string()))?;

        if !canonical.is_dir() {
            return Err(ToolError::InvalidPath(format!(
                "workspace root 不是目录: {workspace_root}"
            )));
        }

        if !self.workspace_roots_configured {
            return Ok(canonical);
        }

        for root in &self.canonical_workspace_roots {
            if canonical.starts_with(root) {
                return Ok(canonical);
            }
        }

        Err(ToolError::PathNotAccessible(format!(
            "workspace root 未登记: {workspace_root}"
        )))
    }

    #[cfg(test)]
    pub(crate) fn is_configured(&self) -> bool {
        self.workspace_roots_configured
    }

    #[cfg(test)]
    pub(crate) fn canonical_roots(&self) -> &[PathBuf] {
        &self.canonical_workspace_roots
    }
}

pub(crate) fn canonicalize_workspace_roots(workspace_roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut canonical_roots = Vec::new();
    for root in workspace_roots {
        let Ok(canonical) = std::fs::canonicalize(root) else {
            continue;
        };
        if !canonical.is_dir() || canonical_roots.contains(&canonical) {
            continue;
        }
        canonical_roots.push(canonical);
    }
    canonical_roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_configured_roots_fail_closed() {
        let workspace = tempfile::tempdir().expect("workspace");
        let unavailable_parent = tempfile::tempdir().expect("unavailable parent");
        let guard = WorkspaceRootGuard::new(vec![unavailable_parent.path().join("missing")]);

        let error = guard
            .validate_workspace_root(workspace.path().to_string_lossy().as_ref())
            .expect_err("unavailable configured roots should fail closed");

        assert!(guard.is_configured());
        assert!(guard.canonical_roots().is_empty());
        assert!(matches!(error, ToolError::PathNotAccessible(_)));
    }
}
