use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use agentdash_domain::canvas::{Canvas, CanvasFile, CanvasRepository};

use super::mount::{PROVIDER_CANVAS_FS, list_inline_entries};
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountEditCapabilities, MountError, MountOperationContext, MountProvider, SearchMatch,
    SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::Mount;

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
        let content = canvas
            .files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.content.clone())
            .ok_or_else(|| MountError::NotFound(format!("Canvas 文件不存在: {path}")))?;
        Ok(ReadResult { path, content })
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
        let files = canvas_files_map(&canvas);
        Ok(ListResult {
            entries: list_inline_entries(
                &files,
                &path,
                options.pattern.as_deref(),
                options.recursive,
            ),
        })
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

        for file in &canvas.files {
            if !base_path.is_empty()
                && file.path != base_path
                && !file.path.starts_with(&format!("{base_path}/"))
            {
                continue;
            }

            for (index, line) in file.content.lines().enumerate() {
                let matched = if query.case_sensitive {
                    line.contains(&query.pattern)
                } else {
                    line.to_lowercase().contains(&query.pattern.to_lowercase())
                };
                if !matched {
                    continue;
                }
                matches.push(SearchMatch {
                    path: file.path.clone(),
                    line: Some((index + 1) as u32),
                    content: line.trim().to_string(),
                });
                if matches.len() >= max_results {
                    return Ok(SearchResult { matches });
                }
            }
        }

        Ok(SearchResult { matches })
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

fn canvas_files_map(canvas: &Canvas) -> BTreeMap<String, String> {
    canvas
        .files
        .iter()
        .map(|file| (file.path.clone(), file.content.clone()))
        .collect()
}

fn map_mount_err(error: String) -> MountError {
    MountError::OperationFailed(error)
}
