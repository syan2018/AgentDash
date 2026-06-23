use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use agentdash_domain::canvas::{Canvas, CanvasFile, CanvasRepository};

use super::mount::PROVIDER_CANVAS_FS;
use super::mount_inline::list_inline_entries;
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountEditCapabilities, MountError, MountOperationContext, MountProvider, SearchMatch,
    SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::canvas::{CanvasResolvedBindingFile, unresolved_canvas_binding_files};
use crate::runtime::Mount;
use crate::vfs::parse_mount_uri;

pub struct CanvasFsMountProvider {
    canvas_repo: Arc<dyn CanvasRepository>,
}

impl CanvasFsMountProvider {
    pub fn new(canvas_repo: Arc<dyn CanvasRepository>) -> Self {
        Self { canvas_repo }
    }

    async fn load_canvas(&self, mount: &Mount) -> Result<Canvas, MountError> {
        let canvas_id = parse_canvas_id(mount)?;
        self.canvas_repo
            .get_by_id(canvas_id)
            .await
            .map_err(|error| MountError::OperationFailed(error.to_string()))?
            .ok_or_else(|| MountError::NotFound(format!("Canvas 不存在: {canvas_id}")))
    }

    async fn update_canvas<F>(&self, mount: &Mount, update: F) -> Result<(), MountError>
    where
        F: FnOnce(&mut Canvas) -> Result<(), MountError>,
    {
        let mut canvas = self.load_canvas(mount).await?;
        update(&mut canvas)?;
        canvas.touch();
        self.canvas_repo
            .update(&canvas)
            .await
            .map_err(|error| MountError::OperationFailed(error.to_string()))
    }
}

#[async_trait]
impl MountProvider for CanvasFsMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_CANVAS_FS
    }

    fn edit_capabilities(&self, _mount: &Mount) -> MountEditCapabilities {
        MountEditCapabilities {
            create: true,
            delete: true,
            rename: true,
        }
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        let canvas = self.load_canvas(mount).await?;
        let file_content = canvas
            .files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.content.clone());
        let binding_file = if file_content.is_none() {
            canvas_binding_files(mount, &canvas, _ctx)
                .await
                .into_iter()
                .find(|file| file.path == path)
        } else {
            None
        };
        let content = file_content
            .or_else(|| binding_file.as_ref().map(|file| file.content.clone()))
            .ok_or_else(|| MountError::NotFound(format!("Canvas 文件不存在: {path}")))?;
        // Canvas 没有 file 级版本号，整个 canvas.updated_at 作为悲观 token：
        // 修改任意文件 ⇒ canvas touch ⇒ token 变化（保守但正确，满足 dedup invalidation）。
        let updated_at_ms = canvas.updated_at.timestamp_millis();
        let mut result = ReadResult::new(path, content)
            .with_version_token(format!("canvas:{}", updated_at_ms))
            .with_modified_at(updated_at_ms);
        if let Some(binding_file) = binding_file {
            result = result.with_attributes(binding_file_attributes(&binding_file));
        }
        Ok(result)
    }

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let normalized = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        self.update_canvas(mount, move |canvas| {
            reject_generated_binding_file_write(canvas, &normalized)?;
            if let Some(file) = canvas.files.iter_mut().find(|file| file.path == normalized) {
                file.content = content.to_string();
            } else {
                canvas
                    .files
                    .push(CanvasFile::new(normalized, content.to_string()));
            }
            Ok(())
        })
        .await
    }

    async fn delete_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let normalized = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        self.update_canvas(mount, move |canvas| {
            reject_generated_binding_file_write(canvas, &normalized)?;
            let before = canvas.files.len();
            canvas.files.retain(|file| file.path != normalized);
            if before == canvas.files.len() {
                return Err(MountError::NotFound(format!(
                    "Canvas 文件不存在: {normalized}"
                )));
            }
            Ok(())
        })
        .await
    }

    async fn rename_text(
        &self,
        mount: &Mount,
        from_path: &str,
        to_path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let from_path = normalize_mount_relative_path(from_path, false).map_err(map_mount_err)?;
        let to_path = normalize_mount_relative_path(to_path, false).map_err(map_mount_err)?;
        self.update_canvas(mount, move |canvas| {
            reject_generated_binding_file_write(canvas, &from_path)?;
            reject_generated_binding_file_write(canvas, &to_path)?;
            if canvas.files.iter().any(|file| file.path == to_path) {
                return Err(MountError::OperationFailed(format!(
                    "目标路径已存在: {to_path}"
                )));
            }
            let file = canvas
                .files
                .iter_mut()
                .find(|file| file.path == from_path)
                .ok_or_else(|| MountError::NotFound(format!("Canvas 文件不存在: {from_path}")))?;
            file.path = to_path;
            Ok(())
        })
        .await
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let path = normalize_mount_relative_path(&options.path, true).map_err(map_mount_err)?;
        let canvas = self.load_canvas(mount).await?;
        let binding_files = canvas_binding_files(mount, &canvas, _ctx).await;
        let files = canvas_files_map_from_bindings(&canvas, &binding_files);
        let binding_attrs = binding_files
            .into_iter()
            .map(|file| (file.path.clone(), binding_file_attributes(&file)))
            .collect::<BTreeMap<_, _>>();
        let mut entries =
            list_inline_entries(&files, &path, options.pattern.as_deref(), options.recursive);
        for entry in &mut entries {
            if let Some(attrs) = binding_attrs.get(&entry.path) {
                entry.is_virtual = true;
                entry.attributes = Some(attrs.clone());
            }
        }
        Ok(ListResult { entries })
    }

    async fn search_text(
        &self,
        mount: &Mount,
        query: &SearchQuery,
        _ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        let canvas = self.load_canvas(mount).await?;
        let base_path = match &query.path {
            Some(path) => normalize_mount_relative_path(path, true).map_err(map_mount_err)?,
            None => String::new(),
        };
        let max_results = query.max_results.unwrap_or(usize::MAX);
        let mut matches = Vec::new();

        for (path, content) in canvas_files_map(mount, &canvas, _ctx).await {
            if !base_path.is_empty()
                && path != base_path
                && !path.starts_with(&format!("{base_path}/"))
            {
                continue;
            }

            for (index, line) in content.lines().enumerate() {
                let matched = if query.case_sensitive {
                    line.contains(&query.pattern)
                } else {
                    line.to_lowercase().contains(&query.pattern.to_lowercase())
                };
                if !matched {
                    continue;
                }
                matches.push(SearchMatch {
                    path: path.clone(),
                    line: Some((index + 1) as u32),
                    content: line.trim().to_string(),
                });
                if matches.len() >= max_results {
                    return Ok(SearchResult {
                        matches,
                        truncated: true,
                    });
                }
            }
        }

        Ok(SearchResult {
            matches,
            truncated: false,
        })
    }

    async fn exec(
        &self,
        _mount: &Mount,
        _request: &ExecRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        Err(MountError::NotSupported(
            "canvas_fs 不支持 exec".to_string(),
        ))
    }
}

