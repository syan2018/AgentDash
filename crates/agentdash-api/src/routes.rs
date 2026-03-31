pub mod acp_sessions;
pub mod address_spaces;
pub mod agents;
pub mod auth_routes;
pub mod backends;
pub mod discovered_options;
pub mod discovery;
pub mod file_picker;
pub mod health;
pub mod identity_directory;
pub mod me;
pub mod project_agents;
pub mod project_sessions;
pub mod projects;
pub mod settings;
pub mod stories;
pub mod story_sessions;
pub mod task_execution;
pub mod workflows;
pub mod workspaces;

use std::sync::Arc;

use agentdash_mcp::{services::McpServices, transport::McpRouterBuilder};
use axum::{
    Router, middleware,
    routing::{delete, get, patch, post, put},
};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::relay;

use crate::app_state::AppState;
use crate::stream;

pub fn create_router(state: Arc<AppState>) -> Router {
    let mcp_services = Arc::new(McpServices {
        project_repo: state.repos.project_repo.clone(),
        story_repo: state.repos.story_repo.clone(),
        task_repo: state.repos.task_repo.clone(),
        workspace_repo: state.repos.workspace_repo.clone(),
    });
    let mcp = McpRouterBuilder::new(mcp_services).build();

    let secured_api = Router::new()
        .route("/me", get(me::get_current_user))
        .route(
            "/directory/users",
            get(identity_directory::list_directory_users),
        )
        .route(
            "/directory/groups",
            get(identity_directory::list_directory_groups),
        )
        // Project CRUD
        .route(
            "/projects",
            get(projects::list_projects).post(projects::create_project),
        )
        .route(
            "/projects/{id}",
            get(projects::get_project)
                .put(projects::update_project)
                .delete(projects::delete_project),
        )
        .route("/projects/{id}/clone", post(projects::clone_project))
        .route("/projects/{id}/grants", get(projects::list_project_grants))
        .route(
            "/projects/{id}/grants/users/{user_id}",
            put(projects::grant_project_user).delete(projects::revoke_project_user),
        )
        .route(
            "/projects/{id}/grants/groups/{group_id}",
            put(projects::grant_project_group).delete(projects::revoke_project_group),
        )
        // Agent CRUD（顶层实体）
        .route(
            "/agents",
            get(agents::list_agents).post(agents::create_agent),
        )
        .route(
            "/agents/{id}",
            get(agents::get_agent)
                .put(agents::update_agent)
                .delete(agents::delete_agent),
        )
        // Project-Agent 关联（新模型）
        .route(
            "/projects/{id}/agent-links",
            get(project_agents::list_project_agent_links)
                .post(project_agents::create_project_agent_link),
        )
        .route(
            "/projects/{id}/agent-links/{agent_id}",
            put(project_agents::update_project_agent_link)
                .delete(project_agents::delete_project_agent_link),
        )
        // 兼容旧路径（project agents / sessions）
        .route(
            "/projects/{id}/agents",
            get(project_agents::list_project_agents),
        )
        .route(
            "/projects/{id}/agents/{agent_key}/session",
            post(project_agents::open_project_agent_session),
        )
        .route(
            "/projects/{id}/agents/{agent_key}/sessions",
            get(project_agents::list_project_agent_sessions),
        )
        .route(
            "/projects/{id}/sessions",
            get(project_sessions::list_project_sessions),
        )
        .route(
            "/projects/{id}/sessions/{binding_id}",
            get(project_sessions::get_project_session),
        )
        .route(
            "/projects/{id}/workflow-assignments",
            get(workflows::list_project_workflow_assignments)
                .post(workflows::create_project_workflow_assignment),
        )
        // Workspace（嵌套在 Project 下创建/列表，独立路由操作）
        .route(
            "/projects/{project_id}/workspaces",
            get(workspaces::list_workspaces).post(workspaces::create_workspace),
        )
        .route(
            "/projects/{project_id}/workspaces/detect",
            post(workspaces::detect_workspace),
        )
        .route("/workspaces/detect-git", post(workspaces::detect_git))
        .route(
            "/workspaces/{id}",
            get(workspaces::get_workspace)
                .put(workspaces::update_workspace)
                .delete(workspaces::delete_workspace),
        )
        .route(
            "/workspaces/{id}/status",
            patch(workspaces::update_workspace_status),
        )
        // Story（支持 project_id 或 backend_id 查询）
        .route(
            "/stories",
            get(stories::list_stories).post(stories::create_story),
        )
        .route(
            "/stories/{id}",
            get(stories::get_story)
                .put(stories::update_story)
                .delete(stories::delete_story),
        )
        .route(
            "/stories/{id}/sessions",
            get(story_sessions::list_story_sessions).post(story_sessions::create_story_session),
        )
        .route(
            "/stories/{id}/sessions/{binding_id}",
            get(story_sessions::get_story_session).delete(story_sessions::unbind_story_session),
        )
        .route(
            "/stories/{id}/tasks",
            get(stories::list_tasks).post(stories::create_task),
        )
        .route(
            "/tasks/{id}",
            get(stories::get_task)
                .put(stories::update_task)
                .delete(stories::delete_task),
        )
        .route("/tasks/{id}/start", post(task_execution::start_task))
        .route("/tasks/{id}/continue", post(task_execution::continue_task))
        .route("/tasks/{id}/cancel", post(task_execution::cancel_task))
        .route("/tasks/{id}/session", get(task_execution::get_task_session))
        // Workflow contract / lifecycle
        .route(
            "/workflow-definitions",
            get(workflows::list_workflows).post(workflows::create_workflow_definition),
        )
        .route(
            "/lifecycle-definitions",
            get(workflows::list_lifecycles).post(workflows::create_lifecycle_definition),
        )
        .route(
            "/workflow-definitions/validate",
            post(workflows::validate_workflow_definition),
        )
        .route(
            "/lifecycle-definitions/validate",
            post(workflows::validate_lifecycle_definition),
        )
        .route(
            "/workflow-definitions/{id}",
            get(workflows::get_workflow_definition)
                .put(workflows::update_workflow_definition)
                .delete(workflows::delete_workflow_definition),
        )
        .route(
            "/workflow-definitions/{id}/enable",
            post(workflows::enable_workflow_definition),
        )
        .route(
            "/workflow-definitions/{id}/disable",
            post(workflows::disable_workflow_definition),
        )
        .route(
            "/lifecycle-definitions/{id}",
            get(workflows::get_lifecycle_definition)
                .put(workflows::update_lifecycle_definition)
                .delete(workflows::delete_lifecycle_definition),
        )
        .route(
            "/lifecycle-definitions/{id}/enable",
            post(workflows::enable_lifecycle_definition),
        )
        .route(
            "/lifecycle-definitions/{id}/disable",
            post(workflows::disable_lifecycle_definition),
        )
        .route("/hook-presets", get(workflows::list_hook_presets))
        .route(
            "/hook-scripts/validate",
            post(workflows::validate_hook_script),
        )
        .route(
            "/hook-presets/custom",
            post(workflows::register_hook_preset),
        )
        .route(
            "/hook-presets/custom/{key}",
            delete(workflows::delete_hook_preset),
        )
        .route(
            "/workflow-templates",
            get(workflows::list_workflow_templates),
        )
        .route(
            "/workflow-templates/{builtin_key}/bootstrap",
            post(workflows::bootstrap_workflow_template),
        )
        .route("/lifecycle-runs", post(workflows::start_lifecycle_run))
        .route("/lifecycle-runs/{id}", get(workflows::get_lifecycle_run))
        .route(
            "/lifecycle-runs/bindings/{binding_kind}/{binding_id}",
            get(workflows::list_lifecycle_runs_by_binding),
        )
        .route(
            "/lifecycle-runs/{id}/steps/{step_key}/activate",
            post(workflows::activate_workflow_step),
        )
        .route(
            "/lifecycle-runs/{id}/steps/{step_key}/complete",
            post(workflows::complete_workflow_step),
        )
        .route(
            "/lifecycle-runs/{id}/steps/{step_key}/artifacts",
            post(workflows::append_workflow_step_artifacts),
        )
        // Backend
        .route(
            "/backends",
            get(backends::list_backends).post(backends::add_backend),
        )
        .route(
            "/backends/{id}",
            get(backends::get_backend).delete(backends::remove_backend),
        )
        .route("/backends/online", get(backends::list_online_backends))
        .route(
            "/backends/{backend_id}/browse",
            post(backends::browse_directory),
        )
        // Settings
        .route(
            "/settings",
            get(settings::list_settings).put(settings::update_settings),
        )
        .route("/settings/{key}", delete(settings::delete_setting))
        // ACP Sessions — CRUD
        .route(
            "/sessions",
            get(acp_sessions::list_sessions).post(acp_sessions::create_session),
        )
        .route(
            "/sessions/{id}",
            get(acp_sessions::get_session).delete(acp_sessions::delete_session),
        )
        .route(
            "/sessions/{id}/hook-runtime",
            get(acp_sessions::get_session_hook_runtime),
        )
        .route("/sessions/{id}/state", get(acp_sessions::get_session_state))
        .route(
            "/sessions/{id}/bindings",
            get(acp_sessions::get_session_bindings),
        )
        .route(
            "/sessions/{id}/context",
            get(acp_sessions::get_session_context),
        )
        // ACP Sessions — Execution
        .route("/sessions/{id}/prompt", post(acp_sessions::prompt_session))
        .route("/sessions/{id}/cancel", post(acp_sessions::cancel_session))
        .route(
            "/sessions/{id}/tool-approvals/{tool_call_id}/approve",
            post(acp_sessions::approve_tool_call),
        )
        .route(
            "/sessions/{id}/tool-approvals/{tool_call_id}/reject",
            post(acp_sessions::reject_tool_call),
        )
        .route(
            "/acp/sessions/{id}/stream",
            get(acp_sessions::acp_session_stream_sse),
        )
        .route(
            "/acp/sessions/{id}/stream/ndjson",
            get(acp_sessions::acp_session_stream_ndjson),
        )
        // Events
        .route("/events/stream", get(stream::event_stream))
        .route("/events/stream/ndjson", get(stream::event_stream_ndjson))
        .route("/events/since/{since_id}", get(stream::get_events_since))
        // Mount Provider 发现（返回可由用户配置的外部服务 provider 列表）
        .route(
            "/mount-providers",
            get(address_spaces::list_configurable_mount_providers),
        )
        // Address Spaces（统一寻址空间能力发现与条目检索）
        .route("/address-spaces", get(address_spaces::list_address_spaces))
        .route(
            "/address-spaces/{space_id}/entries",
            get(address_spaces::list_address_entries),
        )
        .route(
            "/address-spaces/mounts/{mount_id}/entries",
            get(address_spaces::list_mount_entries),
        )
        .route(
            "/address-spaces/read-file",
            post(address_spaces::read_mount_file),
        )
        .route(
            "/address-spaces/write-file",
            post(address_spaces::write_mount_file),
        )
        .route(
            "/address-spaces/apply-patch",
            post(address_spaces::apply_mount_patch),
        )
        .route(
            "/address-spaces/preview",
            post(address_spaces::preview_address_space),
        )
        // File Picker（@ 文件引用选择器 API，走 Address Space 统一访问层）
        .route("/file-picker", get(file_picker::list_files))
        .route("/file-picker/read", post(file_picker::read_file))
        .route(
            "/file-picker/batch-read",
            post(file_picker::batch_read_files),
        )
        // Agent Discovery
        .route("/agents/discovery", get(discovery::get_discovery))
        .route(
            "/agents/discovered-options/stream",
            get(discovered_options::discovered_options_stream),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::authenticate_request,
        ));

    let api = Router::new()
        .route("/health", get(health::health_check))
        .route("/auth/login", post(auth_routes::login))
        .route("/auth/metadata", get(auth_routes::metadata))
        .merge(secured_api)
        .with_state(state.clone());

    Router::new()
        .merge(mcp)
        .nest("/api", api)
        .route(
            "/ws/backend",
            get(relay::ws_handler::ws_backend_handler).with_state(state),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
