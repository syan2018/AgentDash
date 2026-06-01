//! `SessionConstructionProvider` 的 API 层实现。
//!
//! RuntimeSession prompt 进入 connector 前，必须先定位真实 AgentFrame，再把 compose
//! 结果写成新的 frame revision，最后从 frame 投影 RuntimeLaunchRequest。
//!
//! 为什么放这里：frame compose 逻辑依赖 `Arc<AppState>`（repos、services、platform_config），
//! 这些都是 API 层构造的；把 trait impl 也放在 API 层最自然，也不必把依赖下沉到
//! application crate。

use std::sync::Arc;

use async_trait::async_trait;

use agentdash_application::session::types::{
    SessionPromptLifecycle, SessionRepositoryRehydrateMode, UserPromptInput,
    resolve_session_prompt_lifecycle,
};
use agentdash_application::session::{
    AgentLevelMcp, AssemblyLaunchExtras, CompanionLaunchSource, CompanionParentSpec,
    CompanionParentWorkflowSpec, LaunchCommand, LifecycleNodeSpec, OwnerBootstrapSpec,
    OwnerPromptLifecycle, OwnerScope, SessionConstructionProvider,
    SessionConstructionProviderInput, SessionRequestAssembler, StoryStepPhase, StoryStepSpec,
    TerminalHookEffectBinding,
};
use agentdash_application::task::gateway::resolve_effective_task_workspace;
use agentdash_application::workflow::AgentFrameBuilder;
use agentdash_application::workflow::runtime_launch::RuntimeLaunchRequest;
use agentdash_domain::workflow::{AgentFrame, AgentProcedureRef, LifecycleAgent, LifecycleRun};
use agentdash_spi::ConnectorError;

use crate::app_state::AppState;
use crate::rpc::ApiError;

/// 使用 `Arc<AppState>` 的主通道 construction provider。在 AppState 初始化完成后注入
/// session runtime builder。
pub struct AppStateSessionConstructionProvider {
    state: Arc<AppState>,
}

impl AppStateSessionConstructionProvider {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

const CONSTRUCTION_API_ERROR_PREFIX: &str = "__construction_api_error__:";

#[cfg(test)]
fn encode_api_error(kind: &str, message: String) -> String {
    format!("{CONSTRUCTION_API_ERROR_PREFIX}{kind}:{message}")
}

pub(crate) fn decode_construction_runtime_error(message: &str) -> Option<ApiError> {
    let payload = message.strip_prefix(CONSTRUCTION_API_ERROR_PREFIX)?;
    let (kind, detail) = payload.split_once(':')?;
    match kind {
        "unauthorized" => Some(ApiError::Unauthorized(detail.to_string())),
        "forbidden" => Some(ApiError::Forbidden(detail.to_string())),
        "not_found" => Some(ApiError::NotFound(detail.to_string())),
        "conflict" => Some(ApiError::Conflict(detail.to_string())),
        "unprocessable_entity" => Some(ApiError::UnprocessableEntity(detail.to_string())),
        "service_unavailable" => Some(ApiError::ServiceUnavailable(detail.to_string())),
        "internal" => {
            tracing::error!(detail, "session construction internal error");
            Some(ApiError::Internal(String::from("内部 session 构建错误")))
        }
        _ => None,
    }
}

#[async_trait]
impl SessionConstructionProvider for AppStateSessionConstructionProvider {
    async fn build_frame_construction(
        &self,
        input: SessionConstructionProviderInput,
    ) -> Result<RuntimeLaunchRequest, ConnectorError> {
        let session_id = input.session_id.clone();
        let frame = self
            .state
            .repos
            .agent_frame_repo
            .find_by_runtime_session(&session_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "RuntimeSession {session_id} 没有关联 AgentFrame，拒绝 launch"
                ))
            })?;

        let direct_request = RuntimeLaunchRequest::from_frame(&frame);
        let direct_lifecycle =
            self.prompt_lifecycle(direct_request.executor_config.as_ref(), &input);
        if matches!(direct_lifecycle, SessionPromptLifecycle::Plain)
            && launch_request_ready(&direct_request)
        {
            return Ok(apply_command_and_extras(
                direct_request,
                None,
                &input.command,
                None,
            ));
        }

        let agent = self
            .state
            .repos
            .lifecycle_agent_repo
            .get(frame.agent_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "AgentFrame {} 指向的 LifecycleAgent {} 不存在",
                    frame.id, frame.agent_id
                ))
            })?;
        let run = self
            .state
            .repos
            .lifecycle_run_repo
            .get_by_id(agent.run_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "LifecycleAgent {} 指向的 LifecycleRun {} 不存在",
                    agent.id, agent.run_id
                ))
            })?;

        if let Some(companion) = input.command.companion_hint() {
            return self
                .compose_companion_frame(&frame, agent, companion, &input.command)
                .await;
        }

        if frame.graph_instance_id.is_some() && frame.activity_key.is_some() {
            return self
                .compose_lifecycle_node_frame(&frame, agent, run, &input.command)
                .await;
        }

        if input.command.task_hint().is_some()
            || self
                .has_task_association(run.id, agent.id)
                .await
                .map_err(connector_internal)?
        {
            return self.compose_task_frame(&frame, agent, run, &input).await;
        }

        if agent.project_agent_id.is_some() {
            return self
                .compose_project_agent_frame(&frame, agent, run, &input)
                .await;
        }

        if launch_request_ready(&direct_request) {
            return Ok(apply_command_and_extras(
                direct_request,
                None,
                &input.command,
                None,
            ));
        }

        Err(ConnectorError::InvalidConfig(format!(
            "AgentFrame {} 缺少 launch surface，且无法从 lifecycle anchor 推导 compose 路径",
            frame.id
        )))
    }
}

