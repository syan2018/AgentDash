use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const CANVAS_SYSTEM_SKILL_NAME: &str = "canvas-system";
pub const CANVAS_SYSTEM_SKILL_PATH: &str = "skills/canvas-system/SKILL.md";
const CANVAS_SYSTEM_SKILL_CONTENT: &str = include_str!("skills/canvas-system/SKILL.md");

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

    pub fn default_canvas_system_skill() -> Self {
        Self {
            path: CANVAS_SYSTEM_SKILL_PATH.to_string(),
            content: canvas_system_skill_content().to_string(),
        }
    }
}

pub fn ensure_canvas_system_skill(files: &mut Vec<CanvasFile>) -> bool {
    let content = canvas_system_skill_content();
    if let Some(file) = files
        .iter_mut()
        .find(|file| file.path == CANVAS_SYSTEM_SKILL_PATH)
    {
        if file.content == content {
            return false;
        }
        file.content = content.to_string();
        return true;
    }

    files.push(CanvasFile::default_canvas_system_skill());
    true
}

pub fn is_canvas_system_skill_path(path: &str) -> bool {
    path.trim().replace('\\', "/").trim_matches('/') == CANVAS_SYSTEM_SKILL_PATH
}

pub fn canvas_system_skill_content() -> &'static str {
    CANVAS_SYSTEM_SKILL_CONTENT
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasDataBinding {
    pub alias: String,
    pub source_uri: String,
    #[serde(default = "default_binding_content_type")]
    pub content_type: String,
}

impl CanvasDataBinding {
    pub fn new(alias: String, source_uri: String) -> Self {
        Self {
            alias,
            source_uri,
            content_type: default_binding_content_type(),
        }
    }
}

fn default_binding_content_type() -> String {
    "application/json".to_string()
}
