# 进度记录

## 2026-04-01

### 已完成

- 创建任务目录与 PRD
- 初始化 fullstack context
- 汇总 review findings 并按清理波次拆成子任务
- ST-01 第一批修复完成
- inline mount 注入 owner scope 元信息
- inline persistence 改为按 mount owner 持久化，不再 story -> project 猜测回退
- address space 删除“项目下第一个 workspace”兜底
- 前端主链路已切到 `agent-links` 命名空间
- 前端删除旧 task status 兼容映射
- 前端 Task mapper 对未知 `status/execution_mode` 改为显式报错
- 前端删除 executor localStorage 旧格式识别
- 前端删除裸 `SessionNotification` 兼容
- 前端默认不再自动从 NDJSON 降级到 SSE
- 前端删除 `useAcpStream` 中“异常累计 chunk 重复”去重补丁
- 前端 Project/Story session info 不再用请求参数回填缺失的 `binding_id`
- 前端 Project agent session/open summary mapper 不再把缺失 `binding_id` 静默映射为空串
- 前端 system event 仅按显式白名单渲染，不再按 `severity` 对未知事件兜底放行
- 前端项目会话列表 mapper 不再把未知 `execution_status/owner_type` 静默兜底到默认值
- 后端 project-session 上下文构建改为严格失败，不再吞掉坏 binding label / session meta / workspace 读取错误
- sqlite/postgres `session_repository` 读取坏 JSON 不再静默回退到 `None/[]`
- 后端 `project_agents` 不再静默跳过损坏的 agent link
- 后端 `open_project_agent_session` 删除对既有 session 返回值的 `unwrap_or_default` 兜底
- 后端 `resolve_agent_default_lifecycle` 删除旧 agent_key 解析分支，只接受明确的 link agent id
- 后端 Postgres `agent_repository` 不再把坏 `base_config/config_override` JSON 静默兜底为默认对象
- 后端 Postgres `task_repository` 不再把坏 `workspace_id/agent_binding/artifacts` 或未知 `status/execution_mode` 静默兜底为默认值
- Postgres/sqlite `session_repository` 不再把坏序列化、负 event seq、缺失 `visible_canvas_mount_ids_json` 吞成默认值
- session runtime / memory persistence / SessionStore 不再把缺 session、坏 JSON、坏 `last_event_id` 静默吞掉
- `SessionOwnerType` 已删除 loose parse，只接受精确字符串
- tool execution artifact 不再把坏对象内容 / 坏序列化吞成 `{}` / `[]` / `pending`
- 前端 `currentUser/session/workflow` mapper 已改为对未知枚举显式报错，不再静默回退到默认值

### 进行中

- ST-03 旧协议与旧路由清理
- ST-02 持久化层静默默认值清理

### 下一步

- 继续删除旧 project-agent 兼容入口
- 继续清理后端 preset MCP 的 backward-compat 解析
- 继续清理 project/task/story/workspace repository 中坏 JSON / 坏枚举 / 坏时间的静默默认值
- 继续清理前端 mapper 中把缺字段补空串 / 当前时间的宽松映射

### 验证记录

- `pnpm run frontend:check` 通过
- `pnpm run frontend:test` 通过
- `cargo check -p agentdash-api --message-format short` 通过
- `cargo check -p agentdash-infrastructure -p agentdash-api --message-format short` 通过
- `cargo check -p agentdash-application -p agentdash-api --message-format short` 通过
- `cargo test -p agentdash-infrastructure session_repository -- --nocapture` 通过
- `cargo test -p agentdash-infrastructure canvas_repository -- --nocapture` 通过
- `cargo test -p agentdash-application session::memory_persistence -- --nocapture` 通过
- `cargo test -p agentdash-application session::hub -- --nocapture` 通过