impl AppStateSessionConstructionProvider {
    fn assembler(&self) -> SessionRequestAssembler<'_> {
        SessionRequestAssembler::new(
            self.state.services.vfs_service.as_ref(),
            self.state.repos.canvas_repo.as_ref(),
            self.state.services.backend_registry.as_ref(),
            &self.state.repos,
            self.state.config.platform_config.as_ref(),
        )
        .with_audit_bus(self.state.services.audit_bus.clone())
        .with_companion_parent_facts_provider(&self.state.services.session_capability)
    }

    fn prompt_lifecycle(
        &self,
        executor_config: Option<&agentdash_spi::AgentConfig>,
        input: &SessionConstructionProviderInput,
    ) -> SessionPromptLifecycle {
        let supports_repository_restore = executor_config
            .map(|config| {
                self.state
                    .services
                    .connector
                    .supports_repository_restore(config.executor.as_str())
            })
            .unwrap_or(false);
        resolve_session_prompt_lifecycle(
            &input.session_meta,
            input.had_existing_runtime,
            supports_repository_restore,
        )
    }

    async fn has_task_association(
        &self,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
    ) -> Result<bool, agentdash_domain::DomainError> {
        let associations = self
            .state
            .repos
            .lifecycle_subject_association_repo
            .list_by_anchor(run_id, Some(agent_id))
            .await?;
        Ok(associations
            .iter()
            .any(|assoc| assoc.subject_kind == "task"))
    }

    async fn compose_project_agent_frame(
        &self,
        frame: &AgentFrame,
        mut agent: LifecycleAgent,
        run: LifecycleRun,
        input: &SessionConstructionProviderInput,
    ) -> Result<RuntimeLaunchRequest, ConnectorError> {
        let project_agent_id = agent.project_agent_id.ok_or_else(|| {
            ConnectorError::InvalidConfig(format!(
                "LifecycleAgent {} 缺少 project_agent_id",
                agent.id
            ))
        })?;
        let project = self
            .state
            .repos
            .project_repo
            .get_by_id(run.project_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("Project {} 不存在", run.project_id))
            })?;
        let project_agent = self
            .state
            .repos
            .project_agent_repo
            .get_by_project_and_id(project.id, project_agent_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("ProjectAgent {} 不存在", project_agent_id))
            })?;
        let agent_context =
            agentdash_application::session::construction_planner::RuntimeContextInspectionPlanner::build_project_agent_context(
                &self.state.repos,
                &project_agent,
            )
            .await
            .map_err(connector_internal)?;
        let workspace =
            agentdash_application::session::construction_planner::RuntimeContextInspectionPlanner::resolve_project_workspace(
                &self.state.repos,
                &project,
            )
            .await
            .map_err(connector_internal)?;
        let executor_config = merge_user_executor_config(
            input.command.user_input().executor_config.clone(),
            &agent_context.executor_config,
        );
        let lifecycle =
            owner_prompt_lifecycle(self.prompt_lifecycle(Some(&executor_config), input));
        let user_prompt_blocks = required_prompt_blocks(input.command.user_input())?;
        let builder = frame_builder_from_existing(frame, input.session_id.as_str())?;
        let (builder, extras) = self
            .assembler()
            .compose_owner_bootstrap_to_frame(
                builder,
                OwnerBootstrapSpec {
                    owner: OwnerScope::Project {
                        project: &project,
                        workspace: workspace.as_ref(),
                        agent_id: Some(project_agent.id),
                        agent_display_name: agent_context.display_name.clone(),
                        preset_name: agent_context.preset_name.clone(),
                    },
                    executor_config,
                    user_prompt_blocks,
                    agent_mcp: AgentLevelMcp {
                        preset_mcp_servers: agent_context.preset_mcp_servers.clone(),
                    },
                    agent_tool_directives: agent_context
                        .preset_config
                        .capability_directives
                        .clone()
                        .unwrap_or_default(),
                    agent_skill_asset_keys: agent_context
                        .preset_config
                        .skill_asset_keys
                        .clone()
                        .unwrap_or_default(),
                    agent_vfs_access_grants: agent_context
                        .preset_config
                        .vfs_access_grants
                        .clone()
                        .unwrap_or_default(),
                    request_mcp_servers: input.command.local_relay_mcp_declarations().to_vec(),
                    existing_vfs: RuntimeLaunchRequest::from_frame(frame).typed_vfs,
                    visible_canvas_mount_ids: input.session_meta.visible_canvas_mount_ids.clone(),
                    active_workflow: None,
                    lifecycle,
                    audit_session_key: Some(input.session_id.clone()),
                    caller_agent_id: Some(project_agent.id),
                },
            )
            .await
            .map_err(ConnectorError::InvalidConfig)?;

        self.persist_composed_frame(builder, &mut agent, extras, &input.command, None)
            .await
    }

    async fn compose_lifecycle_node_frame(
        &self,
        frame: &AgentFrame,
        mut agent: LifecycleAgent,
        run: LifecycleRun,
        command: &LaunchCommand,
    ) -> Result<RuntimeLaunchRequest, ConnectorError> {
        let lifecycle = self
            .state
            .repos
            .workflow_graph_repo
            .get_by_id(run.lifecycle_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("WorkflowGraph {} 不存在", run.lifecycle_id))
            })?;
        let activity_key = frame.activity_key.clone().ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("AgentFrame {} 缺少 activity_key", frame.id))
        })?;
        let activity = lifecycle
            .activities
            .iter()
            .find(|item| item.key == activity_key)
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "WorkflowGraph {} 中不存在 activity `{activity_key}`",
                    lifecycle.id
                ))
            })?;
        let workflow = match &activity.executor {
            agentdash_domain::workflow::ActivityExecutorSpec::Agent(spec) => self
                .state
                .repos
                .agent_procedure_repo
                .get_by_project_and_key(run.project_id, &spec.procedure_key)
                .await
                .map_err(connector_internal)?,
            _ => None,
        };
        let inherited_executor_config = command
            .user_input()
            .executor_config
            .clone()
            .or_else(|| RuntimeLaunchRequest::from_frame(frame).executor_config);
        let builder = frame_builder_from_existing(frame, command.reason_tag())?;
        let (builder, extras) =
            agentdash_application::session::compose_lifecycle_node_to_frame_with_audit(
                builder,
                &self.state.repos,
                self.state.config.platform_config.as_ref(),
                LifecycleNodeSpec {
                    run: &run,
                    lifecycle: &lifecycle,
                    activity,
                    workflow: workflow.as_ref(),
                    inherited_executor_config,
                },
                Some(self.state.services.audit_bus.clone()),
                frame.first_runtime_session_id().as_deref(),
            )
            .await
            .map_err(ConnectorError::InvalidConfig)?;

        self.persist_composed_frame(builder, &mut agent, extras, command, None)
            .await
    }

    async fn compose_task_frame(
        &self,
        frame: &AgentFrame,
        mut agent: LifecycleAgent,
        run: LifecycleRun,
        input: &SessionConstructionProviderInput,
    ) -> Result<RuntimeLaunchRequest, ConnectorError> {
        let mut associations = self
            .state
            .repos
            .lifecycle_subject_association_repo
            .list_by_anchor(run.id, Some(agent.id))
            .await
            .map_err(connector_internal)?;
        if associations.is_empty() {
            associations = self
                .state
                .repos
                .lifecycle_subject_association_repo
                .list_by_anchor(run.id, None)
                .await
                .map_err(connector_internal)?;
        }
        let task_id = associations
            .iter()
            .find(|assoc| assoc.subject_kind == "task")
            .map(|assoc| assoc.subject_id)
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!(
                    "LifecycleRun {} / Agent {} 缺少 task subject association",
                    run.id, agent.id
                ))
            })?;
        let story = self
            .state
            .repos
            .story_repo
            .find_by_task_id(task_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| ConnectorError::InvalidConfig(format!("Task {task_id} 不存在")))?;
        let task = story.find_task(task_id).cloned().ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("Story {} 中不存在 Task {task_id}", story.id))
        })?;
        let project = self
            .state
            .repos
            .project_repo
            .get_by_id(story.project_id)
            .await
            .map_err(connector_internal)?
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("Project {} 不存在", story.project_id))
            })?;
        let workspace =
            resolve_effective_task_workspace(&self.state.repos, &task, &story, &project)
                .await
                .map_err(connector_internal)?;
        let task_hint = input.command.task_hint();
        let phase = match task_hint.as_ref().and_then(|hint| hint.phase) {
            Some(agentdash_application::session::TaskLaunchPhase::Start) => StoryStepPhase::Start,
            _ => StoryStepPhase::Continue,
        };
        let explicit_executor_config = input
            .command
            .user_input()
            .executor_config
            .clone()
            .or_else(|| input.session_meta.executor_config.clone());
        let builder = frame_builder_from_existing(frame, input.session_id.as_str())?;
        let (builder, extras, hook_binding) = self
            .assembler()
            .compose_story_step_to_frame(
                builder,
                StoryStepSpec {
                    task: &task,
                    story: &story,
                    project: &project,
                    workspace: workspace.as_ref(),
                    phase,
                    override_prompt: task_hint
                        .as_ref()
                        .and_then(|hint| hint.override_prompt.as_deref()),
                    additional_prompt: task_hint
                        .as_ref()
                        .and_then(|hint| hint.additional_prompt.as_deref()),
                    request_mcp_servers: input.command.local_relay_mcp_declarations(),
                    explicit_executor_config,
                    strict_config_resolution: true,
                    active_workflow: None,
                    audit_session_key: Some(input.session_id.clone()),
                },
            )
            .await
            .map_err(|error| ConnectorError::InvalidConfig(error.to_string()))?;

        self.persist_composed_frame(builder, &mut agent, extras, &input.command, hook_binding)
            .await
    }

    async fn compose_companion_frame(
        &self,
        frame: &AgentFrame,
        mut agent: LifecycleAgent,
        companion: CompanionLaunchSource,
        command: &LaunchCommand,
    ) -> Result<RuntimeLaunchRequest, ConnectorError> {
        let builder = frame_builder_from_existing(frame, command.reason_tag())?;
        let (builder, extras) = if let Some(workflow) = companion.workflow {
            self.assembler()
                .compose_companion_with_workflow_to_frame(
                    builder,
                    CompanionParentWorkflowSpec {
                        companion: CompanionParentSpec {
                            parent_session_id: &companion.parent_session_id,
                            slice_mode: companion.slice_mode,
                            companion_executor_config: companion.companion_executor_config,
                            dispatch_prompt: companion.dispatch_prompt,
                        },
                        run: &workflow.run,
                        lifecycle: &workflow.lifecycle,
                        activity: &workflow.activity,
                        workflow: workflow.workflow.as_ref(),
                    },
                )
                .await
        } else {
            self.assembler()
                .compose_companion_to_frame(
                    builder,
                    CompanionParentSpec {
                        parent_session_id: &companion.parent_session_id,
                        slice_mode: companion.slice_mode,
                        companion_executor_config: companion.companion_executor_config,
                        dispatch_prompt: companion.dispatch_prompt,
                    },
                )
                .await
        }
        .map_err(ConnectorError::InvalidConfig)?;

        self.persist_composed_frame(builder, &mut agent, extras, command, None)
            .await
    }

    async fn persist_composed_frame(
        &self,
        builder: AgentFrameBuilder,
        agent: &mut LifecycleAgent,
        extras: AssemblyLaunchExtras,
        command: &LaunchCommand,
        hook_binding: Option<TerminalHookEffectBinding>,
    ) -> Result<RuntimeLaunchRequest, ConnectorError> {
        let frame = builder
            .build(self.state.repos.agent_frame_repo.as_ref())
            .await
            .map_err(connector_internal)?;
        agent.set_current_frame(frame.id);
        self.state
            .repos
            .lifecycle_agent_repo
            .update(agent)
            .await
            .map_err(connector_internal)?;
        let request = RuntimeLaunchRequest::from_frame(&frame);
        Ok(apply_command_and_extras(
            request,
            Some(extras),
            command,
            hook_binding,
        ))
    }
}

