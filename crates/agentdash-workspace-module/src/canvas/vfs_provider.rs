use std::collections::BTreeMap;
use std::sync::Arc;

use agentdash_application_vfs::{
    ApplyPatchError, ApplyPatchRequest, ApplyPatchResult, ApplyPatchTarget, ExecRequest,
    ExecResult, ListOptions, ListResult, MountEditCapabilities, MountError, MountOperationContext,
    MountProvider, PROVIDER_CANVAS_FS, ReadResult, SearchMatch, SearchQuery, SearchResult,
    apply_patch_to_target, list_inline_entries, normalize_mount_relative_path,
};
use agentdash_domain::common::{Mount, MountCapability};
use agentdash_domain::interaction::{
    DefinitionRevisionCommit, InteractionDefinition, InteractionDefinitionRepository,
    InteractionDefinitionRevision, InteractionDefinitionStatus, SourceBundleChangeset, SourceFile,
    SourceFileChange,
};
use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

/// VFS adapter over the current immutable Canvas SourceBundle revision.
///
/// A mount keeps a stable definition identity. Every mutation reloads current, creates one new
/// immutable revision and commits it with repository CAS.
pub struct CanvasSourceBundleMountProvider {
    definitions: Arc<dyn InteractionDefinitionRepository>,
}

impl CanvasSourceBundleMountProvider {
    pub fn new(definitions: Arc<dyn InteractionDefinitionRepository>) -> Self {
        Self { definitions }
    }

    async fn load_current(
        &self,
        mount: &Mount,
    ) -> Result<(InteractionDefinition, InteractionDefinitionRevision), MountError> {
        let definition_id = parse_definition_id(mount)?;
        let definition = self
            .definitions
            .get(definition_id)
            .await
            .map_err(operation_failed)?
            .ok_or_else(|| {
                MountError::NotFound(format!("Canvas definition 不存在: {definition_id}"))
            })?;
        if definition.status != InteractionDefinitionStatus::Active {
            return Err(MountError::NotSupported(
                "archived Canvas definition 不可通过 authoring mount 使用".to_owned(),
            ));
        }
        let revision = self
            .definitions
            .get_revision(definition.current_revision_id)
            .await
            .map_err(operation_failed)?
            .ok_or_else(|| {
                MountError::NotFound(format!(
                    "Canvas current revision 不存在: {}",
                    definition.current_revision_id
                ))
            })?;
        if revision.definition_id != definition.id
            || revision.project_id != definition.project_id
            || revision.authoring_mount_id != mount.id
        {
            return Err(MountError::OperationFailed(
                "Canvas mount 与 current definition revision identity 不一致".to_owned(),
            ));
        }
        Ok((definition, revision))
    }

    async fn commit_changeset(
        &self,
        mount: &Mount,
        changeset: SourceBundleChangeset,
    ) -> Result<(), MountError> {
        ensure_writable(mount)?;
        let (definition, current) = self.load_current(mount).await?;
        self.commit_loaded_changeset(definition, current, changeset)
            .await
    }

    async fn commit_loaded_changeset(
        &self,
        definition: InteractionDefinition,
        current: InteractionDefinitionRevision,
        changeset: SourceBundleChangeset,
    ) -> Result<(), MountError> {
        let source_bundle = current
            .source_bundle
            .apply_changeset(changeset)
            .map_err(operation_failed)?;
        if source_bundle.digest == current.source_bundle.digest {
            return Ok(());
        }
        let mut next = current.clone();
        next.revision_id = Uuid::new_v4();
        next.revision_number = next.revision_number.checked_add(1).ok_or_else(|| {
            MountError::OperationFailed("Canvas definition revision number 已达上限".to_owned())
        })?;
        next.source_bundle = source_bundle;
        next.created_at = chrono::Utc::now();
        next.validate().map_err(operation_failed)?;
        self.definitions
            .commit_revision(
                definition.id,
                DefinitionRevisionCommit {
                    expected_current_revision_id: current.revision_id,
                    revision: next,
                },
            )
            .await
            .map_err(operation_failed)?;
        Ok(())
    }
}

#[derive(Default)]
struct SourceBundlePatchState {
    files: BTreeMap<String, String>,
    renames: Vec<(String, String)>,
}

struct SourceBundlePatchTarget {
    state: Mutex<SourceBundlePatchState>,
}

