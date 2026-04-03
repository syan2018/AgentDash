use std::collections::BTreeSet;
use std::path::{Component, Path};

use agentdash_domain::context_source::{ContextSlot, ContextSourceKind, ContextSourceRef};
use agentdash_domain::workspace::Workspace;
use agentdash_injection::{ContextFragment, MergeStrategy, ResolveSourcesOutput};

use crate::address_space::{
    ListOptions, RelayAddressSpaceService, ResourceRef, selected_workspace_binding,
};
use crate::runtime::RuntimeFileEntry;
use crate::workspace::BackendAvailability;

/// 解析 Story/Task 上声明式来源（File / ProjectSnapshot）为具体上下文片段
///
/// 需要 Workspace 在线才能读取远端文件；如果 Backend 不在线或 Workspace 缺失，
/// 非 required 来源会生成 warning 而非报错。
pub async fn resolve_workspace_declared_sources(
    availability: &dyn BackendAvailability,
    address_space_service: &RelayAddressSpaceService,
    sources: &[ContextSourceRef],
    workspace: Option<&Workspace>,
    base_order: i32,
) -> Result<ResolveSourcesOutput, String> {
    let indexed_sources = sorted_sources(sources)
        .into_iter()
        .filter(|source| {
            matches!(
                source.kind,
                ContextSourceKind::File | ContextSourceKind::ProjectSnapshot
            )
        })
        .collect::<Vec<_>>();

    if indexed_sources.is_empty() {
        return Ok(ResolveSourcesOutput {
            fragments: Vec::new(),
            warnings: Vec::new(),
        });
    }

    let Some(workspace) = workspace else {
        return resolve_workspace_source_unavailable(
            &indexed_sources,
            "声明式来源依赖 Workspace，但当前上下文未绑定可用 Workspace",
        );
    };

    let backend_id = match normalize_workspace_backend_id(workspace) {
        Ok(backend_id) => backend_id,
        Err(err) => return resolve_workspace_source_unavailable(&indexed_sources, &err),
    };
    if !availability.is_online(backend_id).await {
        return resolve_workspace_source_unavailable(
            &indexed_sources,
            &format!("Workspace 所属 Backend 当前不在线: {backend_id}"),
        );
    }

    let mut fragments = Vec::new();
    let mut warnings = Vec::new();

    for (position, source) in indexed_sources.into_iter().enumerate() {
        let order = base_order + position as i32;
        let resolved = match source.kind {
            ContextSourceKind::File => {
                resolve_workspace_file_source(address_space_service, workspace, source, order).await
            }
            ContextSourceKind::ProjectSnapshot => {
                resolve_workspace_snapshot_source(address_space_service, workspace, source, order)
                    .await
            }
            _ => continue,
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

fn resolve_workspace_source_unavailable(
    sources: &[&ContextSourceRef],
    message: &str,
) -> Result<ResolveSourcesOutput, String> {
    if sources.iter().any(|source| source.required) {
        return Err(message.to_string());
    }
    Ok(ResolveSourcesOutput {
        fragments: Vec::new(),
        warnings: sources
            .iter()
            .map(|source| {
                format!(
                    "source `{}` 已跳过: {message}",
                    display_source_label(source)
                )
            })
            .collect(),
    })
}

fn sorted_sources(sources: &[ContextSourceRef]) -> Vec<&ContextSourceRef> {
    let mut indexed_sources = sources.iter().enumerate().collect::<Vec<_>>();
    indexed_sources.sort_by(|(left_index, left), (right_index, right)| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left_index.cmp(right_index))
    });
    indexed_sources
        .into_iter()
        .map(|(_, source)| source)
        .collect()
}

fn normalize_workspace_backend_id(workspace: &Workspace) -> Result<&str, String> {
    let backend_id = selected_workspace_binding(workspace)
        .map(|binding| binding.backend_id.trim())
        .unwrap_or("");
    if backend_id.is_empty() {
        Err("Workspace 当前没有可用 binding.backend_id".to_string())
    } else {
        Ok(backend_id)
    }
}

async fn resolve_workspace_file_source(
    address_space_service: &RelayAddressSpaceService,
    workspace: &Workspace,
    source: &ContextSourceRef,
    order: i32,
) -> Result<ContextFragment, String> {
    let path = normalize_source_locator_path(&source.locator)?;
    let address_space = address_space_service.session_for_workspace(workspace)?;
    let read = address_space_service
        .read_text(
            &address_space,
            &ResourceRef {
                mount_id: "main".to_string(),
                path: path.clone(),
            },
            None,
            None,
        )
        .await
        .map_err(|e| format!("工作空间文件读取失败: {e}"))?;

    Ok(ContextFragment {
        slot: fragment_slot(&source.slot),
        label: fragment_label(&source.kind),
        order,
        strategy: MergeStrategy::Append,
        content: render_source_section(
            source,
            truncate_text(
                format_file_like_read_tool(&read.path, &read.content),
                source.max_chars,
            ),
        ),
    })
}

async fn resolve_workspace_snapshot_source(
    address_space_service: &RelayAddressSpaceService,
    workspace: &Workspace,
    source: &ContextSourceRef,
    order: i32,
) -> Result<ContextFragment, String> {
    let sub_path = normalize_snapshot_locator(&source.locator)?;
    let address_space = address_space_service.session_for_workspace(workspace)?;
    let listed = address_space_service
        .list(
            &address_space,
            "main",
            ListOptions {
                path: sub_path.clone().unwrap_or_else(|| ".".to_string()),
                pattern: None,
                recursive: true,
            },
            None,
            None,
        )
        .await
        .map_err(|e| format!("项目快照读取失败: {e}"))?;

    Ok(ContextFragment {
        slot: fragment_slot(&source.slot),
        label: fragment_label(&source.kind),
        order,
        strategy: MergeStrategy::Append,
        content: render_source_section(
            source,
            build_workspace_snapshot_from_entries(
                selected_workspace_binding(workspace)
                    .map(|binding| binding.root_ref.as_str())
                    .unwrap_or("."),
                sub_path.as_deref(),
                &listed.entries,
                source.max_chars,
            ),
        ),
    })
}

fn normalize_source_locator_path(locator: &str) -> Result<String, String> {
    let trimmed = locator.trim();
    if trimmed.is_empty() {
        return Err("文件来源 locator 不能为空".to_string());
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err("文件来源 locator 不能是绝对路径".to_string());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("文件来源 locator 不能包含 `..`".to_string());
    }

    Ok(trimmed.replace('\\', "/"))
}

fn normalize_snapshot_locator(locator: &str) -> Result<Option<String>, String> {
    let trimmed = locator.trim();
    if trimmed.is_empty() || trimmed == "." {
        return Ok(None);
    }
    normalize_source_locator_path(trimmed).map(Some)
}

pub fn build_workspace_snapshot_from_entries(
    workspace_root: &str,
    sub_path: Option<&str>,
    files: &[RuntimeFileEntry],
    max_chars: Option<usize>,
) -> String {
    let mut summary_entries = BTreeSet::new();
    for file in files {
        let rel = file.path.trim_matches('/');
        if rel.is_empty() {
            continue;
        }
        let parts = rel.split('/').collect::<Vec<_>>();
        if parts.len() == 1 {
            summary_entries.insert(parts[0].to_string());
            continue;
        }

        summary_entries.insert(format!("{}/", parts[0]));
        if parts.len() == 2 {
            summary_entries.insert(rel.to_string());
            continue;
        }
        summary_entries.insert(format!("{}/{}/", parts[0], parts[1]));
    }

    let entries = summary_entries
        .into_iter()
        .take(48)
        .map(|entry| format!("- {entry}"))
        .collect::<Vec<_>>()
        .join("\n");

    let tech_stack = detect_tech_stack_from_entries(files);
    let root_display = sub_path
        .map(|path| format!("{}/{}", workspace_root.trim_end_matches('/'), path))
        .unwrap_or_else(|| workspace_root.to_string())
        .replace('\\', "/");

    truncate_text(
        format!(
            "## 项目快照\n- root: {}\n- tech_stack: {}\n\n## 目录摘要\n{}",
            root_display,
            tech_stack.join(", "),
            entries
        ),
        max_chars,
    )
}

fn detect_tech_stack_from_entries(files: &[RuntimeFileEntry]) -> Vec<&'static str> {
    let paths = files
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    let mut stack = Vec::new();
    if paths.contains(&"Cargo.toml") {
        stack.push("Rust");
    }
    if paths.contains(&"package.json") {
        stack.push("Node.js");
    }
    if paths.contains(&"pnpm-lock.yaml") {
        stack.push("pnpm");
    }
    if paths
        .iter()
        .any(|path| *path == "playwright.config.ts" || *path == "playwright.config.js")
    {
        stack.push("Playwright");
    }
    if stack.is_empty() {
        stack.push("unknown");
    }
    stack
}