fn connector_internal(error: impl std::fmt::Display) -> ConnectorError {
    ConnectorError::Runtime(error.to_string())
}

fn launch_request_ready(request: &RuntimeLaunchRequest) -> bool {
    request.executor_config.is_some()
        && request.working_directory.is_some()
        && request.typed_capability_state.is_some()
}

fn owner_prompt_lifecycle(lifecycle: SessionPromptLifecycle) -> OwnerPromptLifecycle {
    match lifecycle {
        SessionPromptLifecycle::OwnerBootstrap => OwnerPromptLifecycle::OwnerBootstrap,
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::SystemContext,
        ) => OwnerPromptLifecycle::RepositoryRehydrate {
            prebuilt_continuation_bundle: None,
            include_owner_bundle: false,
        },
        SessionPromptLifecycle::RepositoryRehydrate(
            SessionRepositoryRehydrateMode::ExecutorState,
        ) => OwnerPromptLifecycle::RepositoryRehydrate {
            prebuilt_continuation_bundle: None,
            include_owner_bundle: true,
        },
        SessionPromptLifecycle::Plain => OwnerPromptLifecycle::Plain,
    }
}

fn merge_user_executor_config(
    user_config: Option<agentdash_spi::AgentConfig>,
    preset_config: &agentdash_spi::AgentConfig,
) -> agentdash_spi::AgentConfig {
    match user_config {
        Some(mut user_ec) => {
            if user_ec.system_prompt.is_none() {
                user_ec.system_prompt = preset_config.system_prompt.clone();
            }
            if user_ec.system_prompt_mode.is_none() {
                user_ec.system_prompt_mode = preset_config.system_prompt_mode;
            }
            user_ec
        }
        None => preset_config.clone(),
    }
}

