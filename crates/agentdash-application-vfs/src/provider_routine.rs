//! `routine_vfs` mount：投影 Routine 当前触发事实与跨轮次 memory。

use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_domain::inline_file::{InlineFile, InlineFileContent, InlineFileOwnerKind};
use agentdash_domain::routine::{RoutineExecution, RoutineExecutionRepository};
use async_trait::async_trait;
use uuid::Uuid;

use super::mount::PROVIDER_ROUTINE_VFS;
use super::mount_inline::list_inline_entries;
use super::path::normalize_mount_relative_path;
use super::provider::{
    MountEditCapabilities, MountError, MountOperationContext, MountProvider, SearchMatch,
    SearchQuery, SearchResult,
};
use super::types::{ExecRequest, ExecResult, ListOptions, ListResult, ReadResult};
use crate::runtime::{Mount, RuntimeFileEntry};

const MEMORY_CONTAINER_ID: &str = "memory";
const MEMORY_FILES: &[&str] = &[
    "brief.md",
    "facts.md",
    "decisions.md",
    "open-items.md",
    "changelog.md",
];
const ENTITY_FILES: &[&str] = &["brief.md", "facts.md", "open-items.md", "last-run.md"];
const CURRENT_FILES: &[&str] = &["trigger.json", "execution.json", "resolved-prompt.md"];

pub struct RoutineMountProvider {
    routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
    inline_file_repo: Arc<dyn agentdash_domain::inline_file::InlineFileRepository>,
}

impl RoutineMountProvider {
    pub fn new(
        routine_execution_repo: Arc<dyn RoutineExecutionRepository>,
        inline_file_repo: Arc<dyn agentdash_domain::inline_file::InlineFileRepository>,
    ) -> Self {
        Self {
            routine_execution_repo,
            inline_file_repo,
        }
    }
}

