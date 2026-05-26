pub mod acp_sessions;
pub mod auth_routes;
pub mod backend_access;
pub mod backends;
pub mod canvases;
pub mod discovered_options;
pub mod discovery;
pub mod extension_package_artifacts;
pub mod extension_runtime;
pub mod file_picker;
pub mod health;
pub mod identity_directory;
pub mod llm_providers;
pub mod mcp_presets;
pub mod me;
pub mod project_agents;
pub mod project_sessions;
pub mod project_vfs_mounts;
pub mod projects;
pub mod routines;
pub mod settings;
pub mod shared_library;
pub mod skill_assets;
pub mod stories;
pub mod story_sessions;
pub mod task_execution;
pub mod terminals;
pub mod vfs;
pub mod vfs_surfaces;
pub mod workflows;
pub mod workspaces;

use std::sync::Arc;

use agentdash_mcp::{services::McpServices, transport::McpRouterBuilder};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{delete, get, patch, post, put},
};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::relay;

use crate::app_state::AppState;
use crate::stream;

const SKILL_ASSET_UPLOAD_BODY_LIMIT_BYTES: usize = 80 * 1024 * 1024;
const VFS_BINARY_UPLOAD_BODY_LIMIT_BYTES: usize = 80 * 1024 * 1024;
const EXTENSION_PACKAGE_UPLOAD_BODY_LIMIT_BYTES: usize = 80 * 1024 * 1024;

