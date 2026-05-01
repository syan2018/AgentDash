use std::collections::BTreeSet;
use std::path::{Component, Path};

use agentdash_domain::context_source::{ContextSourceKind, ContextSourceRef};
use agentdash_domain::workspace::Workspace;
use agentdash_spi::{ContextFragment, MergeStrategy, ResolveSourcesOutput};

use crate::runtime::RuntimeFileEntry;
use crate::vfs::{ListOptions, RelayVfsService, ResourceRef, selected_workspace_binding};
use crate::workspace::BackendAvailability;

use super::builder::Contribution;
use super::rendering::declared_sources::{
    display_source_label, fragment_label, fragment_slot, render_source_section, truncate_text,
};

/// 把已解析完成的 workspace 静态来源片段薄包装为 `Contribution`。
///
/// 对应老路径下由 `StaticFragmentsContributor` 承载的"预解析声明式来源"——
/// `resolve_workspace_declared_sources` 已经是纯异步函数，产出 `Vec<ContextFragment>`，
/// 上层调用方可以在 await 之后直接调用本函数得到 Contribution 喂给 builder。
pub fn contribute_workspace_static_sources(fragments: Vec<ContextFragment>) -> Contribution {
    Contribution::fragments_only(fragments)
}

/// 解析 Story/Task 上声明式来源（File / ProjectSnapshot）为具体上下文片段
///
/// 需要 Workspace 在线才能读取远端文件；如果 Backend 不在线或 Workspace 缺失，
/// 非 required 来源会生成 warning 而非报错。
pub async fn resolve_workspace_declared_sources(
    availability: &dyn BackendAvailability,
    vfs_service: &RelayVfsService,
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
                resolve_workspace_file_source(vfs_service, workspace, source, order).await
            }
            ContextSourceKind::ProjectSnapshot => {
                resolve_workspace_snapshot_source(vfs_service, workspace, source, order).await
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
    vfs_service: &RelayVfsService,
    workspace: &Workspace,
    source: &ContextSourceRef,
    order: i32,
) -> Result<ContextFragment, String> {
    let path = normalize_source_locator_path(&source.locator)?;
    let vfs = vfs_service.session_for_workspace(workspace)?;
    let read = vfs_service
        .read_text(
            &vfs,
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
        slot: fragment_slot(&source.slot).to_string(),
        label: fragment_label(&source.kind).to_string(),
        order,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:workspace_source:file".to_string(),
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
    vfs_service: &RelayVfsService,
    workspace: &Workspace,
    source: &ContextSourceRef,
    order: i32,
) -> Result<ContextFragment, String> {
    let sub_path = normalize_snapshot_locator(&source.locator)?;
    let vfs = vfs_service.session_for_workspace(workspace)?;
    let listed = vfs_service
        .list(
            &vfs,
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
        slot: fragment_slot(&source.slot).to_string(),
        label: fragment_label(&source.kind).to_string(),
        order,
        strategy: MergeStrategy::Append,
        scope: ContextFragment::default_scope(),
        source: "legacy:workspace_source:snapshot".to_string(),
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
    mount_root_ref: &str,
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
        .map(|path| format!("{}/{}", mount_root_ref.trim_end_matches('/'), path))
        .unwrap_or_else(|| mount_root_ref.to_string())
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
                    is_virtual: false,
                    attributes: None,
                },
                RuntimeFileEntry {
                    path: "src/main.rs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                    is_virtual: false,
                    attributes: None,
                },
                RuntimeFileEntry {
                    path: "src/lib.rs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                    is_virtual: false,
                    attributes: None,
                },
                RuntimeFileEntry {
                    path: "tests/e2e/story.rs".to_string(),
                    size: None,
                    modified_at: None,
                    is_dir: false,
                    is_virtual: false,
                    attributes: None,
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
