use std::fs;
use std::path::{Path, PathBuf};

use agentdash_domain::context_source::{ContextSlot, ContextSourceKind, ContextSourceRef};
use walkdir::WalkDir;

use crate::composer::{ContextFragment, MergeStrategy};
use crate::error::InjectionError;

const MAX_SOURCE_FILE_BYTES: u64 = 1_000_000;
const DEFAULT_TRUNCATE_CHARS: usize = 12_000;

pub struct ResolveSourcesRequest<'a> {
    pub sources: &'a [ContextSourceRef],
    pub workspace_root: Option<&'a Path>,
    pub base_order: i32,
}

pub struct ResolveSourcesOutput {
    pub fragments: Vec<ContextFragment>,
    pub warnings: Vec<String>,
}

pub trait SourceResolver: Send + Sync {
    fn resolve(
        &self,
        source: &ContextSourceRef,
        workspace_root: Option<&Path>,
        order: i32,
    ) -> Result<ContextFragment, InjectionError>;
}

/// 来源解析器注册表 — 按 ContextSourceKind 注册解析器
///
/// 内置解析器在创建时自动注册，外部可通过 `register` 扩展新的来源类型。
pub struct SourceResolverRegistry {
    resolvers: std::collections::HashMap<ContextSourceKind, Box<dyn SourceResolver>>,
}

impl SourceResolverRegistry {
    /// 创建包含内置解析器的注册表
    pub fn with_builtins() -> Self {
        let mut registry = Self {
            resolvers: std::collections::HashMap::new(),
        };
        registry.register(ContextSourceKind::ManualText, Box::new(ManualTextResolver));
        registry.register(ContextSourceKind::File, Box::new(FileResolver));
        registry.register(
            ContextSourceKind::ProjectSnapshot,
            Box::new(ProjectSnapshotResolver),
        );
        registry
    }

    /// 注册新的来源解析器
    pub fn register(&mut self, kind: ContextSourceKind, resolver: Box<dyn SourceResolver>) {
        self.resolvers.insert(kind, resolver);
    }

    /// 查找指定 kind 的解析器
    pub fn get(&self, kind: &ContextSourceKind) -> Option<&dyn SourceResolver> {
        self.resolvers.get(kind).map(|r| r.as_ref())
    }

    pub fn supported_kinds(&self) -> Vec<&ContextSourceKind> {
        self.resolvers.keys().collect()
    }
}

impl Default for SourceResolverRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

pub fn resolve_declared_sources(
    request: ResolveSourcesRequest<'_>,
) -> Result<ResolveSourcesOutput, InjectionError> {
    resolve_declared_sources_with_registry(request, &SourceResolverRegistry::with_builtins())
}

/// 使用指定注册表解析声明式上下文来源
pub fn resolve_declared_sources_with_registry(
    request: ResolveSourcesRequest<'_>,
    registry: &SourceResolverRegistry,
) -> Result<ResolveSourcesOutput, InjectionError> {
    let mut indexed_sources = request.sources.iter().enumerate().collect::<Vec<_>>();
    indexed_sources.sort_by(|(left_index, left), (right_index, right)| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left_index.cmp(right_index))
    });

    let mut fragments = Vec::new();
    let mut warnings = Vec::new();

    for (position, source) in indexed_sources
        .into_iter()
        .map(|(_, source)| source)
        .enumerate()
    {
        let order = request.base_order + position as i32;
        let resolver = registry.get(&source.kind);

        let resolved = match resolver {
            Some(r) => r.resolve(source, request.workspace_root, order),
            None => {
                let msg = format!(
                    "source `{}` 的类型 {:?} 暂无已注册的解析器",
                    display_source_label(source),
                    source.kind
                );
                if source.required {
                    return Err(InjectionError::MissingWorkspace(msg));
                }
                warnings.push(msg);
                continue;
            }
        };

        match resolved {
            Ok(fragment) => fragments.push(fragment),
            Err(err) if source.required => return Err(err),
            Err(err) => warnings.push(format!(
                "source `{}` 已跳过: {err}",
                display_source_label(source)
            )),
        }
    }

    Ok(ResolveSourcesOutput {
        fragments,
        warnings,
    })
}

struct ManualTextResolver;

impl SourceResolver for ManualTextResolver {
    fn resolve(
        &self,
        source: &ContextSourceRef,
        _workspace_root: Option<&Path>,
        order: i32,
    ) -> Result<ContextFragment, InjectionError> {
        Ok(ContextFragment {
            slot: fragment_slot(&source.slot),
            label: fragment_label(&source.kind),
            order,
            strategy: MergeStrategy::Append,
            content: render_source_section(source, source.locator.clone()),
        })
    }
}

