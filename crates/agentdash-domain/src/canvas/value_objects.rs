use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::DomainError;
use crate::embedded_skill::{EmbeddedSkillBundle, EmbeddedSkillFile, EmbeddedSkillFileKind};

pub const CANVAS_SYSTEM_SKILL_NAME: &str = "canvas-system";
const CANVAS_SYSTEM_SKILL_CONTENT: &str = include_str!("skills/canvas-system/SKILL.md");
const CANVAS_SYSTEM_RUNTIME_BRIDGE_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/runtime-bridge.md");
const CANVAS_SYSTEM_RUNTIME_ACTIONS_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/runtime-actions.md");
const CANVAS_SYSTEM_VFS_ASSETS_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/vfs-assets.md");
const CANVAS_SYSTEM_INTERACTION_STATE_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/interaction-state.md");
const CANVAS_SYSTEM_AGENT_SUBMIT_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/agent-submit.md");
const CANVAS_SYSTEM_AGENT_SIDE_INTERFACES_REFERENCE_CONTENT: &str =
    include_str!("skills/canvas-system/references/agent-side-interfaces.md");
const CANVAS_SYSTEM_BUNDLE_FILES: &[EmbeddedSkillFile] = &[
    EmbeddedSkillFile {
        relative_path: "SKILL.md",
        content: CANVAS_SYSTEM_SKILL_CONTENT,
        kind: EmbeddedSkillFileKind::Skill,
    },
    EmbeddedSkillFile {
        relative_path: "references/runtime-bridge.md",
        content: CANVAS_SYSTEM_RUNTIME_BRIDGE_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/runtime-actions.md",
        content: CANVAS_SYSTEM_RUNTIME_ACTIONS_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/vfs-assets.md",
        content: CANVAS_SYSTEM_VFS_ASSETS_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/interaction-state.md",
        content: CANVAS_SYSTEM_INTERACTION_STATE_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/agent-submit.md",
        content: CANVAS_SYSTEM_AGENT_SUBMIT_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
    EmbeddedSkillFile {
        relative_path: "references/agent-side-interfaces.md",
        content: CANVAS_SYSTEM_AGENT_SIDE_INTERFACES_REFERENCE_CONTENT,
        kind: EmbeddedSkillFileKind::Reference,
    },
];

pub const CANVAS_SYSTEM_BUNDLE: EmbeddedSkillBundle = EmbeddedSkillBundle {
    name: CANVAS_SYSTEM_SKILL_NAME,
    root_path: "skills/canvas-system",
    entry_path: "SKILL.md",
    files: CANVAS_SYSTEM_BUNDLE_FILES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CanvasScope {
    #[default]
    Personal,
    Project,
}

impl CanvasScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Project => "project",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw.trim() {
            "personal" => Ok(Self::Personal),
            "project" => Ok(Self::Project),
            value => Err(DomainError::InvalidConfig(format!(
                "Canvas scope `{value}` 无效"
            ))),
        }
    }
}