fn parse_canvas_id(mount: &Mount) -> Result<Uuid, MountError> {
    let raw = mount
        .metadata
        .get("canvas_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| MountError::OperationFailed("mount metadata 缺少 canvas_id".to_string()))?;
    Uuid::parse_str(raw)
        .map_err(|error| MountError::OperationFailed(format!("canvas_id 无效: {error}")))
}

async fn canvas_files_map(
    mount: &Mount,
    canvas: &Canvas,
    ctx: &MountOperationContext,
) -> BTreeMap<String, String> {
    let binding_files = canvas_binding_files(mount, canvas, ctx).await;
    canvas_files_map_from_bindings(canvas, &binding_files)
}

fn canvas_files_map_from_bindings(
    canvas: &Canvas,
    binding_files: &[CanvasResolvedBindingFile],
) -> BTreeMap<String, String> {
    let mut files = canvas
        .files
        .iter()
        .map(|file| (file.path.clone(), file.content.clone()))
        .collect::<BTreeMap<_, _>>();
    for binding_file in binding_files {
        files
            .entry(binding_file.path.clone())
            .or_insert_with(|| binding_file.content.clone());
    }
    files
}

async fn canvas_binding_files(
    mount: &Mount,
    canvas: &Canvas,
    ctx: &MountOperationContext,
) -> Vec<CanvasResolvedBindingFile> {
    let mut files = unresolved_canvas_binding_files(canvas)
        .into_iter()
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();

    let (Some(vfs), Some(resolver)) = (&ctx.runtime_vfs, &ctx.runtime_text_resolver) else {
        return files.into_values().collect();
    };

    for file in files.values_mut() {
        let Ok(source_ref) = parse_mount_uri(&file.source_uri, vfs.as_ref()) else {
            continue;
        };
        if source_ref.mount_id == mount.id && source_ref.path == file.path {
            continue;
        }
        if let Ok(result) = resolver
            .read_runtime_text(vfs.as_ref(), &file.source_uri, ctx.identity.as_ref())
            .await
        {
            file.content = result.content;
            file.resolved = true;
        }
    }
    files.into_values().collect()
}

fn reject_generated_binding_file_write(canvas: &Canvas, path: &str) -> Result<(), MountError> {
    if canvas
        .bindings
        .iter()
        .any(|binding| binding.data_path() == path)
    {
        return Err(MountError::OperationFailed(format!(
            "Canvas binding 文件由 source_uri 生成，不能直接修改: {path}"
        )));
    }
    Ok(())
}

fn binding_file_attributes(
    file: &CanvasResolvedBindingFile,
) -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = serde_json::Map::new();
    attrs.insert(
        "canvas_generated".to_string(),
        serde_json::Value::String("binding".to_string()),
    );
    attrs.insert(
        "alias".to_string(),
        serde_json::Value::String(file.alias.clone()),
    );
    attrs.insert(
        "source_uri".to_string(),
        serde_json::Value::String(file.source_uri.clone()),
    );
    attrs.insert(
        "content_type".to_string(),
        serde_json::Value::String(file.content_type.clone()),
    );
    attrs.insert(
        "resolved".to_string(),
        serde_json::Value::Bool(file.resolved),
    );
    attrs
}