fn required_prompt_blocks(
    input: &UserPromptInput,
) -> Result<Vec<serde_json::Value>, ConnectorError> {
    input
        .prompt_blocks
        .clone()
        .ok_or_else(|| ConnectorError::InvalidConfig("必须提供 promptBlocks".to_string()))
}

fn frame_builder_from_existing(
    frame: &AgentFrame,
    created_by_id: &str,
) -> Result<AgentFrameBuilder, ConnectorError> {
    let runtime_session_id = frame.first_runtime_session_id().ok_or_else(|| {
        ConnectorError::InvalidConfig(format!("AgentFrame {} 缺少 runtime_session ref", frame.id))
    })?;
    let mut builder = AgentFrameBuilder::new(frame.agent_id)
        .with_runtime_session(runtime_session_id)
        .with_created_by("session_launch", Some(created_by_id.to_string()));
    if let Some(procedure_id) = frame.procedure_id {
        builder = builder.with_procedure(AgentProcedureRef::ById(procedure_id));
    }
    if let (Some(graph_instance_id), Some(activity_key)) =
        (frame.graph_instance_id, frame.activity_key.clone())
    {
        builder = builder.with_graph_instance(graph_instance_id, activity_key);
    }
    if let Some(profile) = frame.execution_profile_json.clone() {
        builder = builder.with_execution_profile_raw(profile);
    }
    Ok(builder)
}

