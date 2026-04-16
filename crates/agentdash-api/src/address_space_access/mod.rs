/// Address Space 集成测试 — 需要 API 层组件（BackendRegistry、MountProvider 等）
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use agentdash_application::address_space::inline_persistence::{
        InlineContentOverlay, InlineContentPersister,
    };
    use agentdash_application::address_space::*;
    use agentdash_spi::{AddressSpace, MountCapability};

    use agentdash_agent::AgentTool;
    use agentdash_domain::common::error::DomainError;
    use agentdash_domain::inline_file::{InlineFile, InlineFileOwnerKind, InlineFileRepository};
    use agentdash_relay::RelayMessage;
    use async_trait::async_trait;
    use chrono::Utc;
    use tokio::sync::{Mutex, mpsc};

    use agentdash_application::address_space::tools::fs::{
        FsApplyPatchTool, FsGlobTool, FsGrepTool, FsReadTool, MountsListTool,
        SharedRuntimeAddressSpace, ShellExecTool,
    };

    // `MountCapability` 统一使用 agentdash_spi 版本，避免重复导入
    use agentdash_domain::context_container::{
        ContextContainerDefinition, ContextContainerExposure, ContextContainerFile,
        ContextContainerProvider,
    };
    use agentdash_domain::workspace::Workspace;

    use crate::relay::registry::ConnectedBackend;

    fn sample_workspace() -> Workspace {
        let mut workspace = Workspace::new(
            uuid::Uuid::new_v4(),
            "repo".to_string(),
            agentdash_domain::workspace::WorkspaceIdentityKind::LocalDir,
            serde_json::json!({ "root_hint": "/workspace/repo" }),
            agentdash_domain::workspace::WorkspaceResolutionPolicy::PreferOnline,
        );
        let mut binding = agentdash_domain::workspace::WorkspaceBinding::new(
            workspace.id,
            "backend-a".to_string(),
            "/workspace/repo".to_string(),
            serde_json::json!({}),
        );
        binding.status = agentdash_domain::workspace::WorkspaceBindingStatus::Ready;
        workspace.status = agentdash_domain::workspace::WorkspaceStatus::Ready;
        workspace.set_bindings(vec![binding]);
        workspace.refresh_default_binding();
        workspace
    }

    fn inline_container(
        id: &str,
        mount_id: &str,
        path: &str,
        content: &str,
    ) -> ContextContainerDefinition {
        ContextContainerDefinition {
            id: id.to_string(),
            mount_id: mount_id.to_string(),
            display_name: id.to_string(),
            provider: ContextContainerProvider::InlineFiles {
                files: vec![ContextContainerFile {
                    path: path.to_string(),
                    content: content.to_string(),
                }],
            },
            capabilities: vec![
                MountCapability::Read,
                MountCapability::List,
                MountCapability::Search,
            ],
            default_write: false,
            exposure: ContextContainerExposure::default(),
        }
    }

    #[test]
    fn normalize_mount_relative_path_blocks_escape() {
        let err = normalize_mount_relative_path("../secret", false).expect_err("should fail");
        assert!(err.contains("路径越界"));
    }

    fn empty_mount_registry() -> Arc<MountProviderRegistry> {
        Arc::new(MountProviderRegistry::new())
    }

    /// 内存中的 InlineFileRepository，用于测试
    #[derive(Default, Clone)]
    struct MemoryInlineFileRepo {
        files: Arc<Mutex<Vec<InlineFile>>>,
    }

    impl MemoryInlineFileRepo {
        fn new_with_files(files: Vec<InlineFile>) -> Self {
            Self {
                files: Arc::new(Mutex::new(files)),
            }
        }
    }

    #[async_trait]
    impl InlineFileRepository for MemoryInlineFileRepo {
        async fn get_file(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: uuid::Uuid,
            container_id: &str,
            path: &str,
        ) -> Result<Option<InlineFile>, DomainError> {
            let files = self.files.lock().await;
            Ok(files.iter().find(|f| {
                f.owner_kind == owner_kind
                    && f.owner_id == owner_id
                    && f.container_id == container_id
                    && f.path == path
            }).cloned())
        }

        async fn list_files(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: uuid::Uuid,
            container_id: &str,
        ) -> Result<Vec<InlineFile>, DomainError> {
            let files = self.files.lock().await;
            Ok(files.iter().filter(|f| {
                f.owner_kind == owner_kind
                    && f.owner_id == owner_id
                    && f.container_id == container_id
            }).cloned().collect())
        }

        async fn list_files_by_owner(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: uuid::Uuid,
        ) -> Result<Vec<InlineFile>, DomainError> {
            let files = self.files.lock().await;
            Ok(files.iter().filter(|f| {
                f.owner_kind == owner_kind && f.owner_id == owner_id
            }).cloned().collect())
        }

        async fn upsert_file(&self, file: &InlineFile) -> Result<(), DomainError> {
            let mut files = self.files.lock().await;
            if let Some(existing) = files.iter_mut().find(|f| {
                f.owner_kind == file.owner_kind
                    && f.owner_id == file.owner_id
                    && f.container_id == file.container_id
                    && f.path == file.path
            }) {
                existing.content = file.content.clone();
                existing.updated_at = file.updated_at;
            } else {
                files.push(file.clone());
            }
            Ok(())
        }

        async fn upsert_files(&self, new_files: &[InlineFile]) -> Result<(), DomainError> {
            for file in new_files {
                self.upsert_file(file).await?;
            }
            Ok(())
        }

        async fn delete_file(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: uuid::Uuid,
            container_id: &str,
            path: &str,
        ) -> Result<(), DomainError> {
            let mut files = self.files.lock().await;
            files.retain(|f| {
                !(f.owner_kind == owner_kind
                    && f.owner_id == owner_id
                    && f.container_id == container_id
                    && f.path == path)
            });
            Ok(())
        }

        async fn delete_by_container(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: uuid::Uuid,
            container_id: &str,
        ) -> Result<(), DomainError> {
            let mut files = self.files.lock().await;
            files.retain(|f| {
                !(f.owner_kind == owner_kind
                    && f.owner_id == owner_id
                    && f.container_id == container_id)
            });
            Ok(())
        }

        async fn delete_by_owner(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: uuid::Uuid,
        ) -> Result<(), DomainError> {
            let mut files = self.files.lock().await;
            files.retain(|f| !(f.owner_kind == owner_kind && f.owner_id == owner_id));
            Ok(())
        }

        async fn count_files(
            &self,
            owner_kind: InlineFileOwnerKind,
            owner_id: uuid::Uuid,
            container_id: &str,
        ) -> Result<i64, DomainError> {
            let files = self.files.lock().await;
            Ok(files.iter().filter(|f| {
                f.owner_kind == owner_kind
                    && f.owner_id == owner_id
                    && f.container_id == container_id
            }).count() as i64)
        }
    }

    fn mount_registry_with_inline_fs_repo(
        repo: Arc<dyn InlineFileRepository>,
    ) -> Arc<MountProviderRegistry> {
        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(InlineFsMountProvider::new(repo)));
        Arc::new(registry)
    }

    /// 构建带 owner 坐标的 inline mount（模拟 build_derived_address_space 的输出）
    fn make_inline_mount_with_owner(
        mount_id: &str,
        container_id: &str,
        owner_kind: &str,
        owner_id: uuid::Uuid,
        capabilities: Vec<MountCapability>,
        default_write: bool,
    ) -> agentdash_spi::Mount {
        agentdash_spi::Mount {
            id: mount_id.to_string(),
            provider: PROVIDER_INLINE_FS.to_string(),
            backend_id: String::new(),
            root_ref: format!("context://inline/{container_id}"),
            capabilities,
            default_write,
            display_name: mount_id.to_string(),
            metadata: serde_json::json!({
                "container_id": container_id,
                "agentdash_context_owner_kind": owner_kind,
                "agentdash_context_owner_id": owner_id.to_string(),
            }),
        }
    }

    #[derive(Default)]
    struct MemoryInlinePersister;

    #[async_trait]
    impl InlineContentPersister for MemoryInlinePersister {
        async fn persist_write(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: uuid::Uuid,
            _container_id: &str,
            _path: &str,
            _content: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn persist_delete(
            &self,
            _owner_kind: InlineFileOwnerKind,
            _owner_id: uuid::Uuid,
            _container_id: &str,
            _path: &str,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn session_for_workspace_creates_main_mount() {
        let service = RelayAddressSpaceService::new(empty_mount_registry());
        let session = service
            .session_for_workspace(&sample_workspace())
            .expect("session should build");
        assert_eq!(session.default_mount_id.as_deref(), Some("main"));
        assert_eq!(session.mounts.len(), 1);
        assert!(session.mounts[0].supports(MountCapability::Exec));
    }

    #[test]
    fn build_task_address_space_merges_project_story_and_workspace_policy() {
        let service = RelayAddressSpaceService::new(empty_mount_registry());
        let mut project = agentdash_domain::project::Project::new("proj".into(), "desc".into());
        project.config.context_containers = vec![inline_container(
            "project-spec",
            "spec",
            "backend/spec.md",
            "# spec",
        )];

        let mut story =
            agentdash_domain::story::Story::new(project.id, "story".into(), "desc".into());
        story.context.context_containers = vec![inline_container(
            "story-brief",
            "brief",
            "brief.md",
            "story brief",
        )];

        let mut ws = sample_workspace();
        ws.mount_capabilities = vec![MountCapability::Read, MountCapability::List];

        let address_space = service
            .build_address_space(
                &project,
                Some(&story),
                Some(&ws),
                SessionMountTarget::Task,
                Some("PI_AGENT"),
            )
            .expect("address space should build");

        assert_eq!(address_space.default_mount_id.as_deref(), Some("main"));
        assert_eq!(address_space.mounts.len(), 3);
        let main = address_space
            .mounts
            .iter()
            .find(|m| m.id == "main")
            .expect("main mount");
        assert!(!main.supports(MountCapability::Exec));
        assert!(main.supports(MountCapability::Read));
        assert!(address_space.mounts.iter().any(|m| m.id == "spec"));
        assert!(address_space.mounts.iter().any(|m| m.id == "brief"));
    }

    #[test]
    fn story_containers_can_disable_and_override_project_defaults() {
        let service = RelayAddressSpaceService::new(empty_mount_registry());
        let mut project = agentdash_domain::project::Project::new("proj".into(), "desc".into());
        project.config.context_containers = vec![
            inline_container("project-spec", "shared", "spec.md", "project spec"),
            inline_container("project-km", "km", "index.md", "project km"),
        ];

        let mut story =
            agentdash_domain::story::Story::new(project.id, "story".into(), "desc".into());
        story.context.disabled_container_ids = vec!["project-km".into()];
        story.context.context_containers = vec![inline_container(
            "story-spec",
            "shared",
            "spec.md",
            "story override",
        )];

        let address_space = service
            .build_address_space(
                &project,
                Some(&story),
                None,
                SessionMountTarget::Task,
                Some("PI_AGENT"),
            )
            .expect("address space should build");

        assert_eq!(address_space.mounts.len(), 1);
        let mount = &address_space.mounts[0];
        assert_eq!(mount.id, "shared");
        // 验证 mount metadata 包含 container_id 和 owner 坐标
        assert_eq!(
            mount.metadata.get("container_id").and_then(|v| v.as_str()),
            Some("story-spec")
        );
    }

    #[tokio::test]
    async fn inline_mount_supports_read_list_and_search() {
        let owner_id = uuid::Uuid::new_v4();
        let container_id = "story-brief";
        let repo = MemoryInlineFileRepo::new_with_files(vec![
            InlineFile::new(InlineFileOwnerKind::Project, owner_id, container_id, "brief.md", "hello inline mount"),
            InlineFile::new(InlineFileOwnerKind::Project, owner_id, container_id, "notes/todo.md", "todo: verify inline search"),
        ]);
        let service = RelayAddressSpaceService::new(
            mount_registry_with_inline_fs_repo(Arc::new(repo)),
        );
        let address_space = AddressSpace {
            mounts: vec![make_inline_mount_with_owner(
                "brief",
                container_id,
                "project",
                owner_id,
                vec![
                    MountCapability::Read,
                    MountCapability::List,
                    MountCapability::Search,
                ],
                false,
            )],
            default_mount_id: Some("brief".to_string()),
            ..Default::default()
        };

        let read = service
            .read_text(
                &address_space,
                &ResourceRef {
                    mount_id: "brief".to_string(),
                    path: "brief.md".to_string(),
                },
                None,
                None,
            )
            .await
            .expect("inline read");
        assert_eq!(read.content, "hello inline mount");

        let listed = service
            .list(
                &address_space,
                "brief",
                ListOptions {
                    path: ".".to_string(),
                    pattern: None,
                    recursive: true,
                },
                None,
                None,
            )
            .await
            .expect("inline list");
        assert!(listed.entries.iter().any(|e| e.path == "brief.md"));
        assert!(listed.entries.iter().any(|e| e.path == "notes/todo.md"));

        let hits = service
            .search_text(&address_space, "brief", ".", "verify", 10, None)
            .await
            .expect("inline search");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].contains("notes/todo.md:1"));
    }

    #[tokio::test]
    async fn inline_mount_supports_apply_patch_via_overlay() {
        let owner_id = uuid::Uuid::new_v4();
        let container_id = "story-brief";
        let repo = MemoryInlineFileRepo::new_with_files(vec![
            InlineFile::new(InlineFileOwnerKind::Project, owner_id, container_id, "brief.md", "hello inline mount\n"),
            InlineFile::new(InlineFileOwnerKind::Project, owner_id, container_id, "obsolete.md", "remove me\n"),
        ]);
        let service = RelayAddressSpaceService::new(
            mount_registry_with_inline_fs_repo(Arc::new(repo)),
        );
        let runtime_address_space = AddressSpace {
            mounts: vec![make_inline_mount_with_owner(
                "brief",
                container_id,
                "project",
                owner_id,
                vec![
                    MountCapability::Read,
                    MountCapability::Write,
                    MountCapability::List,
                    MountCapability::Search,
                ],
                true,
            )],
            default_mount_id: Some("brief".to_string()),
            source_project_id: Some(owner_id.to_string()),
            source_story_id: None,
        };
        let overlay = InlineContentOverlay::new(Arc::new(MemoryInlinePersister));

        let patch = r#"*** Begin Patch
*** Update File: brief.md
*** Move to: docs/brief.md
@@
-hello inline mount
+hello patched inline mount
*** Delete File: obsolete.md
*** Add File: new.md
+new inline file
*** End Patch"#;

        let result = service
            .apply_patch(&runtime_address_space, "brief", patch, Some(&overlay), None)
            .await
            .expect("inline patch");
        assert_eq!(result.modified, vec!["docs/brief.md".to_string()]);
        assert_eq!(result.deleted, vec!["obsolete.md".to_string()]);
        assert_eq!(result.added, vec!["new.md".to_string()]);

        let moved = service
            .read_text(
                &runtime_address_space,
                &ResourceRef {
                    mount_id: "brief".to_string(),
                    path: "docs/brief.md".to_string(),
                },
                Some(&overlay),
                None,
            )
            .await
            .expect("read moved file");
        assert_eq!(moved.content, "hello patched inline mount\n");

        let listed = service
            .list(
                &runtime_address_space,
                "brief",
                ListOptions {
                    path: ".".to_string(),
                    pattern: None,
                    recursive: true,
                },
                Some(&overlay),
                None,
            )
            .await
            .expect("inline list after patch");
        assert!(
            listed
                .entries
                .iter()
                .any(|entry| entry.path == "docs/brief.md")
        );
        assert!(listed.entries.iter().any(|entry| entry.path == "new.md"));
        assert!(
            !listed
                .entries
                .iter()
                .any(|entry| entry.path == "obsolete.md")
        );

        let hits = service
            .search_text(
                &runtime_address_space,
                "brief",
                ".",
                "patched inline",
                10,
                Some(&overlay),
            )
            .await
            .expect("search patched inline");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].contains("docs/brief.md:1"));
    }

    #[tokio::test]
    async fn read_text_routes_via_tool_transport() {
        let registry = crate::relay::registry::BackendRegistry::new();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        registry
            .try_register(ConnectedBackend {
                backend_id: "backend-a".to_string(),
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                capabilities: agentdash_relay::CapabilitiesPayload {
                    executors: Vec::new(),
                    supports_cancel: true,
                    supports_discover_options: true,
                    mcp_servers: Vec::new(),
                },
                accessible_roots: vec!["/workspace".to_string()],
                sender,
                connected_at: Utc::now(),
            })
            .await
            .expect("backend should register");

        let mut mount_registry = MountProviderRegistry::new();
        mount_registry.register(Arc::new(crate::mount_providers::RelayFsMountProvider::new(
            registry.clone(),
        )));
        let service = RelayAddressSpaceService::new(Arc::new(mount_registry));
        let session = service
            .session_for_workspace(&sample_workspace())
            .expect("session");

        let handle = tokio::spawn({
            let service = service.clone();
            let session = session.clone();
            async move {
                service
                    .read_text(
                        &session,
                        &ResourceRef {
                            mount_id: "main".to_string(),
                            path: "src/main.rs".to_string(),
                        },
                        None,
                        None,
                    )
                    .await
            }
        });

        let message = receiver.recv().await.expect("command sent");
        let id = message.id().to_string();
        match message {
            RelayMessage::CommandToolFileRead { payload, .. } => {
                assert_eq!(payload.mount_root_ref, "/workspace/repo");
                assert_eq!(payload.path, "src/main.rs");
            }
            other => panic!("unexpected: {other:?}"),
        }

        let resolved = registry
            .resolve_response(&RelayMessage::ResponseToolFileRead {
                id,
                payload: Some(agentdash_relay::ToolFileReadResponse {
                    call_id: "call".to_string(),
                    content: "fn main() {}".to_string(),
                    encoding: "utf-8".to_string(),
                }),
                error: None,
            })
            .await;
        assert!(resolved);

        let result = handle.await.expect("task").expect("read");
        assert_eq!(result.content, "fn main() {}");
    }

    #[test]
    fn runtime_tool_schemas_are_openai_compatible() {
        let service = Arc::new(RelayAddressSpaceService::new(empty_mount_registry()));
        let address_space = AddressSpace {
            mounts: vec![agentdash_spi::Mount {
                id: "brief".to_string(),
                provider: PROVIDER_INLINE_FS.to_string(),
                backend_id: String::new(),
                root_ref: "context://inline/brief".to_string(),
                capabilities: vec![
                    MountCapability::Read,
                    MountCapability::List,
                    MountCapability::Search,
                ],
                default_write: false,
                display_name: "brief".to_string(),
                metadata: serde_json::json!({
                    "container_id": "brief",
                    "agentdash_context_owner_kind": "project",
                    "agentdash_context_owner_id": uuid::Uuid::new_v4().to_string(),
                }),
            }],
            default_mount_id: Some("brief".to_string()),
            ..Default::default()
        };

        let shared_address_space = SharedRuntimeAddressSpace::new(address_space);

        let schemas = vec![
            MountsListTool::new(service.clone(), shared_address_space.clone()).parameters_schema(),
            FsReadTool::new(service.clone(), shared_address_space.clone(), None, None)
                .parameters_schema(),
            FsApplyPatchTool::new(service.clone(), shared_address_space.clone(), None, None)
                .parameters_schema(),
            FsGlobTool::new(service.clone(), shared_address_space.clone(), None, None)
                .parameters_schema(),
            FsGrepTool::new(service.clone(), shared_address_space.clone(), None, None)
                .parameters_schema(),
            ShellExecTool::new(service, shared_address_space).parameters_schema(),
        ];

        for schema in schemas {
            let properties = schema["properties"].as_object().expect("properties");
            let required = schema["required"]
                .as_array()
                .expect("required")
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
            for key in properties.keys() {
                assert!(
                    required.contains(key.as_str()),
                    "required should contain `{key}`"
                );
            }
        }
    }
}