impl SourceBundlePatchTarget {
    fn new(revision: &InteractionDefinitionRevision) -> Self {
        Self {
            state: Mutex::new(SourceBundlePatchState {
                files: revision
                    .source_bundle
                    .files
                    .iter()
                    .map(|file| (file.path.clone(), file.content.clone()))
                    .collect(),
                renames: Vec::new(),
            }),
        }
    }

    fn into_state(self) -> SourceBundlePatchState {
        self.state.into_inner()
    }
}

#[async_trait]
impl ApplyPatchTarget for SourceBundlePatchTarget {
    fn edit_capabilities(&self) -> MountEditCapabilities {
        MountEditCapabilities {
            create: true,
            delete: true,
            rename: true,
        }
    }

    async fn read_text(&self, path: &str) -> Result<String, ApplyPatchError> {
        self.state
            .lock()
            .await
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| ApplyPatchError::Apply(format!("Canvas source 文件不存在: {path}")))
    }

    async fn write_text(&self, path: &str, content: &str) -> Result<(), ApplyPatchError> {
        self.state
            .lock()
            .await
            .files
            .insert(path.to_owned(), content.to_owned());
        Ok(())
    }

    async fn create_text(&self, path: &str, content: &str) -> Result<(), ApplyPatchError> {
        let mut state = self.state.lock().await;
        if state.files.contains_key(path) {
            return Err(ApplyPatchError::Apply(format!(
                "Canvas source 目标文件已存在: {path}"
            )));
        }
        state.files.insert(path.to_owned(), content.to_owned());
        Ok(())
    }

    async fn delete_text(&self, path: &str) -> Result<(), ApplyPatchError> {
        if self.state.lock().await.files.remove(path).is_none() {
            return Err(ApplyPatchError::Apply(format!(
                "Canvas source 文件不存在: {path}"
            )));
        }
        Ok(())
    }

    async fn rename_text(&self, from_path: &str, to_path: &str) -> Result<(), ApplyPatchError> {
        let mut state = self.state.lock().await;
        if state.files.contains_key(to_path) {
            return Err(ApplyPatchError::Apply(format!(
                "Canvas source 目标文件已存在: {to_path}"
            )));
        }
        let content = state.files.remove(from_path).ok_or_else(|| {
            ApplyPatchError::Apply(format!("Canvas source 文件不存在: {from_path}"))
        })?;
        state.files.insert(to_path.to_owned(), content);
        state
            .renames
            .push((from_path.to_owned(), to_path.to_owned()));
        Ok(())
    }
}

#[async_trait]
impl MountProvider for CanvasSourceBundleMountProvider {
    fn provider_id(&self) -> &str {
        PROVIDER_CANVAS_FS
    }

    fn display_name(&self) -> &str {
        "Canvas Source"
    }

    fn supported_capabilities(&self) -> Vec<&str> {
        vec!["read", "write", "list", "search"]
    }

    fn edit_capabilities(&self, mount: &Mount) -> MountEditCapabilities {
        if mount.supports(MountCapability::Write) {
            MountEditCapabilities {
                create: true,
                delete: true,
                rename: true,
            }
        } else {
            MountEditCapabilities::default()
        }
    }

    fn prefers_native_apply_patch(&self) -> bool {
        true
    }

