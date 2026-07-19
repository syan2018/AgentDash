use agentdash_agent_runtime::{
    RuntimeToolDefinition, RuntimeToolEffect, RuntimeToolExecutor, RuntimeToolInvocation,
    RuntimeToolPermission, RuntimeToolResourceGrant, RuntimeVfsGrantedOperation,
};
use agentdash_agent_service_api::{AgentToolName, AgentToolResult};
use async_trait::async_trait;

/// Final Runtime Tool Broker executor over the immutable applied Product VFS projection.
pub struct MountsListRuntimeTool;

#[async_trait]
impl RuntimeToolExecutor for MountsListRuntimeTool {
    fn definition(&self) -> RuntimeToolDefinition {
        RuntimeToolDefinition {
            name: AgentToolName::new("mounts_list").expect("static runtime tool name"),
            description: "List VFS mounts granted by the applied AgentRun resource surface."
                .to_owned(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }),
            permission: RuntimeToolPermission::VfsRead,
            effect: RuntimeToolEffect::ReadOnly,
        }
    }

    async fn execute(&self, invocation: RuntimeToolInvocation) -> AgentToolResult {
        let RuntimeToolResourceGrant::Vfs(vfs) = invocation.grant.resources else {
            return AgentToolResult::Rejected {
                code: "runtime_vfs_grant_required".to_owned(),
                message: "mounts_list requires a typed VFS execution grant".to_owned(),
            };
        };
        let mounts = vfs
            .mounts
            .into_iter()
            .filter(|mount| mount.operations.contains(&RuntimeVfsGrantedOperation::List))
            .map(|mount| {
                serde_json::json!({
                    "id": mount.id,
                    "display_name": mount.display_name,
                    "operations": mount.operations,
                })
            })
            .collect::<Vec<_>>();
        AgentToolResult::Completed {
            output: serde_json::json!({ "mounts": mounts }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_runtime::{
        RuntimeToolAppliedSurfaceEvidence, RuntimeToolAuthorizationGrant, RuntimeToolProductTarget,
        RuntimeToolResolvedContext, RuntimeVfsExecutionGrant, RuntimeVfsMountGrant,
        RuntimeVfsPathGrant,
    };
    use agentdash_agent_runtime_contract::RuntimeThreadId;
    use agentdash_agent_service_api::{
        AgentBindingGeneration, AgentProfileDigest, AgentServiceInstanceId, AgentSourceCoordinate,
        AgentSurfaceDigest, AgentSurfaceRevision,
    };

    #[tokio::test]
    async fn mounts_list_only_returns_explicitly_allowed_mounts() {
        let executor = MountsListRuntimeTool;
        let result = executor.execute(invocation()).await;
        let AgentToolResult::Completed { output } = result else {
            panic!("mounts_list must complete");
        };
        assert_eq!(output["mounts"].as_array().unwrap().len(), 1);
        assert_eq!(output["mounts"][0]["id"], "main");
        assert!(
            !output.to_string().contains("secret"),
            "mount without an explicit List grant must remain invisible"
        );
    }

    #[tokio::test]
    async fn stateless_executor_does_not_share_mounts_between_agent_runs() {
        let executor = MountsListRuntimeTool;
        let first = invocation();
        let mut second = invocation();
        second.grant.target.run_id = "run-other".to_owned();
        second.grant.resources = RuntimeToolResourceGrant::Vfs(RuntimeVfsExecutionGrant {
            default_mount_id: Some("other".to_owned()),
            mounts: vec![mount("other", vec![RuntimeVfsGrantedOperation::List])],
        });

        let AgentToolResult::Completed { output: first } = executor.execute(first).await else {
            panic!("first mounts_list must complete");
        };
        let AgentToolResult::Completed { output: second } = executor.execute(second).await else {
            panic!("second mounts_list must complete");
        };
        assert_eq!(first["mounts"][0]["id"], "main");
        assert_eq!(second["mounts"][0]["id"], "other");
        assert!(!second.to_string().contains("main"));
    }

    fn mount(id: &str, operations: Vec<RuntimeVfsGrantedOperation>) -> RuntimeVfsMountGrant {
        RuntimeVfsMountGrant {
            id: id.to_owned(),
            provider: "memory".to_owned(),
            backend_id: String::new(),
            root_ref: format!("memory://{id}"),
            display_name: id.to_owned(),
            metadata: serde_json::Value::Null,
            operations,
            path_scopes: vec![RuntimeVfsPathGrant::All],
        }
    }

    fn invocation() -> RuntimeToolInvocation {
        RuntimeToolInvocation {
            context: RuntimeToolResolvedContext {
                runtime_thread_id: RuntimeThreadId::new("thread-test").unwrap(),
                binding_generation: AgentBindingGeneration(1),
                source: AgentSourceCoordinate::new("source-test").unwrap(),
                service_instance_id: AgentServiceInstanceId::new("service-test").unwrap(),
                profile_digest: AgentProfileDigest::new("profile-test").unwrap(),
                bound_surface_revision: AgentSurfaceRevision(1),
                bound_surface_digest: AgentSurfaceDigest::new("bound-test").unwrap(),
                applied_surface_revision: AgentSurfaceRevision(1),
                applied_surface_digest: AgentSurfaceDigest::new("applied-test").unwrap(),
            },
            tool: AgentToolName::new("mounts_list").unwrap(),
            arguments: serde_json::json!({}),
            grant: RuntimeToolAuthorizationGrant {
                permission: RuntimeToolPermission::VfsRead,
                effect: RuntimeToolEffect::ReadOnly,
                target: RuntimeToolProductTarget {
                    project_id: "project-test".to_owned(),
                    run_id: "run-test".to_owned(),
                    agent_id: "agent-test".to_owned(),
                },
                applied_surface: RuntimeToolAppliedSurfaceEvidence {
                    snapshot_revision: 1,
                    revision: 1,
                    digest: "surface-test".to_owned(),
                    projection_revision: 1,
                    provenance_source: "test".to_owned(),
                    provenance_revision: 1,
                },
                resources: RuntimeToolResourceGrant::Vfs(RuntimeVfsExecutionGrant {
                    default_mount_id: Some("main".to_owned()),
                    mounts: vec![
                        mount(
                            "main",
                            vec![
                                RuntimeVfsGrantedOperation::Read,
                                RuntimeVfsGrantedOperation::List,
                            ],
                        ),
                        mount("secret", vec![RuntimeVfsGrantedOperation::Read]),
                    ],
                }),
            },
        }
    }
}