struct FileResolver;

impl SourceResolver for FileResolver {
    fn resolve(
        &self,
        source: &ContextSourceRef,
        workspace_root: Option<&Path>,
        order: i32,
    ) -> Result<ContextFragment, InjectionError> {
        let path = resolve_path(&source.locator, workspace_root)?;
        let metadata = fs::metadata(&path)?;
        if metadata.len() > MAX_SOURCE_FILE_BYTES {
            return Err(InjectionError::SourceTooLarge {
                path,
                size: metadata.len(),
            });
        }

        let raw = fs::read_to_string(&path)?;
        let formatted = format_file_like_read_tool(&path, &raw, workspace_root);

        Ok(ContextFragment {
            slot: fragment_slot(&source.slot),
            label: fragment_label(&source.kind),
            order,
            strategy: MergeStrategy::Append,
            content: render_source_section(source, truncate_text(formatted, source.max_chars)),
        })
    }
}

struct ProjectSnapshotResolver;

impl SourceResolver for ProjectSnapshotResolver {
    fn resolve(
        &self,
        source: &ContextSourceRef,
        workspace_root: Option<&Path>,
        order: i32,
    ) -> Result<ContextFragment, InjectionError> {
        let root = workspace_root.ok_or_else(|| {
            InjectionError::MissingWorkspace(display_source_label(source).to_string())
        })?;
        let content = build_project_snapshot(root, source.max_chars);

        Ok(ContextFragment {
            slot: fragment_slot(&source.slot),
            label: fragment_label(&source.kind),
            order,
            strategy: MergeStrategy::Append,
            content: render_source_section(source, content),
        })
    }
}

fn resolve_path(locator: &str, workspace_root: Option<&Path>) -> Result<PathBuf, InjectionError> {
    let candidate = PathBuf::from(locator);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        let root =
            workspace_root.ok_or_else(|| InjectionError::MissingWorkspace(locator.to_string()))?;
        root.join(candidate)
    };

    if path.exists() {
        Ok(path)
    } else {
        Err(InjectionError::PathNotFound(path))
    }
}

