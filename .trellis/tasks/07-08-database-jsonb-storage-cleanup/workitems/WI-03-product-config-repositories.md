# WI-03 Product And Config Repositories

Status: done

Scope:

- Convert project, agent, backend view, story, workspace, canvas, settings, LLM, MCP, routine, VFS, and backend access repositories from JSON string roundtrip to JSONB typed mapping.

Files:

- `agent_repository.rs`
- `project_repository.rs`
- `backend_repository.rs`
- `story_repository.rs`
- `workspace_repository.rs`
- `canvas_repository.rs`
- `canvas_runtime_state_repository.rs`
- `settings_repository.rs`
- `llm_provider_repository.rs`
- `mcp_preset_repository.rs`
- `routine_repository.rs`
- `project_vfs_mount_repository.rs`
- `project_backend_access_repository.rs`

Validation:

- `cargo check -p agentdash-infrastructure` passed.