#[async_trait]
impl MountProvider for RoutineMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_ROUTINE_VFS
    }

    fn supported_capabilities(&self) -> Vec<&str> {
        vec!["read", "write", "list", "search"]
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        match classify_path(mount, &path)? {
            RoutinePath::Current(CurrentFile::Trigger) => {
                let execution = self.load_execution(mount).await?;
                let content = serde_json::to_string_pretty(&serde_json::json!({
                    "routine_id": execution.routine_id,
                    "execution_id": execution.id,
                    "trigger_source": execution.trigger_source,
                    "trigger_payload": execution.trigger_payload,
                    "entity_key": execution.entity_key,
                    "started_at": execution.started_at,
                }))
                .map_err(|error| MountError::OperationFailed(error.to_string()))?;
                Ok(
                    ReadResult::new(path, content)
                        .with_attributes(virtual_attrs("routine_current")),
                )
            }
            RoutinePath::Current(CurrentFile::Execution) => {
                let execution = self.load_execution(mount).await?;
                let content = serde_json::to_string_pretty(&execution)
                    .map_err(|error| MountError::OperationFailed(error.to_string()))?;
                Ok(
                    ReadResult::new(path, content)
                        .with_attributes(virtual_attrs("routine_current")),
                )
            }
            RoutinePath::Current(CurrentFile::ResolvedPrompt) => {
                let execution = self.load_execution(mount).await?;
                Ok(
                    ReadResult::new(path, execution.resolved_prompt.unwrap_or_default())
                        .with_attributes(virtual_attrs("routine_current")),
                )
            }
            RoutinePath::Memory { file } => {
                self.read_memory_file(mount, &path, MEMORY_CONTAINER_ID, &file)
                    .await
            }
            RoutinePath::Entity { entity_key, file } => {
                let container_id = entity_container_id(&entity_key);
                self.read_memory_file(mount, &path, &container_id, &file)
                    .await
            }
        }
    }

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        let (container_id, file_path) = match classify_path(mount, &path)? {
            RoutinePath::Current(_) => {
                return Err(MountError::NotSupported(format!(
                    "Routine current projection 是只读路径: {path}"
                )));
            }
            RoutinePath::Memory { file } => (MEMORY_CONTAINER_ID.to_string(), file),
            RoutinePath::Entity { entity_key, file } => (entity_container_id(&entity_key), file),
        };
        let routine_id = parse_routine_id(mount)?;
        let file = InlineFile::new_text(
            InlineFileOwnerKind::Routine,
            routine_id,
            container_id,
            file_path,
            content,
        );
        self.inline_file_repo
            .upsert_file(&file)
            .await
            .map_err(map_domain_err)
    }

    fn edit_capabilities(&self, _mount: &Mount) -> MountEditCapabilities {
        MountEditCapabilities {
            create: true,
            delete: false,
            rename: false,
        }
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let path = normalize_mount_relative_path(&options.path, true).map_err(map_mount_err)?;
        let files = self.projected_file_map(mount).await?;
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
        ctx: &MountOperationContext,
    ) -> Result<SearchResult, MountError> {
        let base_path = match &query.path {
            Some(path) => normalize_mount_relative_path(path, true).map_err(map_mount_err)?,
            None => String::new(),
        };
        let entries = self
            .list(
                mount,
                &ListOptions {
                    path: base_path,
                    pattern: None,
                    recursive: true,
                },
                ctx,
            )
            .await?;
        let max_results = query.max_results.unwrap_or(usize::MAX);
        let needle = if query.case_sensitive {
            query.pattern.clone()
        } else {
            query.pattern.to_lowercase()
        };
        let mut matches = Vec::new();
        for entry in entries.entries.into_iter().filter(|entry| !entry.is_dir) {
            let read = self.read_text(mount, &entry.path, ctx).await?;
            for (index, line) in read.content.lines().enumerate() {
                let haystack = if query.case_sensitive {
                    line.to_string()
                } else {
                    line.to_lowercase()
                };
                if !haystack.contains(&needle) {
                    continue;
                }
                matches.push(SearchMatch {
                    path: entry.path.clone(),
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

    async fn stat(
        &self,
        mount: &Mount,
        path: &str,
        ctx: &MountOperationContext,
    ) -> Result<RuntimeFileEntry, MountError> {
        let path = normalize_mount_relative_path(path, false).map_err(map_mount_err)?;
        let listing = self
            .list(
                mount,
                &ListOptions {
                    path: String::new(),
                    pattern: None,
                    recursive: true,
                },
                ctx,
            )
            .await?;
        listing
            .entries
            .into_iter()
            .find(|entry| entry.path == path)
            .ok_or_else(|| MountError::NotFound(format!("Routine memory 路径不存在: {path}")))
    }

    async fn exec(
        &self,
        _mount: &Mount,
        _request: &ExecRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ExecResult, MountError> {
        Err(MountError::NotSupported(
            "routine_vfs 不支持 exec".to_string(),
        ))
    }
}

impl RoutineMountProvider {
    async fn load_execution(&self, mount: &Mount) -> Result<RoutineExecution, MountError> {
        let execution_id = parse_execution_id(mount)?;
        self.routine_execution_repo
            .get_by_id(execution_id)
            .await
            .map_err(map_domain_err)?
            .ok_or_else(|| MountError::NotFound(format!("RoutineExecution 不存在: {execution_id}")))
    }

    async fn read_memory_file(
        &self,
        mount: &Mount,
        display_path: &str,
        container_id: &str,
        file_path: &str,
    ) -> Result<ReadResult, MountError> {
        let routine_id = parse_routine_id(mount)?;
        let file = self
            .inline_file_repo
            .get_file(
                InlineFileOwnerKind::Routine,
                routine_id,
                container_id,
                file_path,
            )
            .await
            .map_err(map_domain_err)?;
        let Some(file) = file else {
            return Ok(ReadResult::new(display_path, "").with_attributes(memory_attrs(true)));
        };
        let updated_at_ms = file.updated_at.timestamp_millis();
        let size_bytes = file.size_bytes;
        let content = match file.content {
            InlineFileContent::Text { content } => content,
            InlineFileContent::Binary { .. } => {
                return Err(MountError::NotSupported(format!(
                    "Routine memory 不支持二进制文本读取: {display_path}"
                )));
            }
        };
        Ok(ReadResult::new(display_path, content)
            .with_version_token(format!("ts:{updated_at_ms}:{size_bytes}"))
            .with_modified_at(updated_at_ms)
            .with_attributes(memory_attrs(false)))
    }

    async fn projected_file_map(
        &self,
        mount: &Mount,
    ) -> Result<BTreeMap<String, String>, MountError> {
        let routine_id = parse_routine_id(mount)?;
        let mut files = BTreeMap::new();
        for file in CURRENT_FILES {
            files.insert(format!("current/{file}"), String::new());
        }
        for file in MEMORY_FILES {
            files.insert(format!("memory/{file}"), String::new());
        }
        let memory_files = self
            .inline_file_repo
            .list_files(
                InlineFileOwnerKind::Routine,
                routine_id,
                MEMORY_CONTAINER_ID,
            )
            .await
            .map_err(map_domain_err)?;
        for file in memory_files {
            files.insert(format!("memory/{}", file.path), String::new());
        }

        if let Some(entity_key) = current_entity_key(mount) {
            let encoded = encode_path_segment(&entity_key);
            for file in ENTITY_FILES {
                files.insert(format!("entities/{encoded}/{file}"), String::new());
            }
            let container_id = entity_container_id(&entity_key);
            let entity_files = self
                .inline_file_repo
                .list_files(InlineFileOwnerKind::Routine, routine_id, &container_id)
                .await
                .map_err(map_domain_err)?;
            for file in entity_files {
                files.insert(format!("entities/{encoded}/{}", file.path), String::new());
            }
        }

        Ok(files)
    }
}

enum CurrentFile {
    Trigger,
    Execution,
    ResolvedPrompt,
}

enum RoutinePath {
    Current(CurrentFile),
    Memory { file: String },
    Entity { entity_key: String, file: String },
}

fn classify_path(mount: &Mount, path: &str) -> Result<RoutinePath, MountError> {
    let segments = if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect::<Vec<_>>()
    };
    match segments.as_slice() {
        ["current", "trigger.json"] => Ok(RoutinePath::Current(CurrentFile::Trigger)),
        ["current", "execution.json"] => Ok(RoutinePath::Current(CurrentFile::Execution)),
        ["current", "resolved-prompt.md"] => Ok(RoutinePath::Current(CurrentFile::ResolvedPrompt)),
        ["memory", file] if MEMORY_FILES.contains(file) => Ok(RoutinePath::Memory {
            file: (*file).to_string(),
        }),
        ["entities", encoded_key, file] if ENTITY_FILES.contains(file) => {
            let entity_key = decode_path_segment(encoded_key)?;
            let current = current_entity_key(mount).ok_or_else(|| {
                MountError::NotSupported("当前 Routine execution 没有 entity_key".to_string())
            })?;
            if entity_key != current {
                return Err(MountError::NotSupported(format!(
                    "只能写入当前 entity memory: {}",
                    encode_path_segment(&current)
                )));
            }
            Ok(RoutinePath::Entity {
                entity_key,
                file: (*file).to_string(),
            })
        }
        _ => Err(MountError::NotFound(format!(
            "Routine memory 路径不存在: {path}"
        ))),
    }
}

fn parse_routine_id(mount: &Mount) -> Result<Uuid, MountError> {
    parse_uuid_metadata(mount, "routine_id")
}

fn parse_execution_id(mount: &Mount) -> Result<Uuid, MountError> {
    parse_uuid_metadata(mount, "execution_id")
}

fn parse_uuid_metadata(mount: &Mount, key: &str) -> Result<Uuid, MountError> {
    let raw = mount
        .metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| MountError::OperationFailed(format!("routine mount metadata 缺少 {key}")))?;
    Uuid::parse_str(raw)
        .map_err(|error| MountError::OperationFailed(format!("routine mount {key} 无效: {error}")))
}

fn current_entity_key(mount: &Mount) -> Option<String> {
    mount
        .metadata
        .get("entity_key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn entity_container_id(entity_key: &str) -> String {
    format!("entity:{entity_key}")
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        let ch = *byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            encoded.push(ch);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn decode_path_segment(value: &str) -> Result<String, MountError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(MountError::OperationFailed(format!(
                    "entity key path segment 非法: {value}"
                )));
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|error| MountError::OperationFailed(error.to_string()))?;
            let byte = u8::from_str_radix(hex, 16)
                .map_err(|error| MountError::OperationFailed(error.to_string()))?;
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|error| MountError::OperationFailed(error.to_string()))
}

fn virtual_attrs(kind: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = serde_json::Map::new();
    attrs.insert(
        "content_kind".to_string(),
        serde_json::Value::String("text".to_string()),
    );
    attrs.insert(
        "routine_projection".to_string(),
        serde_json::Value::String(kind.to_string()),
    );
    attrs
}

fn memory_attrs(is_virtual: bool) -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = virtual_attrs("routine_memory");
    attrs.insert(
        "virtual_default".to_string(),
        serde_json::Value::Bool(is_virtual),
    );
    attrs
}

fn map_mount_err(error: String) -> MountError {
    MountError::OperationFailed(error)
}

fn map_domain_err(error: agentdash_domain::common::error::DomainError) -> MountError {
    MountError::OperationFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::inline_file::InlineFileRepository;
    use agentdash_domain::routine::{RoutineDispatchRefs, RoutineExecutionStatus};
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryInlineRepo {
        files: Mutex<Vec<InlineFile>>,
    }

    #[async_trait]
    impl InlineFileRepository for MemoryInlineRepo {
        async fn get_file(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
            path: &str,
        ) -> Result<Option<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .expect("lock")
                .iter()
                .find(|file| {
                    file.owner_kind == owner_kind
                        && file.owner_id == owner_id
                        && file.container_id == container_id
                        && file.path == path
                })
                .cloned())
        }

        async fn list_files(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<Vec<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .expect("lock")
                .iter()
                .filter(|file| {
                    file.owner_kind == owner_kind
                        && file.owner_id == owner_id
                        && file.container_id == container_id
                })
                .cloned()
                .collect())
        }

        async fn list_files_by_owner(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
        ) -> Result<Vec<InlineFile>, DomainError> {
            Ok(self
                .files
                .lock()
                .expect("lock")
                .iter()
                .filter(|file| file.owner_kind == owner_kind && file.owner_id == owner_id)
                .cloned()
                .collect())
        }

        async fn upsert_file(&self, file: &InlineFile) -> Result<(), DomainError> {
            let mut files = self.files.lock().expect("lock");
            if let Some(existing) = files.iter_mut().find(|existing| {
                existing.owner_kind == file.owner_kind
                    && existing.owner_id == file.owner_id
                    && existing.container_id == file.container_id
                    && existing.path == file.path
            }) {
                *existing = file.clone();
            } else {
                files.push(file.clone());
            }
            Ok(())
        }

        async fn upsert_files(&self, files: &[InlineFile]) -> Result<(), DomainError> {
            for file in files {
                self.upsert_file(file).await?;
            }
            Ok(())
        }

        async fn delete_file(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
            path: &str,
        ) -> Result<(), DomainError> {
            self.files.lock().expect("lock").retain(|file| {
                file.owner_kind != owner_kind
                    || file.owner_id != owner_id
                    || file.container_id != container_id
                    || file.path != path
            });
            Ok(())
        }

        async fn delete_by_container(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<(), DomainError> {
            self.files.lock().expect("lock").retain(|file| {
                file.owner_kind != owner_kind
                    || file.owner_id != owner_id
                    || file.container_id != container_id
            });
            Ok(())
        }

        async fn delete_by_owner(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
        ) -> Result<(), DomainError> {
            self.files
                .lock()
                .expect("lock")
                .retain(|file| file.owner_kind != owner_kind || file.owner_id != owner_id);
            Ok(())
        }

        async fn count_files(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: Uuid,
            container_id: &str,
        ) -> Result<i64, DomainError> {
            Ok(self
                .list_files(owner_kind, owner_id, container_id)
                .await?
                .len() as i64)
        }
    }

    struct MemoryExecutionRepo {
        execution: RoutineExecution,
    }

    #[async_trait]
    impl RoutineExecutionRepository for MemoryExecutionRepo {
        async fn create(&self, _execution: &RoutineExecution) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<RoutineExecution>, DomainError> {
            Ok((self.execution.id == id).then(|| self.execution.clone()))
        }

        async fn update(&self, _execution: &RoutineExecution) -> Result<(), DomainError> {
            Ok(())
        }

        async fn list_by_routine(
            &self,
            _routine_id: Uuid,
            _limit: u32,
            _offset: u32,
        ) -> Result<Vec<RoutineExecution>, DomainError> {
            Ok(vec![self.execution.clone()])
        }

        async fn find_latest_by_entity_key(
            &self,
            _routine_id: Uuid,
            _entity_key: &str,
        ) -> Result<Option<RoutineExecution>, DomainError> {
            Ok(Some(self.execution.clone()))
        }
    }

    fn fixture() -> (RoutineMountProvider, Mount, MountOperationContext) {
        let routine_id = Uuid::new_v4();
        let execution_id = Uuid::new_v4();
        let execution = RoutineExecution {
            id: execution_id,
            routine_id,
            trigger_source: "webhook".to_string(),
            trigger_payload: Some(json!({ "pull_request": { "number": 42 } })),
            resolved_prompt: Some("review PR 42".to_string()),
            dispatch_refs: Some(RoutineDispatchRefs::new(
                agentdash_domain::workflow::AgentRuntimeRefs::new(
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    Some(agentdash_domain::workflow::OrchestrationBindingRefs::new(
                        Uuid::new_v4(),
                        "routine.main",
                        1,
                    )),
                ),
            )),
            status: RoutineExecutionStatus::Dispatched,
            started_at: Utc::now(),
            completed_at: None,
            error: None,
            entity_key: Some("PR/42".to_string()),
        };

        let provider = RoutineMountProvider::new(
            Arc::new(MemoryExecutionRepo { execution }),
            Arc::new(MemoryInlineRepo::default()),
        );
        let mount = crate::build_routine_mount(routine_id, execution_id, "webhook", Some("PR/42"));
        (provider, mount, MountOperationContext::default())
    }

    #[tokio::test]
    async fn current_projection_is_readable_and_read_only() {
        let (provider, mount, ctx) = fixture();

        let trigger = provider
            .read_text(&mount, "current/trigger.json", &ctx)
            .await
            .expect("trigger");
        assert!(trigger.content.contains("pull_request"));

        let err = provider
            .write_text(&mount, "current/trigger.json", "{}", &ctx)
            .await
            .expect_err("current should be readonly");
        assert!(matches!(err, MountError::NotSupported(_)));
    }

    #[tokio::test]
    async fn writes_routine_memory_and_current_entity_memory_only() {
        let (provider, mount, ctx) = fixture();
        provider
            .write_text(&mount, "memory/brief.md", "Routine purpose", &ctx)
            .await
            .expect("write memory");
        let brief = provider
            .read_text(&mount, "memory/brief.md", &ctx)
            .await
            .expect("read memory");
        assert_eq!(brief.content, "Routine purpose");

        provider
            .write_text(&mount, "entities/PR%2F42/last-run.md", "Done", &ctx)
            .await
            .expect("write entity");
        let last_run = provider
            .read_text(&mount, "entities/PR%2F42/last-run.md", &ctx)
            .await
            .expect("read entity");
        assert_eq!(last_run.content, "Done");

        let err = provider
            .write_text(&mount, "entities/PR%2F43/last-run.md", "Nope", &ctx)
            .await
            .expect_err("other entity should be rejected");
        assert!(matches!(err, MountError::NotSupported(_)));
    }

    #[tokio::test]
    async fn list_projects_current_memory_and_encoded_entity_paths() {
        let (provider, mount, ctx) = fixture();
        let result = provider
            .list(
                &mount,
                &ListOptions {
                    path: String::new(),
                    pattern: None,
                    recursive: true,
                },
                &ctx,
            )
            .await
            .expect("list");
        let paths = result
            .entries
            .into_iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        assert!(paths.contains(&"current/trigger.json".to_string()));
        assert!(paths.contains(&"memory/brief.md".to_string()));
        assert!(paths.contains(&"entities/PR%2F42/last-run.md".to_string()));
    }
}