fn build_project_snapshot(root: &Path, max_chars: Option<usize>) -> String {
    let tech_stack = detect_tech_stack(root);
    let entries = WalkDir::new(root)
        .max_depth(2)
        .into_iter()
        .filter_entry(|entry| !is_ignored_dir(entry.path()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != root)
        .take(48)
        .map(|entry| {
            let rel = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .display()
                .to_string();
            let suffix = if entry.file_type().is_dir() { "/" } else { "" };
            format!("- {}{}", rel.replace('\\', "/"), suffix)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let content = format!(
        "## 项目快照\n- root: {}\n- tech_stack: {}\n\n## 目录摘要\n{}",
        root.display(),
        tech_stack.join(", "),
        entries
    );

    truncate_text(content, max_chars)
}

fn detect_tech_stack(root: &Path) -> Vec<&'static str> {
    let mut stack = Vec::new();
    if root.join("Cargo.toml").exists() {
        stack.push("Rust");
    }
    if root.join("package.json").exists() {
        stack.push("Node.js");
    }
    if root.join("pnpm-lock.yaml").exists() {
        stack.push("pnpm");
    }
    if root.join("playwright.config.ts").exists() {
        stack.push("Playwright");
    }
    if stack.is_empty() {
        stack.push("unknown");
    }
    stack
}

fn is_ignored_dir(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    matches!(name, "node_modules" | "target" | ".git" | ".next" | "dist")
}

fn render_source_section(source: &ContextSourceRef, content: String) -> String {
    let title = display_source_label(source);
    format!("## 来源: {title}\n{content}")
}

fn format_file_like_read_tool(path: &Path, content: &str, workspace_root: Option<&Path>) -> String {
    let display_path = workspace_root
        .and_then(|root| path.strip_prefix(root).ok())
        .map(|rel| rel.display().to_string())
        .unwrap_or_else(|| path.display().to_string())
        .replace('\\', "/");

    let numbered = content
        .lines()
        .enumerate()
        .map(|(index, line)| format!("{:>4} | {}", index + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    if numbered.is_empty() {
        format!("文件: {display_path}\n   1 | ")
    } else {
        format!("文件: {display_path}\n{numbered}")
    }
}

fn truncate_text(content: String, max_chars: Option<usize>) -> String {
    let max = max_chars.unwrap_or(DEFAULT_TRUNCATE_CHARS);
    if content.chars().count() <= max {
        return content;
    }

    let truncated = content.chars().take(max).collect::<String>();
    format!("{truncated}\n\n> 内容已截断")
}

fn display_source_label(source: &ContextSourceRef) -> &str {
    source.label.as_deref().unwrap_or(source.locator.as_str())
}

fn fragment_label(kind: &ContextSourceKind) -> &'static str {
    match kind {
        ContextSourceKind::ManualText => "declared_manual_text",
        ContextSourceKind::File => "declared_file_source",
        ContextSourceKind::ProjectSnapshot => "declared_project_snapshot",
        ContextSourceKind::HttpFetch => "declared_http_fetch",
        ContextSourceKind::McpResource => "declared_mcp_resource",
        ContextSourceKind::EntityRef => "declared_entity_ref",
    }
}

fn fragment_slot(slot: &ContextSlot) -> &'static str {
    match slot {
        ContextSlot::Requirements => "requirements",
        ContextSlot::Constraints => "constraints",
        ContextSlot::Codebase => "codebase",
        ContextSlot::References => "references",
        ContextSlot::InstructionAppend => "instruction_append",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use agentdash_domain::context_source::{
        ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
    };

    use super::*;

    #[test]
    fn resolves_manual_text_source() {
        let result = resolve_declared_sources(ResolveSourcesRequest {
            sources: &[ContextSourceRef {
                kind: ContextSourceKind::ManualText,
                locator: "hello world".to_string(),
                label: Some("manual note".to_string()),
                slot: ContextSlot::Requirements,
                priority: 100,
                required: true,
                max_chars: None,
                delivery: ContextDelivery::Resource,
            }],
            workspace_root: None,
            base_order: 10,
        })
        .expect("manual text should resolve");

        assert_eq!(result.fragments.len(), 1);
        assert!(result.fragments[0].content.contains("manual note"));
        assert!(result.fragments[0].content.contains("hello world"));
    }

    #[test]
    fn resolves_file_source_relative_to_workspace() {
        let temp = tempfile::tempdir().expect("temp dir");
        fs::write(temp.path().join("notes.md"), "# hello\nworld").expect("write source file");

        let result = resolve_declared_sources(ResolveSourcesRequest {
            sources: &[ContextSourceRef {
                kind: ContextSourceKind::File,
                locator: "notes.md".to_string(),
                label: None,
                slot: ContextSlot::References,
                priority: 1,
                required: true,
                max_chars: None,
                delivery: ContextDelivery::Resource,
            }],
            workspace_root: Some(temp.path()),
            base_order: 20,
        })
        .expect("file source should resolve");

        assert!(result.fragments[0].content.contains("文件: notes.md"));
        assert!(result.fragments[0].content.contains("1 | # hello"));
        assert!(result.fragments[0].content.contains("2 | world"));
    }

    #[test]
    fn resolves_typescript_file_source_like_read_tool() {
        let temp = tempfile::tempdir().expect("temp dir");
        fs::write(
            temp.path().join("StoryPage.tsx"),
            "export function StoryPage() {\n  return null;\n}\n",
        )
        .expect("write tsx source file");

        let result = resolve_declared_sources(ResolveSourcesRequest {
            sources: &[ContextSourceRef {
                kind: ContextSourceKind::File,
                locator: "StoryPage.tsx".to_string(),
                label: Some("Story 页面".to_string()),
                slot: ContextSlot::References,
                priority: 10,
                required: true,
                max_chars: None,
                delivery: ContextDelivery::Resource,
            }],
            workspace_root: Some(temp.path()),
            base_order: 20,
        })
        .expect("tsx source should resolve");

        assert!(result.fragments[0].content.contains("文件: StoryPage.tsx"));
        assert!(
            result.fragments[0]
                .content
                .contains("1 | export function StoryPage() {")
        );
        assert!(result.fragments[0].content.contains("2 |   return null;"));
    }

    #[test]
    fn resolves_project_snapshot_source() {
        let temp = tempfile::tempdir().expect("temp dir");
        fs::write(temp.path().join("Cargo.toml"), "[package]\nname='demo'\n").expect("write cargo");
        fs::create_dir(temp.path().join("src")).expect("create src");
        fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").expect("write main");

        let result = resolve_declared_sources(ResolveSourcesRequest {
            sources: &[ContextSourceRef {
                kind: ContextSourceKind::ProjectSnapshot,
                locator: ".".to_string(),
                label: Some("workspace snapshot".to_string()),
                slot: ContextSlot::Codebase,
                priority: 1,
                required: true,
                max_chars: None,
                delivery: ContextDelivery::Resource,
            }],
            workspace_root: Some(temp.path()),
            base_order: 30,
        })
        .expect("project snapshot should resolve");

        assert!(result.fragments[0].content.contains("workspace snapshot"));
        assert!(result.fragments[0].content.contains("Rust"));
        assert!(result.fragments[0].content.contains("src/"));
    }
}
