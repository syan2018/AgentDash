# 进度记录

## 2026-04-03

### 已完成

- `SessionPromptLifecycle` 收敛为 `OwnerBootstrap / RepositoryRehydrate / Plain` 三态
- API route 不再各自拼装“首轮还是续跑”的临时判定，统一复用 session 模型生命周期解析
- `SessionBootstrapState` 接入内存持久化、SQLite、PostgreSQL session repository
- `SessionHub` 新增基于 `session_events` 的恢复消息重建能力
- 恢复消息时过滤 owner resource block，避免 `project/story/task` bootstrap 二次回灌
- `AgentConnector` 补充：
  - `supports_repository_restore(executor)`
  - `has_live_session(session_id)`
- `CompositeConnector` 与 `PiAgentConnector` 已消费仓储恢复能力
- `ExecutionContext` 新增 `restored_session_state`
- PostgreSQL migration 补 `sessions.bootstrap_state`
- PostgreSQL story/task 仓储修复 `task_count INTEGER` 与 `i64/INT8` 不匹配问题
- 手工前端验证通过：
  - 首次 prompt
  - reopen 后再次 prompt
  - restart 后 reopen 再次 prompt
  - 三种情况下 owner context 不重复注入

### 手工验证结论

- session 仓储流里的 `agentdash://story-context/<story_id>` 资源注入计数保持 1 次
- 页面首次发送时只出现 1 张 `Story 上下文` 卡片
- reopen / restart 后继续发送不会再出现第二份 bootstrap

### 发现的附带问题

- 前端 discovery 能看到远程 backend 上报的 `CODEX / CLAUDE_CODE / GEMINI ...`
- 但云端 `CompositeConnector` 当前并未直接路由这些远程 executors
- 因此前端手工选择 `CODEX` 后，会报：
  - `未知执行器 'CODEX'，无法路由到任何连接器`

### 验证记录

- `cargo test -p agentdash-application session::hub::tests -- --nocapture` 通过
- `cargo test -p agentdash-api session_prompt_lifecycle -- --nocapture` 通过
- `cargo test -p agentdash-executor prompt_restores_repository_messages_before_new_user_prompt -- --nocapture` 通过
- `cargo check -p agentdash-application -p agentdash-api -p agentdash-executor -p agentdash-infrastructure -p agentdash-local` 通过
- `cargo check -p agentdash-infrastructure -p agentdash-api -p agentdash-local` 通过
- `cargo build --bin agentdash-server --bin agentdash-local` 通过
- `pnpm run frontend:check` 通过
- `pnpm run frontend:test` 通过
- `pnpm run frontend:lint` 未通过，但失败项来自仓库内既有前端规则问题，不是本次 session 重构新增错误

### 下一步建议

- 单独收敛“discovery 可见 executors”与“session prompt 实际可路由 executors”的契约
- 若继续推进前端 E2E，应先修复 Playwright/dev 启动与远程 executor 路由契约，再恢复 critical case 的可信度
