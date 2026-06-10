use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileDiscoveryIntent {
    ImplicitWorkspaceScan,
    ExplicitSubtreeScan,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FileDiscoveryPolicy {
    intent: FileDiscoveryIntent,
}

const HARD_EXCLUDE_DIRS: &[&str] = &[".git", ".svn", ".hg", ".bzr", ".jj", ".sl"];
const BUILTIN_NOISE_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".venv",
    "__pycache__",
];

impl FileDiscoveryPolicy {
    pub(crate) fn from_base(base: &Path, workspace_root: &Path) -> Self {
        let intent = if base == workspace_root {
            FileDiscoveryIntent::ImplicitWorkspaceScan
        } else {
            FileDiscoveryIntent::ExplicitSubtreeScan
        };
        Self { intent }
    }

    #[cfg(test)]
    pub(crate) fn implicit_workspace_scan() -> Self {
        Self {
            intent: FileDiscoveryIntent::ImplicitWorkspaceScan,
        }
    }

    #[cfg(test)]
    pub(crate) fn explicit_subtree_scan() -> Self {
        Self {
            intent: FileDiscoveryIntent::ExplicitSubtreeScan,
        }
    }

    pub(crate) fn respects_workspace_ignores(self) -> bool {
        self.intent == FileDiscoveryIntent::ImplicitWorkspaceScan
    }

    pub(crate) fn is_implicit_workspace_scan(self) -> bool {
        self.intent == FileDiscoveryIntent::ImplicitWorkspaceScan
    }

    pub(crate) fn allows_path(self, path: &Path, workspace_root: &Path) -> bool {
        if path_has_named_segment(path, workspace_root, HARD_EXCLUDE_DIRS) {
            return false;
        }
        if self.is_implicit_workspace_scan()
            && path_has_named_segment(path, workspace_root, BUILTIN_NOISE_DIRS)
        {
            return false;
        }
        true
    }

    pub(crate) fn exclude_dir_names(self) -> Vec<&'static str> {
        let mut dirs = HARD_EXCLUDE_DIRS.to_vec();
        if self.is_implicit_workspace_scan() {
            dirs.extend(BUILTIN_NOISE_DIRS);
        }
        dirs
    }
}

fn path_has_named_segment(path: &Path, workspace_root: &Path, names: &[&str]) -> bool {
    let relative = path.strip_prefix(workspace_root).unwrap_or(path);
    relative.components().any(|component| {
        let std::path::Component::Normal(segment) = component else {
            return false;
        };
        segment_matches(segment, names)
    })
}

fn segment_matches(segment: &std::ffi::OsStr, names: &[&str]) -> bool {
    segment
        .to_str()
        .is_some_and(|value| names.iter().any(|name| value.eq_ignore_ascii_case(name)))
}