pub fn create_router(state: Arc<AppState>) -> Router {
    let mcp_services = Arc::new(McpServices {
        project_repo: state.repos.project_repo.clone(),
        story_repo: state.repos.story_repo.clone(),
        workspace_repo: state.repos.workspace_repo.clone(),
        workflow_definition_repo: state.repos.workflow_definition_repo.clone(),
        activity_lifecycle_definition_repo: state.repos.activity_lifecycle_definition_repo.clone(),
        state_change_repo: state.repos.state_change_repo.clone(),
    });
    let mcp = McpRouterBuilder::new(mcp_services)
        .build()
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::authenticate_request,
        ));

    let secured_api = Router::new()
        .route("/me", get(me::get_current_user))
        .route("/auth/logout", post(auth_routes::logout))
        .route("/auth/revoke", post(auth_routes::revoke_token))
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
            "/projects/{project_id}/vfs-mounts",
            get(project_vfs_mounts::list_vfs_mounts).post(project_vfs_mounts::create_vfs_mount),
        )
        .route(
            "/projects/{project_id}/vfs-mounts/{mount_id}",
            get(project_vfs_mounts::get_vfs_mount)
                .put(project_vfs_mounts::update_vfs_mount)
                .delete(project_vfs_mounts::delete_vfs_mount),
        )
        .route(
            "/projects/{id}/grants/users/{user_id}",
            put(projects::grant_project_user).delete(projects::revoke_project_user),
        )
        .route(
            "/projects/{id}/grants/groups/{group_id}",
            put(projects::grant_project_group).delete(projects::revoke_project_group),
        )
        // LLM Provider CRUD
        .route(
            "/llm-providers",
            get(llm_providers::list_providers).post(llm_providers::create_provider),
        )
        .route(
            "/llm-providers/reorder",
            post(llm_providers::reorder_providers),
        )
        .route(
            "/llm-providers/probe-models",
            post(llm_providers::probe_models),
        )
        .route(
            "/llm-providers/codex-oauth/{flow_id}",
            get(llm_providers::get_codex_oauth_status),
        )
        .route(
            "/llm-providers/codex-oauth/{flow_id}/cancel",
            post(llm_providers::cancel_codex_oauth),
        )
        .route(
            "/llm-providers/{id}",
            get(llm_providers::get_provider)
                .put(llm_providers::update_provider)
                .delete(llm_providers::delete_provider),
        )
        .route(
            "/llm-providers/{id}/codex-oauth/start",
            post(llm_providers::start_codex_oauth),
        )
        // Project Agent 项目实例
        .route(
            "/projects/{id}/agents",
            get(project_agents::list_project_agent_configs)
                .post(project_agents::create_project_agent),
        )
        .route(
            "/projects/{id}/agents/summary",
            get(project_agents::list_project_agents),
        )
        .route(
            "/projects/{id}/agents/{project_agent_id}",
            put(project_agents::update_project_agent).delete(project_agents::delete_project_agent),
        )
        .route(
            "/projects/{id}/agents/{project_agent_id}/session",
            post(project_agents::open_project_agent_session),
        )
        .route(
            "/projects/{id}/agents/{project_agent_id}/sessions",
            get(project_agents::list_project_agent_sessions),
        )
        // Routine CRUD（嵌套在 Project 下）
        .route(
            "/projects/{id}/routines",
            get(routines::list_routines).post(routines::create_routine),
        )
        .route(
            "/routines/{id}",
            get(routines::get_routine)
                .put(routines::update_routine)
                .delete(routines::delete_routine),
        )
        .route("/routines/{id}/enable", patch(routines::enable_routine))
        .route(
            "/routines/{id}/regenerate-token",
            post(routines::regenerate_webhook_token),
        )
        .route("/routines/{id}/executions", get(routines::list_executions))
        .route(
            "/projects/{id}/sessions",
            get(project_sessions::list_project_sessions),
        )
        .route(
            "/projects/{id}/sessions/{binding_id}",
            get(project_sessions::get_project_session),
        )
        .route(
            "/projects/{project_id}/canvases",
            get(canvases::list_project_canvases).post(canvases::create_canvas),
        )
        // MCP Preset（Project 级 MCP Server 配置模板，Assets 页子类目）
        .route(
            "/projects/{project_id}/mcp-presets",
            get(mcp_presets::list_mcp_presets).post(mcp_presets::create_mcp_preset),
        )
        .route(
            "/projects/{project_id}/mcp-presets/probe",
            post(mcp_presets::probe_mcp_transport_handler),
        )
        .route(
            "/projects/{project_id}/mcp-presets/{id}",
            get(mcp_presets::get_mcp_preset)
                .patch(mcp_presets::update_mcp_preset)
                .delete(mcp_presets::delete_mcp_preset),
        )
        .route(
            "/projects/{project_id}/mcp-presets/{id}/clone",
            post(mcp_presets::clone_mcp_preset),
        )
        // Skill Asset（Project 级云端 Skill 仓储，Assets 页子类目）
        .route(
            "/projects/{project_id}/skill-assets",
            get(skill_assets::list_skill_assets).post(skill_assets::create_skill_asset),
        )
        .route(
            "/projects/{project_id}/skill-assets/upload",
            post(skill_assets::upload_skill_assets)
                .layer(DefaultBodyLimit::max(SKILL_ASSET_UPLOAD_BODY_LIMIT_BYTES)),
        )
        .route(
            "/projects/{project_id}/skill-assets/import",
            post(skill_assets::import_remote_skill_asset),
        )
        .route(
            "/projects/{project_id}/skill-assets/{id}",
            get(skill_assets::get_skill_asset)
                .patch(skill_assets::update_skill_asset)
                .delete(skill_assets::delete_skill_asset),
        )
        .route(
            "/projects/{project_id}/skill-assets/{id}/files/blob",
            get(skill_assets::read_skill_asset_file_blob),
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
        .route(
            "/projects/{project_id}/workspaces/candidates",
            get(backend_access::list_workspace_candidates),
        )
        .route(
            "/projects/{project_id}/workspaces/sync-backend-bindings",
            post(backend_access::sync_workspace_bindings),
        )
        .route(
            "/projects/{project_id}/backend-access",
            get(backend_access::list_project_backend_access)
                .post(backend_access::create_project_backend_access),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}",
            patch(backend_access::update_project_backend_access)
                .delete(backend_access::revoke_project_backend_access),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}/inventory",
            get(backend_access::list_project_backend_inventory),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}/inventory/refresh",
            post(backend_access::refresh_project_backend_inventory),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}/inventory/register",
            post(backend_access::register_project_backend_inventory),
        )
        .route(
            "/projects/{project_id}/backend-access/{access_id}/browse",
            post(backend_access::browse_project_backend_access),
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
            "/canvases/{id}",
            get(canvases::get_canvas)
                .put(canvases::update_canvas)
                .delete(canvases::delete_canvas),
        )
        .route(
            "/canvases/{id}/runtime-snapshot",
            get(canvases::get_canvas_runtime_snapshot),
        )
        .route(
            "/canvases/{id}/runtime-invoke",
            post(canvases::invoke_canvas_runtime_action),
        )
        .route(
            "/canvases/{id}/promote-extension",
            post(canvases::promote_canvas_to_extension),
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
            "/activity-lifecycle-definitions",
            get(workflows::list_activity_lifecycles)
                .post(workflows::create_activity_lifecycle_definition),
        )
        .route(
            "/workflow-definitions/validate",
            post(workflows::validate_workflow_definition),
        )
        .route(
            "/activity-lifecycle-definitions/validate",
            post(workflows::validate_activity_lifecycle_definition),
        )
        .route(
            "/workflow-definitions/{id}",
            get(workflows::get_workflow_definition)
                .put(workflows::update_workflow_definition)
                .delete(workflows::delete_workflow_definition),
        )
        .route(
            "/activity-lifecycle-definitions/{id}",
            get(workflows::get_activity_lifecycle_definition)
                .put(workflows::update_activity_lifecycle_definition)
                .delete(workflows::delete_activity_lifecycle_definition),
        )
        .route("/tool-catalog", get(workflows::query_tool_catalog))
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
        .route("/lifecycle-runs", post(workflows::start_lifecycle_run))
        .route("/lifecycle-runs/{id}", get(workflows::get_lifecycle_run))
        .route(
            "/lifecycle-runs/by-session/{session_id}",
            get(workflows::list_lifecycle_runs_by_session),
        )
        .route(
            "/lifecycle-runs/{id}/activities/{activity_key}/attempts/{attempt}/human-decision",
            post(workflows::submit_human_decision),
        )
        // Backend
        .route(
            "/backends",
            get(backends::list_backends).post(backends::add_backend),
        )
        .route(
            "/local-runtime/ensure",
            post(backends::ensure_local_runtime),
        )
        .route(
            "/backends/runtime-health",
            get(backends::list_runtime_health),
        )
        .route(
            "/backends/runtime-summary",
            get(backends::list_runtime_summary),
        )
        .route(
            "/backends/{id}",
            get(backends::get_backend).delete(backends::remove_backend),
        )
        .route(
            "/backends/{id}/runtime-health",
            get(backends::get_runtime_health),
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
        // Shared Library（公共资源库，Marketplace 的后端资产入口）
        .route(
            "/shared-library/assets",
            get(shared_library::list_library_assets),
        )
        .route(
            "/shared-library/assets/seed-builtin",
            post(shared_library::seed_builtin_library_assets),
        )
        .route(
            "/shared-library/assets/{id}",
            get(shared_library::get_library_asset),
        )
        .route(
            "/projects/{project_id}/shared-library/install",
            post(shared_library::install_library_asset),
        )
        .route(
            "/projects/{project_id}/shared-library/publish",
            post(shared_library::publish_library_asset),
        )
        .route(
            "/projects/{project_id}/shared-library/source-status",
            get(shared_library::get_project_asset_source_status),
        )
        .route(
            "/projects/{project_id}/extension-runtime",
            get(extension_runtime::get_project_extension_runtime),
        )
        .route(
            "/projects/{project_id}/extension-runtime/invoke-action",
            post(extension_runtime::invoke_project_extension_runtime_action),
        )
        .route(
            "/projects/{project_id}/extension-runtime/webviews/{extension_key}/{*asset_path}",
            get(extension_runtime::get_project_extension_webview_asset),
        )
        .route(
            "/projects/{project_id}/extension-artifacts",
            get(extension_package_artifacts::list_extension_package_artifacts)
                .post(extension_package_artifacts::upload_extension_package_artifact)
                .layer(DefaultBodyLimit::max(
                    EXTENSION_PACKAGE_UPLOAD_BODY_LIMIT_BYTES,
                )),
        )
        .route(
            "/projects/{project_id}/extension-artifacts/{artifact_id}/install",
            post(extension_package_artifacts::install_extension_package_artifact_route),
        )
        .route(
            "/projects/{project_id}/extension-artifacts/{artifact_id}/archive",
            get(extension_package_artifacts::download_extension_package_archive),
        )
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
            "/sessions/{id}/meta",
            get(acp_sessions::get_session_meta).patch(acp_sessions::update_session_meta),
        )
        .route(
            "/sessions/{id}/hook-runtime",
            get(acp_sessions::get_session_hook_runtime),
        )
        .route("/sessions/{id}/state", get(acp_sessions::get_session_state))
        .route(
            "/sessions/{id}/events",
            get(acp_sessions::list_session_events),
        )
        .route(
            "/sessions/{id}/bindings",
            get(acp_sessions::get_session_bindings),
        )
        .route(
            "/sessions/{id}/context",
            get(acp_sessions::get_session_context),
        )
        .route(
            "/sessions/{id}/context/projection",
            get(acp_sessions::get_session_context_projection),
        )
        .route(
            "/sessions/{id}/lineage",
            get(acp_sessions::get_session_lineage),
        )
        .route("/sessions/{id}/fork", post(acp_sessions::fork_session))
        .route(
            "/sessions/{id}/projection/rollback",
            post(acp_sessions::rollback_session_projection),
        )
        .route(
            "/sessions/{id}/context/audit",
            get(acp_sessions::get_session_context_audit),
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
            "/sessions/{id}/companion-requests/{request_id}/respond",
            post(acp_sessions::respond_companion_request),
        )
        .route(
            "/acp/sessions/{id}/stream/ndjson",
            get(acp_sessions::acp_session_stream_ndjson),
        )
        // Events
        .route("/events/stream/ndjson", get(stream::event_stream_ndjson))
        // Mount Provider 发现（返回可由用户配置的外部服务 provider 列表）
        .route(
            "/mount-providers",
            get(vfs::list_configurable_mount_providers),
        )
        .route("/vfs-surfaces/resolve", post(vfs_surfaces::resolve_surface))
        .route(
            "/vfs-surfaces/{surface_ref}",
            get(vfs_surfaces::get_surface),
        )
        .route(
            "/vfs-surfaces/{surface_ref}/mounts/{mount_id}/entries",
            get(vfs_surfaces::list_surface_mount_entries),
        )
        .route(
            "/vfs-surfaces/read-file",
            post(vfs_surfaces::read_surface_file),
        )
        .route(
            "/vfs-surfaces/read-file-blob",
            post(vfs_surfaces::read_surface_file_blob),
        )
        .route(
            "/vfs-surfaces/upload-file-blob",
            post(vfs_surfaces::upload_surface_file_blob)
                .layer(DefaultBodyLimit::max(VFS_BINARY_UPLOAD_BODY_LIMIT_BYTES)),
        )
        .route(
            "/vfs-surfaces/write-file",
            post(vfs_surfaces::write_surface_file),
        )
        .route(
            "/vfs-surfaces/create-file",
            post(vfs_surfaces::create_surface_file),
        )
        .route(
            "/vfs-surfaces/delete-file",
            post(vfs_surfaces::delete_surface_file),
        )
        .route(
            "/vfs-surfaces/rename-file",
            post(vfs_surfaces::rename_surface_file),
        )
        .route(
            "/vfs-surfaces/stat-file",
            post(vfs_surfaces::stat_surface_file),
        )
        .route(
            "/vfs-surfaces/apply-patch",
            post(vfs_surfaces::apply_surface_patch),
        )
        // Terminals（交互式终端）
        .route(
            "/sessions/{id}/terminals",
            get(terminals::list_terminals).post(terminals::spawn_terminal),
        )
        .route("/terminals/{id}/input", post(terminals::terminal_input))
        .route("/terminals/{id}/resize", post(terminals::terminal_resize))
        .route("/terminals/{id}", delete(terminals::terminal_kill))
        // VFSs（统一寻址空间能力发现与条目检索）
        .route("/vfs", get(vfs::list_vfs))
        .route("/vfs/{space_id}/entries", get(vfs::list_address_entries))
        // File Picker（@ 文件引用选择器 API，走 VFS 统一访问层）
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
        .route("/auth/oidc/start", post(auth_routes::start_oidc_login))
        .route("/auth/oidc/callback", get(auth_routes::oidc_callback))
        .route("/auth/metadata", get(auth_routes::metadata))
        // Routine Webhook 触发端点（不走 session auth，中间件外单独 Bearer 校验）
        .route(
            "/routine-triggers/{endpoint_id}/fire",
            post(routines::fire_webhook),
        )
        .route(
            "/local-runtime/projects/{project_id}/extension-artifacts/{artifact_id}/archive",
            get(extension_package_artifacts::download_extension_package_archive_for_backend),
        )
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
