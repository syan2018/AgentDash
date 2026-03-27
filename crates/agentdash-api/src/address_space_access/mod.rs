/// Address Space 访问层 — Relay 传输实现与 Runtime 工具
///
/// 值类型、路径工具和 Mount 推导逻辑已迁移到 `agentdash_application::address_space`。
pub use agentdash_application::address_space::*;
pub use agentdash_executor::{ExecutionAddressSpace, ExecutionMountCapability};

mod inline_persistence;
mod relay_service;
mod tools_fs;
mod tools_hook;
mod tools_workflow;
mod tools_companion;
mod runtime_provider;

pub use inline_persistence::{
    DbInlineContentPersister, InlineContentOverlay, InlineContentPersister,
};
pub use relay_service::RelayAddressSpaceService;
pub use runtime_provider::{RelayRuntimeToolProvider, SharedExecutorHubHandle};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use agentdash_agent::AgentTool;
    use agentdash_application::runtime::{RuntimeAddressSpace, RuntimeMount};
    use agentdash_relay::RelayMessage;
    use chrono::Utc;
    use tokio::sync::mpsc;

    use super::tools_fs::{
        FsListTool, FsReadTool, FsSearchTool, FsWriteTool, MountsListTool, ShellExecTool,
    };

    use agentdash_domain::context_container::{
        ContextContainerCapability, ContextContainerDefinition, ContextContainerExposure,
        ContextContainerFile, ContextContainerProvider, MountDerivationPolicy,
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
                ContextContainerCapability::Read,
                ContextContainerCapability::List,
                ContextContainerCapability::Search,
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

    fn mount_registry_with_inline_fs() -> Arc<MountProviderRegistry> {
        let mut registry = MountProviderRegistry::new();
        registry.register(Arc::new(InlineFsMountProvider));
        Arc::new(registry)
    }

    #[test]
    fn session_for_workspace_creates_main_mount() {
        let service = RelayAddressSpaceService::new(empty_mount_registry());
        let session = service
            .session_for_workspace(&sample_workspace())
            .expect("session should build");
        assert_eq!(session.default_mount_id.as_deref(), Some("main"));
        assert_eq!(session.mounts.len(), 1);
        assert!(session.mounts[0].supports(ExecutionMountCapability::Exec));
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
        project.config.mount_policy = MountDerivationPolicy {
            include_local_workspace: true,
            local_workspace_capabilities: vec![
                ContextContainerCapability::Read,
                ContextContainerCapability::List,
            ],
        };

        let mut story =
            agentdash_domain::story::Story::new(project.id, "story".into(), "desc".into());
        story.context.context_containers = vec![inline_container(
            "story-brief",
            "brief",
            "brief.md",
            "story brief",
        )];

        let address_space = service
            .build_address_space(
                &project,
                Some(&story),
                Some(&sample_workspace()),
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
        assert!(!main.supports(ExecutionMountCapability::Exec));
        assert!(main.supports(ExecutionMountCapability::Read));
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
            .build_address_space(&project, Some(&story), None, SessionMountTarget::Task, Some("PI_AGENT"))
            .expect("address space should build");

        assert_eq!(address_space.mounts.len(), 1);
        let mount = &address_space.mounts[0];
        assert_eq!(mount.id, "shared");
        let files =
            inline_files_from_mount(&RuntimeMount::from(mount))
                .expect("inline files");
        assert_eq!(
            files.get("spec.md").map(String::as_str),
            Some("story override")
        );
    }

    #[tokio::test]
    async fn inline_mount_supports_read_list_and_search() {
        let service = RelayAddressSpaceService::new(mount_registry_with_inline_fs());
        let runtime_address_space = RuntimeAddressSpace {
            mounts: vec![
                build_context_container_mount(&ContextContainerDefinition {
                    id: "story-brief".to_string(),
                    mount_id: "brief".to_string(),
                    display_name: "brief".to_string(),
                    provider: ContextContainerProvider::InlineFiles {
                        files: vec![
                            ContextContainerFile {
                                path: "brief.md".to_string(),
                                content: "hello inline mount".to_string(),
                            },
                            ContextContainerFile {
                                path: "notes/todo.md".to_string(),
                                content: "todo: verify inline search".to_string(),
                            },
                        ],
                    },
                    capabilities: vec![
                        ContextContainerCapability::Read,
                        ContextContainerCapability::List,
                        ContextContainerCapability::Search,
                    ],
                    default_write: false,
                    exposure: ContextContainerExposure::default(),
                })
                .expect("mount should build"),
            ],
            default_mount_id: Some("brief".to_string()),
            ..Default::default()
        };
        let address_space = runtime_address_space.to_execution_address_space();

        let read = service
            .read_text(
                &address_space,
                &ResourceRef {
                    mount_id: "brief".to_string(),
                    path: "brief.md".to_string(),
                },
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
                    supports_workspace_files: true,
                    supports_discover_options: true,
                },
                accessible_roots: vec!["/workspace".to_string()],
                sender,
                connected_at: Utc::now(),
            })
            .await
            .expect("backend should register");

        let mut mount_registry = MountProviderRegistry::new();
        mount_registry.register(Arc::new(
            crate::mount_providers::RelayFsMountProvider::new(registry.clone()),
        ));
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
                    )
                    .await
            }
        });

        let message = receiver.recv().await.expect("command sent");
        let id = message.id().to_string();
        match message {
            RelayMessage::CommandToolFileRead { payload, .. } => {
                assert_eq!(payload.workspace_root, "/workspace/repo");
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
        let address_space = ExecutionAddressSpace {
            mounts: vec![agentdash_executor::ExecutionMount {
                id: "brief".to_string(),
                provider: PROVIDER_INLINE_FS.to_string(),
                backend_id: String::new(),
                root_ref: "context://inline/brief".to_string(),
                capabilities: vec![
                    ExecutionMountCapability::Read,
                    ExecutionMountCapability::List,
                    ExecutionMountCapability::Search,
                ],
                default_write: false,
                display_name: "brief".to_string(),
                metadata: serde_json::json!({ "files": { "brief.md": "hello" } }),
            }],
            default_mount_id: Some("brief".to_string()),
            ..Default::default()
        };

        let schemas = vec![
            MountsListTool::new(service.clone(), address_space.clone()).parameters_schema(),
            FsReadTool::new(service.clone(), address_space.clone(), None).parameters_schema(),
            FsWriteTool::new(service.clone(), address_space.clone(), None).parameters_schema(),
            FsListTool::new(service.clone(), address_space.clone(), None).parameters_schema(),
            FsSearchTool::new(service.clone(), address_space.clone(), None).parameters_schema(),
            ShellExecTool::new(service, address_space).parameters_schema(),
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
