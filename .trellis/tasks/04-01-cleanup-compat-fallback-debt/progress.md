# 进度记录

## 2026-04-02

### 已完成

- Hook Runtime `before_stop` 不再伪造 stop gate fallback steering 文本
- `StopDecision::Continue` 增加显式 `allow_empty` 语义，agent loop 允许结构化空 continue
- 增加 `runtime_alignment` / `hook_delegate` 测试，覆盖“空 continue 继续运行”与“stop gate 未满足但无假消息”场景
- `PostgresRuntime` 在 `DATABASE_URL` 非 PostgreSQL 协议时改为直接失败，不再静默回退 embedded
- 为 `postgres_runtime` 增加环境变量解析测试，覆盖 `postgres://` / `postgresql://` / 非法 scheme
- `turn_dispatcher` 中继 prompt 时不再静默丢弃序列化失败的 runtime MCP server，改为显式报错
- relay WebSocket handler 收到非法 relay JSON 协议包时改为显式报错并断开连接
- `story_sessions` / `acp_sessions` / `canvases` 复用 story session context 时不再把 project/workspace/session meta 读取错误降成“无上下文”
- `dev-joint` 删除 `embedded-postgresql(auto)` 这类伪 URL 哑值，数据库模式展示改为显式 embedded 文案
- `kill-ports.js` 端口清理失败时改为非 0 退出，不再伪装成功
- session prompt 主路径已移除 `prompt` 文本字段，只保留 `promptBlocks`
- project/story owner prompt 构建只接收结构化 blocks，不再合并 `original_prompt`
- `SessionHub` prompt pipeline 现在要求显式 `executor_config`，不再从 request/session meta 全部缺失时静默补默认执行器
- `acp_sessions` 构建 story/project owner prompt 时，缺失执行器配置会直接报错；project default 仅允许作为显式 `executor_config` 来源，不再隐式兜底执行路径
- relay `CommandPromptPayload` / local command handler 已删除旧 `prompt` 协议字段
- `project_agents` 现在严格解析 preset MCP 配置，坏 `name/type/url/command/headers/args/env` 直接报错，不再 warn + skip
- task preset `thinking_level` 非法时直接报错，不再静默忽略
- 前端 SessionChat / TaskAgentSessionPanel 已统一仅发送 `promptBlocks`
- 前端 `workflow.ts` / `storyStore.ts` 已继续收紧 mapper，去掉缺字段补空串 / 当前时间 / 默认状态 / 默认绑定的主路径兜底
- Task agent binding UI 已删除“自动选第一个 executor / 项目默认 / 第一个 preset”推断
- ACP SSE / NDJSON resume header 已改为严格校验，坏 header 不再回退到 `0`
- `dev-joint` 现在会显式拒绝非法 `DATABASE_URL`，并在传递 server 环境时清掉继承的脏值
- `dev-joint` / `wait-for-ready.js` 的 ready check 已收紧为只接受 `200`

### 进行中

- ST-06 开发基础设施与 embedded PostgreSQL 生命周期清理

### 下一步

- 本轮代码层可低风险推进的兼容/回退清理已基本收尾
- 剩余更大的尾项主要是 embedded PostgreSQL ownership / supervisor 生命周期建模，以及 workflow/schema runtime migration 的设计级收口
- 如继续推进，建议单独立 task 处理“dev runtime ownership”与“schema/runtime migration 收敛”，避免在当前批次继续堆补丁

### 验证记录

- `cargo test -p agentdash-agent --test runtime_alignment -- --nocapture` 通过
- `cargo test -p agentdash-application session::hook_delegate -- --nocapture` 通过
- `cargo test -p agentdash-infrastructure postgres_runtime -- --nocapture` 通过
- `cargo check -p agentdash-agent-types -p agentdash-agent -p agentdash-application -p agentdash-infrastructure -p agentdash-api --message-format short` 通过
- `cargo check -p agentdash-api --message-format short` 通过
- `node --check scripts/dev-joint.js` 通过
- `node --check scripts/kill-ports.js` 通过
- `cargo check -p agentdash-application -p agentdash-api -p agentdash-local -p agentdash-relay --message-format short` 通过
- `cargo test -p agentdash-application session::hub -- --nocapture` 通过
- `cargo test -p agentdash-api -- --nocapture` 通过
- `cargo test -p agentdash-local -- --nocapture` 通过
- `pnpm run frontend:check` 通过
- `pnpm run frontend:test` 通过
- `node --check scripts/wait-for-ready.js` 通过

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
- relay MCP server 解析已要求显式 `type`，不再根据 `url/command` 猜测 transport

### 进行中

- ST-03 旧协议与旧路由清理
- ST-02 持久化层静默默认值清理
- ST-04 执行器与 provider fallback 清理

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
- `cargo test -p agentdash-local -- --nocapture` 通过