fn apply_command_and_extras(
    mut request: RuntimeLaunchRequest,
    extras: Option<AssemblyLaunchExtras>,
    command: &LaunchCommand,
    hook_binding: Option<TerminalHookEffectBinding>,
) -> RuntimeLaunchRequest {
    let mut prompt_blocks = command.user_input().prompt_blocks.clone();
    let mut environment_variables = command.user_input().env.clone();
    if let Some(config) = command.user_input().executor_config.clone() {
        request.executor_config = Some(config);
    }
    if let Some(extras) = extras {
        if extras.prompt_blocks.is_some() {
            prompt_blocks = extras.prompt_blocks;
        }
        if !extras.environment_variables.is_empty() {
            environment_variables = extras.environment_variables;
        }
        if let Some(config) = extras.executor_config {
            request.executor_config = Some(config);
        }
        if let Some(bundle) = extras.context_bundle {
            request.context_bundle = Some(bundle);
        }
        if let Some(capability_state) = extras.capability_state {
            request.typed_capability_state = Some(capability_state);
        }
        if let Some(vfs) = extras.vfs {
            request.working_directory = vfs
                .default_mount()
                .map(|mount| std::path::PathBuf::from(mount.root_ref.trim()))
                .filter(|path| !path.as_os_str().is_empty())
                .or(request.working_directory);
            request.typed_vfs = Some(vfs);
        }
        if !extras.mcp_servers.is_empty() {
            request.typed_mcp_servers = extras.mcp_servers;
        }
    }
    request.prompt_blocks = prompt_blocks;
    request.environment_variables = environment_variables;
    request.identity = command.identity();
    if let Some(binding) = hook_binding {
        request.terminal_hook_effect_binding = Some(binding);
    }
    request
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_construction_runtime_error_roundtrip_not_found() {
        let encoded = encode_api_error("not_found", "session missing".to_string());
        let decoded = decode_construction_runtime_error(&encoded);
        match decoded {
            Some(ApiError::NotFound(message)) => assert_eq!(message, "session missing"),
            other => panic!("期望 NotFound，实际为: {other:?}"),
        }
    }

    #[test]
    fn decode_construction_runtime_error_ignores_plain_runtime_text() {
        assert!(decode_construction_runtime_error("plain runtime error").is_none());
    }
}