fn map_mount_err(error: String) -> MountError {
    MountError::OperationFailed(error)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::canvas::{CanvasDataBinding, CanvasRepository};
    use agentdash_spi::platform::mount::MountRuntimeTextResolver;

    use super::*;
    use crate::vfs::build_canvas_mount;

    #[derive(Default)]
    struct FakeCanvasRepo {
        canvases: Mutex<BTreeMap<Uuid, Canvas>>,
    }

    struct StaticRuntimeTextResolver {
        content: Mutex<String>,
    }

    #[async_trait]
    impl MountRuntimeTextResolver for StaticRuntimeTextResolver {
        async fn read_runtime_text(
            &self,
            _vfs: &agentdash_spi::Vfs,
            uri: &str,
            _identity: Option<&agentdash_spi::platform::auth::AuthIdentity>,
        ) -> Result<ReadResult, MountError> {
            if uri != "main://data/stats.csv" {
                return Err(MountError::NotFound(uri.to_string()));
            }
            Ok(ReadResult::new(
                "data/stats.csv",
                self.content.lock().expect("lock").clone(),
            ))
        }
    }

    #[async_trait]
    impl CanvasRepository for FakeCanvasRepo {
        async fn create(&self, canvas: &Canvas) -> Result<(), DomainError> {
            self.canvases
                .lock()
                .expect("lock")
                .insert(canvas.id, canvas.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<Canvas>, DomainError> {
            Ok(self.canvases.lock().expect("lock").get(&id).cloned())
        }

        async fn get_by_mount_id(
            &self,
            project_id: Uuid,
            mount_id: &str,
        ) -> Result<Option<Canvas>, DomainError> {
            Ok(self
                .canvases
                .lock()
                .expect("lock")
                .values()
                .find(|canvas| canvas.project_id == project_id && canvas.mount_id == mount_id)
                .cloned())
        }

        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Canvas>, DomainError> {
            Ok(self
                .canvases
                .lock()
                .expect("lock")
                .values()
                .filter(|canvas| canvas.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, canvas: &Canvas) -> Result<(), DomainError> {
            self.canvases
                .lock()
                .expect("lock")
                .insert(canvas.id, canvas.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.canvases.lock().expect("lock").remove(&id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn canvas_mount_exposes_resolved_binding_files_as_read_only_generated_files() {
        let project_id = Uuid::new_v4();
        let mut canvas = Canvas::new(
            project_id,
            "cvs-dashboard".to_string(),
            "Dashboard".to_string(),
            String::new(),
        );
        canvas.bindings = vec![CanvasDataBinding::new(
            "stats".to_string(),
            "main://data/stats.csv".to_string(),
        )];

        let vfs = agentdash_spi::Vfs {
            mounts: vec![
                build_canvas_mount(&canvas),
                Mount {
                    id: "main".to_string(),
                    provider: "inline_fs".to_string(),
                    backend_id: String::new(),
                    root_ref: "context://inline/main".to_string(),
                    capabilities: vec![agentdash_spi::MountCapability::Read],
                    default_write: false,
                    display_name: "Main".to_string(),
                    metadata: serde_json::json!({}),
                },
            ],
            default_mount_id: Some(canvas.mount_id.clone()),
            ..Default::default()
        };
        let mount = vfs.mounts.first().expect("mount").clone();
        let resolver = Arc::new(StaticRuntimeTextResolver {
            content: Mutex::new("name,value\nA,1".to_string()),
        });

        let repo = Arc::new(FakeCanvasRepo::default());
        repo.create(&canvas).await.expect("create canvas");
        let provider = CanvasFsMountProvider::new(repo);
        let ctx = MountOperationContext {
            runtime_vfs: Some(Arc::new(vfs.clone())),
            runtime_text_resolver: Some(resolver.clone()),
            ..MountOperationContext::default()
        };

        let read = provider
            .read_text(&mount, "bindings/stats.csv", &ctx)
            .await
            .expect("read binding file");
        assert_eq!(read.content, "name,value\nA,1");
        assert_eq!(
            read.attributes
                .as_ref()
                .and_then(|attrs| attrs.get("resolved"))
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );

        *resolver.content.lock().expect("lock") = "name,value\nB,2".to_string();
        let reread = provider
            .read_text(&mount, "bindings/stats.csv", &ctx)
            .await
            .expect("reread binding file");
        assert_eq!(reread.content, "name,value\nB,2");

        let listed = provider
            .list(
                &mount,
                &ListOptions {
                    path: "bindings".to_string(),
                    pattern: None,
                    recursive: true,
                },
                &ctx,
            )
            .await
            .expect("list bindings");
        assert!(
            listed
                .entries
                .iter()
                .any(|entry| entry.path == "bindings/stats.csv")
        );

        let write_error = provider
            .write_text(&mount, "bindings/stats.csv", "changed", &ctx)
            .await
            .expect_err("generated binding files are read-only");
        assert!(matches!(write_error, MountError::OperationFailed(_)));
    }
}
