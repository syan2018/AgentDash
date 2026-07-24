# 当前数据库 Shape

首发基线包含 46 张业务表，另有 SQLx 自管的 `_sqlx_migrations`。

| 边界 | 表 |
| --- | --- |
| 身份与授权 | `users`, `auth_sessions`, `groups`, `group_memberships`, `project_subject_grants`, `lifecycle_subject_associations` |
| Project / Workspace / Backend | `projects`, `project_agents`, `backends`, `backend_workspace_inventory`, `backend_execution_leases`, `project_backend_access`, `workspaces`, `workspace_bindings`, `runtime_health`, `runner_registration_tokens` |
| Workflow / Lifecycle | `workflow_graphs`, `workflow_executor_effects`, `lifecycle_runs`, `lifecycle_agents`, `lifecycle_gates`, `stories`, `routines`, `routine_executions`, `state_changes`, `agent_procedures`, `agent_lineages`, `agent_run_lineages` |
| AgentRun Product | `agent_run_terminal_projection_head`, `agent_run_terminal_projection`, `agent_run_terminal_projection_change`, `dash_complete_source`, `dash_complete_effect` |
| Canvas / VFS | `canvases`, `canvas_files`, `agent_run_canvas_state`, `project_vfs_mounts`, `inline_fs_files` |
| Assets / Integration / Settings | `extension_package_artifacts`, `project_extension_installations`, `library_assets`, `skill_assets`, `mcp_presets`, `llm_providers`, `llm_provider_user_credentials`, `settings` |

## 结构判断

- `groups` / `group_memberships` 保留：它们表达独立权限主体及多对多成员关系，规范化结构合理。
- Gate delivery 收入 `lifecycle_gates.delivery`：状态只随单个 Gate 生灭和查询。
- Canvas observation / snapshot 合并到 `agent_run_canvas_state`：共享同一 run/agent/mount owner，但各自独立更新 JSONB 列。
- Terminal 使用 current projection + head + change log；控制关联作为 typed change delta，不维护一对一镜像和无消费者 outbox。
- `agent_run_lineages` 保留给 fork graph store；不再为同表维护第二套通用 Repository。

## 基线验证

- migration 文件：`0001_init.sql` 一份。
- 全新 schema：46 张业务表。
- retired 表：0 张。
- SQLx 元数据：version `1`, description `init`, checksum 等于当前 baseline SHA-384。