    async fn apply_patch(
        &self,
        mount: &Mount,
        request: &ApplyPatchRequest,
        _ctx: &MountOperationContext,
    ) -> Result<ApplyPatchResult, MountError> {
        ensure_writable(mount)?;
        let (definition, current) = self.load_current(mount).await?;
        let target = SourceBundlePatchTarget::new(&current);
        let affected = apply_patch_to_target(&target, &request.patch)
            .await
            .map_err(operation_failed)?;
        let next_state = target.into_state();
        let original_files = current
            .source_bundle
            .files
            .iter()
            .map(|file| (file.path.clone(), file))
            .collect::<BTreeMap<_, _>>();
        let mut media_types = current
            .source_bundle
            .files
            .iter()
            .map(|file| (file.path.clone(), file.media_type.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut entry_file = current.source_bundle.entry_file.clone();
        for (from_path, to_path) in &next_state.renames {
            let media_type = media_types.remove(from_path).unwrap_or_default();
            media_types.insert(to_path.clone(), media_type);
            if entry_file == *from_path {
                entry_file = to_path.clone();
            }
        }
        let mut file_changes = original_files
            .keys()
            .filter(|path| !next_state.files.contains_key(*path))
            .map(|path| SourceFileChange::Delete { path: path.clone() })
            .collect::<Vec<_>>();
        for (path, content) in &next_state.files {
            if original_files
                .get(path)
                .is_some_and(|file| file.content == *content)
            {
                continue;
            }
            file_changes.push(SourceFileChange::Upsert(
                SourceFile::new(
                    path.clone(),
                    content.clone(),
                    media_types.remove(path).flatten(),
                )
                .map_err(operation_failed)?,
            ));
        }
        self.commit_loaded_changeset(
            definition,
            current,
            SourceBundleChangeset {
                entry_file: Some(entry_file),
                file_changes,
                ..SourceBundleChangeset::default()
            },
        )
        .await?;
        Ok(ApplyPatchResult {
            added: affected.added,
            modified: affected.modified,
            deleted: affected.deleted,
        })
    }

    async fn read_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<ReadResult, MountError> {
        let path = normalize_mount_relative_path(path, false).map_err(operation_failed)?;
        let (_, revision) = self.load_current(mount).await?;
        let file = revision
            .source_bundle
            .files
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| MountError::NotFound(format!("Canvas source 文件不存在: {path}")))?;
        let mut attributes = serde_json::Map::new();
        attributes.insert(
            "definition_id".to_owned(),
            serde_json::Value::String(revision.definition_id.to_string()),
        );
        attributes.insert(
            "definition_revision_id".to_owned(),
            serde_json::Value::String(revision.revision_id.to_string()),
        );
        attributes.insert(
            "source_bundle_digest".to_owned(),
            serde_json::Value::String(revision.source_bundle.digest.clone()),
        );
        if let Some(media_type) = &file.media_type {
            attributes.insert(
                "media_type".to_owned(),
                serde_json::Value::String(media_type.clone()),
            );
        }
        Ok(ReadResult::new(path, file.content.clone())
            .with_version_token(format!(
                "interaction-revision:{}:{}",
                revision.revision_id, revision.source_bundle.digest
            ))
            .with_modified_at(revision.created_at.timestamp_millis())
            .with_attributes(attributes))
    }

    async fn write_text(
        &self,
        mount: &Mount,
        path: &str,
        content: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        ensure_writable(mount)?;
        let path = normalize_mount_relative_path(path, false).map_err(operation_failed)?;
        let (_, current) = self.load_current(mount).await?;
        let media_type = current
            .source_bundle
            .files
            .iter()
            .find(|file| file.path == path)
            .and_then(|file| file.media_type.clone());
        let file = SourceFile::new(path, content, media_type).map_err(operation_failed)?;
        self.commit_changeset(
            mount,
            SourceBundleChangeset {
                file_changes: vec![SourceFileChange::Upsert(file)],
                ..SourceBundleChangeset::default()
            },
        )
        .await
    }

    async fn delete_text(
        &self,
        mount: &Mount,
        path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        ensure_writable(mount)?;
        let path = normalize_mount_relative_path(path, false).map_err(operation_failed)?;
        self.commit_changeset(
            mount,
            SourceBundleChangeset {
                file_changes: vec![SourceFileChange::Delete { path }],
                ..SourceBundleChangeset::default()
            },
        )
        .await
    }

    async fn rename_text(
        &self,
        mount: &Mount,
        from_path: &str,
        to_path: &str,
        _ctx: &MountOperationContext,
    ) -> Result<(), MountError> {
        ensure_writable(mount)?;
        let from_path =
            normalize_mount_relative_path(from_path, false).map_err(operation_failed)?;
        let to_path = normalize_mount_relative_path(to_path, false).map_err(operation_failed)?;
        let (_, current) = self.load_current(mount).await?;
        if current
            .source_bundle
            .files
            .iter()
            .any(|file| file.path == to_path)
        {
            return Err(MountError::OperationFailed(format!(
                "Canvas source 目标路径已存在: {to_path}"
            )));
        }
        let source = current
            .source_bundle
            .files
            .iter()
            .find(|file| file.path == from_path)
            .ok_or_else(|| {
                MountError::NotFound(format!("Canvas source 文件不存在: {from_path}"))
            })?;
        let target = SourceFile::new(
            to_path.clone(),
            source.content.clone(),
            source.media_type.clone(),
        )
        .map_err(operation_failed)?;
        self.commit_changeset(
            mount,
            SourceBundleChangeset {
                entry_file: (current.source_bundle.entry_file == from_path).then_some(to_path),
                file_changes: vec![
                    SourceFileChange::Delete { path: from_path },
                    SourceFileChange::Upsert(target),
                ],
                ..SourceBundleChangeset::default()
            },
        )
        .await
    }

    async fn list(
        &self,
        mount: &Mount,
        options: &ListOptions,
        _ctx: &MountOperationContext,
    ) -> Result<ListResult, MountError> {
        let path = normalize_mount_relative_path(&options.path, true).map_err(operation_failed)?;
        let (_, revision) = self.load_current(mount).await?;
        let files = revision
            .source_bundle
            .files
            .into_iter()
            .map(|file| (file.path, file.content))
            .collect::<BTreeMap<_, _>>();
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
        let (_, revision) = self.load_current(mount).await?;
        let base_path = query
            .path
            .as_deref()
            .map(|path| normalize_mount_relative_path(path, true).map_err(operation_failed))
            .transpose()?
            .unwrap_or_default();
        let max_results = query.max_results.unwrap_or(usize::MAX);
        let pattern = if query.case_sensitive {
            query.pattern.clone()
        } else {
            query.pattern.to_lowercase()
        };
        let mut matches = Vec::new();
        for file in revision.source_bundle.files {
            if !base_path.is_empty()
                && file.path != base_path
                && !file.path.starts_with(&format!("{base_path}/"))
            {
                continue;
            }
            for (index, line) in file.content.lines().enumerate() {
                let candidate = if query.case_sensitive {
                    line.to_owned()
                } else {
                    line.to_lowercase()
                };
                if !candidate.contains(&pattern) {
                    continue;
                }
                matches.push(SearchMatch {
                    path: file.path.clone(),
                    line: Some((index + 1) as u32),
                    content: line.trim().to_owned(),
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
            "Canvas authoring mount 不支持 exec".to_owned(),
        ))
    }
}

fn parse_definition_id(mount: &Mount) -> Result<Uuid, MountError> {
    let raw = if !mount.backend_id.trim().is_empty() {
        mount.backend_id.as_str()
    } else {
        mount
            .metadata
            .get("definition_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                MountError::OperationFailed("Canvas mount 缺少 definition identity".to_owned())
            })?
    };
    Uuid::parse_str(raw)
        .map_err(|error| MountError::OperationFailed(format!("definition_id 无效: {error}")))
}

fn ensure_writable(mount: &Mount) -> Result<(), MountError> {
    if mount.supports(MountCapability::Write) {
        Ok(())
    } else {
        Err(MountError::NotSupported(
            "Canvas authoring mount is read-only".to_owned(),
        ))
    }
}

fn operation_failed(error: impl std::fmt::Display) -> MountError {
    MountError::OperationFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use agentdash_domain::interaction::{
        InteractionDefinitionKind, InteractionError, InteractionOwner, SourceBundle,
        SourceSandboxConfig,
    };
    use tokio::sync::RwLock;

    use super::*;

    #[derive(Default)]
    struct FixtureDefinitions {
        definitions: RwLock<BTreeMap<Uuid, InteractionDefinition>>,
        revisions: RwLock<BTreeMap<Uuid, InteractionDefinitionRevision>>,
        commits: AtomicUsize,
    }

    #[async_trait]
    impl InteractionDefinitionRepository for FixtureDefinitions {
        async fn create(
            &self,
            definition: &InteractionDefinition,
            initial_revision: &InteractionDefinitionRevision,
        ) -> Result<(), InteractionError> {
            self.definitions
                .write()
                .await
                .insert(definition.id, definition.clone());
            self.revisions
                .write()
                .await
                .insert(initial_revision.revision_id, initial_revision.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<InteractionDefinition>, InteractionError> {
            Ok(self.definitions.read().await.get(&id).cloned())
        }

        async fn get_revision(
            &self,
            revision_id: Uuid,
        ) -> Result<Option<InteractionDefinitionRevision>, InteractionError> {
            Ok(self.revisions.read().await.get(&revision_id).cloned())
        }

        async fn list_by_owner(
            &self,
            owner: &InteractionOwner,
        ) -> Result<Vec<InteractionDefinition>, InteractionError> {
            Ok(self
                .definitions
                .read()
                .await
                .values()
                .filter(|definition| &definition.owner == owner)
                .cloned()
                .collect())
        }

        async fn list_canvas_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<InteractionDefinition>, InteractionError> {
            Ok(self
                .definitions
                .read()
                .await
                .values()
                .filter(|definition| {
                    definition.project_id == project_id
                        && definition.kind == InteractionDefinitionKind::Canvas
                })
                .cloned()
                .collect())
        }

        async fn commit_revision(
            &self,
            definition_id: Uuid,
            commit: DefinitionRevisionCommit,
        ) -> Result<InteractionDefinition, InteractionError> {
            let mut definitions = self.definitions.write().await;
            let definition =
                definitions
                    .get_mut(&definition_id)
                    .ok_or_else(|| InteractionError::NotFound {
                        entity: "interaction_definition",
                        id: definition_id.to_string(),
                    })?;
            if definition.current_revision_id != commit.expected_current_revision_id {
                return Err(InteractionError::DefinitionRevisionConflict {
                    definition_id,
                    expected_revision_id: commit.expected_current_revision_id,
                    actual_revision_id: definition.current_revision_id,
                });
            }
            definition.current_revision_id = commit.revision.revision_id;
            definition.updated_at = commit.revision.created_at;
            self.revisions
                .write()
                .await
                .insert(commit.revision.revision_id, commit.revision);
            self.commits.fetch_add(1, Ordering::SeqCst);
            Ok(definition.clone())
        }

        async fn archive(
            &self,
            definition_id: Uuid,
        ) -> Result<InteractionDefinition, InteractionError> {
            let mut definitions = self.definitions.write().await;
            let definition =
                definitions
                    .get_mut(&definition_id)
                    .ok_or_else(|| InteractionError::NotFound {
                        entity: "interaction_definition",
                        id: definition_id.to_string(),
                    })?;
            definition.status = InteractionDefinitionStatus::Archived;
            Ok(definition.clone())
        }
    }

    #[tokio::test]
    async fn native_multi_file_patch_commits_one_definition_revision() {
        let definition_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let source = SourceBundle::new(
            "src/main.tsx",
            vec![
                SourceFile::new("src/main.tsx", "const value = 'old';\n", None)
                    .expect("main source"),
                SourceFile::new(
                    "src/old.css",
                    "body { color: red; }\n",
                    Some("text/css".into()),
                )
                .expect("css source"),
            ],
            SourceSandboxConfig::default(),
        )
        .expect("source bundle");
        let revision = InteractionDefinitionRevision::new_canvas_v1(
            definition_id,
            1,
            project_id,
            InteractionOwner::User("user-1".into()),
            "Canvas",
            "",
            source,
            serde_json::json!({}),
            serde_json::json!({"type":"object"}),
            "user-1",
        )
        .expect("revision");
        let mount_id = revision.authoring_mount_id.clone();
        let (definition, revision) = revision
            .into_initial_definition()
            .expect("initial definition");
        let repository = Arc::new(FixtureDefinitions::default());
        repository
            .create(&definition, &revision)
            .await
            .expect("seed definition");
        let provider = CanvasSourceBundleMountProvider::new(repository.clone());
        let mount = Mount {
            id: mount_id,
            provider: PROVIDER_CANVAS_FS.into(),
            backend_id: definition_id.to_string(),
            root_ref: format!("canvas-root://{definition_id}"),
            capabilities: vec![
                MountCapability::Read,
                MountCapability::Write,
                MountCapability::List,
                MountCapability::Search,
            ],
            default_write: true,
            display_name: "Canvas".into(),
            metadata: serde_json::json!({}),
        };

        provider
            .apply_patch(
                &mount,
                &ApplyPatchRequest {
                    patch: "*** Begin Patch\n*** Update File: src/main.tsx\n@@\n-const value = 'old';\n+const value = 'new';\n*** Delete File: src/old.css\n*** Add File: src/theme.css\n+body { color: blue; }\n*** End Patch".into(),
                },
                &MountOperationContext::default(),
            )
            .await
            .expect("apply patch");

        assert_eq!(repository.commits.load(Ordering::SeqCst), 1);
        let current = repository
            .get(definition_id)
            .await
            .expect("load definition")
            .expect("definition");
        let current_revision = repository
            .get_revision(current.current_revision_id)
            .await
            .expect("load revision")
            .expect("revision");
        assert_eq!(current_revision.revision_number, 2);
        assert_eq!(
            current_revision
                .source_bundle
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/main.tsx", "src/theme.css"]
        );
    }
}