fn format_file_like_read_tool(path: &str, content: &str) -> String {
    let numbered = content
        .lines()
        .enumerate()
        .map(|(index, line)| format!("{:>4} | {}", index + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    if numbered.is_empty() {
        format!("file: {}\n   1 | ", path.replace('\\', "/"))
    } else {
        format!("file: {}\n{}", path.replace('\\', "/"), numbered)
    }
}

fn truncate_text(content: String, max_chars: Option<usize>) -> String {
    const DEFAULT_TRUNCATE_CHARS: usize = 12_000;
    let max = max_chars.unwrap_or(DEFAULT_TRUNCATE_CHARS);
    if content.chars().count() <= max {
        return content;
    }

    let truncated = content.chars().take(max).collect::<String>();
    format!("{truncated}\n\n> 内容已截断")
}

fn render_source_section(source: &ContextSourceRef, content: String) -> String {
    let title = display_source_label(source);
    format!("## 来源: {title}\n{content}")
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
    use super::*;

    #[test]
    fn snapshot_builder_keeps_directory_shape() {
        let snapshot = build_workspace_snapshot_from_entries(
            "/workspace/demo",
            None,
            &[
                RuntimeFileEntry {
                    path: "Cargo.toml".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                },
                RuntimeFileEntry {
                    path: "src/main.rs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                },
                RuntimeFileEntry {
                    path: "src/lib.rs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                },
                RuntimeFileEntry {
                    path: "tests/e2e/story.rs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                },
            ],
            None,
        );

        assert!(snapshot.contains("Rust"));
        assert!(snapshot.contains("- src/"));
        assert!(snapshot.contains("- src/main.rs"));
        assert!(snapshot.contains("- tests/e2e/"));
    }

    #[test]
    fn file_locator_rejects_parent_dir() {
        let err = normalize_source_locator_path("../secret.txt").expect_err("应拒绝父级目录");
        assert!(err.contains(".."));
    }
}
