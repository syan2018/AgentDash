# Work Item 05: Settings preference 事实源收束

## Goal

将旧 `user_preferences` 表和 BackendRepository preference port 中仍被消费的用户偏好迁入 scoped settings，避免 settings 事实源分裂。

## Source Issues

- `adversarial-review.md` Issue 20。
- `research/11-project-workspace-backend-placement.md` Issue 3。

## Evidence

- `crates/agentdash-domain/src/settings.rs:22` 起定义 scoped settings。
- `crates/agentdash-api/src/routes/settings.rs:47` / `:157` 读写 `settings_repo`。
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:341` / `:366` 从 scoped settings 读取 `agent.pi.user_preferences`。
- `crates/agentdash-domain/src/backend/repository.rs:25` / `:26` 仍有 `get_preferences/save_preferences`。
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:295` / `:320` 仍读写 `user_preferences`。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:178` / `:189` 仍读旧 `hide_system_steer_messages`。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1578` / `:1590` 仍读旧偏好。

## Requirements

- `hide_system_steer_messages` 迁入 scoped settings。
- AgentRun workspace/query 与 lifecycle agent route 不再读取 `BackendRepository::get_preferences`。
- BackendRepository 移除 user preference 职责。
- DB migration 迁移旧 `user_preferences.key='prefs'` 中仍有价值的字段。
- 迁移后删除旧表或旧 port；不保留兼容 fallback。

## Suggested Implementation Shape

- 定义 scoped setting key，例如 `agent.mailbox.hide_system_steer_messages`。
- 在 settings repository 中读取该 key，按缺省值处理未设置状态。
- 修改 AgentRun workspace/query 和 lifecycle route mapper 的偏好读取路径。
- 写 migration：
  - 从旧 `user_preferences` JSON 中提取 `hide_system_steer_messages`。
  - 写入 scoped `settings`。
  - 删除旧表或至少删除业务引用后由 migration 清理。
- 移除 domain/backend `UserPreferences` 与 BackendRepository preference methods。

## Tests / Verification

- migration guard。
- repository migration test 或 SQL-focused verification。
- AgentRun workspace query test 覆盖 setting true/false/default。
- frontend typecheck，如 settings key/API shape 影响前端。

## Out of Scope

- 不重做整个 settings UI。
- 不处理其它尚未发现的旧 preference 字段，除非迁移中确认仍被业务消费。