impl std::fmt::Display for CanvasScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasAccessAction {
    View,
    EditSource,
    Publish,
    ManageShared,
    Copy,
    RuntimeWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CanvasAccessProjection {
    pub can_view: bool,
    pub can_edit_source: bool,
    pub can_publish: bool,
    pub can_manage_shared: bool,
    pub can_copy: bool,
    pub runtime_write_allowed: bool,
}

impl CanvasAccessProjection {
    pub fn allows(&self, action: CanvasAccessAction) -> bool {
        match action {
            CanvasAccessAction::View => self.can_view,
            CanvasAccessAction::EditSource => self.can_edit_source,
            CanvasAccessAction::Publish => self.can_publish,
            CanvasAccessAction::ManageShared => self.can_manage_shared,
            CanvasAccessAction::Copy => self.can_copy,
            CanvasAccessAction::RuntimeWrite => self.runtime_write_allowed,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CanvasSandboxConfig {
    #[serde(default)]
    pub libraries: Vec<String>,
    #[serde(default)]
    pub import_map: CanvasImportMap,
}

impl CanvasSandboxConfig {
    pub fn react_default() -> Self {
        let mut imports = BTreeMap::new();
        imports.insert(
            "react".to_string(),
            "https://esm.sh/react@18?dev".to_string(),
        );
        imports.insert(
            "react-dom/client".to_string(),
            "https://esm.sh/react-dom@18/client?dev".to_string(),
        );

        Self {
            libraries: vec!["react".to_string(), "react-dom/client".to_string()],
            import_map: CanvasImportMap { imports },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CanvasImportMap {
    #[serde(default)]
    pub imports: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasFile {
    pub path: String,
    pub content: String,
}

impl CanvasFile {
    pub fn new(path: String, content: String) -> Self {
        Self { path, content }
    }

    pub fn default_entry() -> Self {
        Self {
            path: "src/main.tsx".to_string(),
            content: r#"const root = document.getElementById("root");

if (!root) {
  throw new Error("Canvas root element not found");
}

root.innerHTML = `
  <section style="font-family: sans-serif; padding: 16px;">
    <h1 style="margin: 0 0 8px;">Live Canvas Ready</h1>
    <p style="margin: 0; color: #475569;">
      Start editing <code>src/main.tsx</code> to render your canvas.
    </p>
  </section>
`;
"#
            .to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanvasDataBinding {
    pub alias: String,
    pub source_uri: String,
    #[serde(default)]
    pub content_type: String,
}

impl CanvasDataBinding {
    pub fn new(alias: String, source_uri: String) -> Self {
        Self::with_content_type(alias, source_uri, None)
    }

    pub fn with_content_type(
        alias: String,
        source_uri: String,
        content_type: Option<String>,
    ) -> Self {
        let content_type = normalize_binding_content_type(content_type.as_deref(), &source_uri);
        Self {
            alias,
            source_uri,
            content_type,
        }
    }

    pub fn data_path(&self) -> String {
        canvas_binding_data_path(&self.alias, &self.content_type, &self.source_uri)
    }

    pub fn placeholder_content(&self) -> &'static str {
        if content_type_base(&self.content_type) == "application/json" {
            "null"
        } else {
            ""
        }
    }
}

pub fn canvas_binding_data_path(alias: &str, content_type: &str, source_uri: &str) -> String {
    let extension =
        extension_for_content_type(content_type).or_else(|| source_uri_extension(source_uri));
    format!(
        "bindings/{}.{}",
        alias.trim(),
        extension.unwrap_or_else(|| "txt".to_string())
    )
}

pub fn normalize_binding_content_type(content_type: Option<&str>, source_uri: &str) -> String {
    let explicit = content_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(content_type_base);
    explicit.unwrap_or_else(|| infer_binding_content_type(source_uri))
}

pub fn is_text_compatible_binding_content_type(content_type: &str) -> bool {
    let base = content_type_base(content_type);
    base.starts_with("text/")
        || base == "application/json"
        || base.ends_with("+json")
        || base == "application/x-ndjson"
        || base == "image/svg+xml"
        || base == "application/xml"
        || base.ends_with("+xml")
        || base == "application/yaml"
        || base == "application/x-yaml"
        || base == "application/javascript"
        || base == "application/ecmascript"
        || base == "application/x-javascript"
}

pub fn infer_binding_content_type(source_uri: &str) -> String {
    match source_uri_extension(source_uri).as_deref() {
        Some("json") => "application/json",
        Some("ndjson") => "application/x-ndjson",
        Some("csv") => "text/csv",
        Some("md") | Some("markdown") => "text/markdown",
        Some("html") | Some("htm") => "text/html",
        Some("css") => "text/css",
        Some("js") | Some("mjs") | Some("cjs") => "text/javascript",
        Some("ts") | Some("tsx") => "text/typescript",
        Some("jsx") => "text/jsx",
        Some("svg") => "image/svg+xml",
        Some("txt") | Some("log") => "text/plain",
        Some("xml") => "application/xml",
        Some("yaml") | Some("yml") => "application/yaml",
        _ => "text/plain",
    }
    .to_string()
}

fn extension_for_content_type(content_type: &str) -> Option<String> {
    let base = content_type_base(content_type);
    let extension = match base.as_str() {
        "application/json" => "json",
        value if value.ends_with("+json") => "json",
        "application/x-ndjson" => "ndjson",
        "text/csv" => "csv",
        "text/markdown" => "md",
        "text/html" => "html",
        "text/css" => "css",
        "text/javascript"
        | "application/javascript"
        | "application/x-javascript"
        | "text/ecmascript"
        | "application/ecmascript" => "js",
        "text/typescript" => "ts",
        "text/jsx" => "jsx",
        "text/tsx" => "tsx",
        "image/svg+xml" => "svg",
        "text/plain" => "txt",
        "application/xml" | "text/xml" => "xml",
        "application/yaml" | "application/x-yaml" | "text/yaml" => "yaml",
        _ => return None,
    };
    Some(extension.to_string())
}

fn source_uri_extension(source_uri: &str) -> Option<String> {
    let without_fragment = source_uri.split('#').next().unwrap_or(source_uri);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let file_name = without_query
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(without_query);
    let extension = file_name.rsplit_once('.')?.1;
    let extension = extension.trim().to_ascii_lowercase();
    if extension
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        && !extension.is_empty()
    {
        Some(extension)
    } else {
        None
    }
}

fn content_type_base(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_content_type_is_inferred_from_source_uri_extension() {
        let binding = CanvasDataBinding::new(
            "stats".to_string(),
            "main://reports/stats.csv?download=1".to_string(),
        );

        assert_eq!(binding.content_type, "text/csv");
        assert_eq!(binding.data_path(), "bindings/stats.csv");
    }

    #[test]
    fn explicit_binding_content_type_controls_data_path() {
        let binding = CanvasDataBinding::with_content_type(
            "summary".to_string(),
            "main://reports/summary".to_string(),
            Some("Text/Markdown; charset=utf-8".to_string()),
        );

        assert_eq!(binding.content_type, "text/markdown");
        assert_eq!(binding.data_path(), "bindings/summary.md");
    }

    #[test]
    fn text_compatible_binding_content_types_include_structured_text_assets() {
        for content_type in [
            "image/svg+xml",
            "application/xml",
            "application/yaml",
            "application/javascript",
        ] {
            assert!(is_text_compatible_binding_content_type(content_type));
        }
        assert!(!is_text_compatible_binding_content_type("image/png"));
    }
}
