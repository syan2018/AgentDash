# PRD：Session 生命周期与仓储恢复重构

## 背景

此前 session 的“首轮初始化 / reopen / restart 后继续会话”逻辑分散在多个层面：

- 前端会话页知道一部分 owner prompt 注入语义
- API route 自己判断是否属于首轮 bootstrap
- 执行器恢复更多依赖 continuation 文本，而不是结构化会话历史
- `SessionHub` 里只要已经有 broadcaster / backlog 条目，就可能被误判为“执行器还能热续跑”

结果是：只要服务重启、会话页 reopen、或历史流重放稍微绕一点，就很容易再次注入 project/story/task 上下文和初始化文本。

## 目标

把 session 生命周期收敛为 session 模型自身负责的统一机制，满足：

1. 前端只发送结构化 `promptBlocks`，不再承载 owner bootstrap 规则。
2. owner session 的首次初始化、冷启动恢复、热续跑，统一由 session 生命周期模型判定。
3. session 仓储能重建完整消息历史，而不是只退化成 continuation 摘要文本。
4. 支持仓储恢复的 connector 可以真正消费历史消息并继续运行。
5. reopen / restart 后再次 prompt，不会重复注入 owner context / 初始化逻辑。

## 非目标

- 不为旧 schema / 旧 API 做兼容包装。
- 不解决 discovery 与远程 executor prompt 路由未对齐的问题；该问题作为后续缺口记录。

## 核心设计

### 1. Session 生命周期三态

- `OwnerBootstrap`
  - 用于 owner session 首次 prompt。
  - 需要附加 owner resource 展示块，并把 owner markdown 放入 `system_context`。
- `RepositoryRehydrate`
  - 用于“已有历史，但执行器 live runtime 已不存在”的场景。
  - 分成两种恢复模式：
    - `SystemContext`
    - `ExecutorState`
- `Plain`
  - 仅在执行器仍持有 live runtime 时使用。

### 2. 热续跑判定下沉到 connector

- 不能再用 `SessionHub` 里是否已有 session broadcaster 作为判断依据。
- 必须通过 connector 能力 `has_live_session(session_id)` 判断执行器是否仍能热续跑。

### 3. 仓储恢复优先级

- 若 connector 支持 `supports_repository_restore(executor)`：
  - 优先重建结构化消息历史，放入 `ExecutionContext.restored_session_state`
- 否则：
  - 退化为 continuation `system_context`

### 4. Owner 上下文过滤

仓储恢复时必须过滤以下 owner resource block，避免首轮 bootstrap 内容被再次回灌给模型：

- `agentdash://project-context/*`
- `agentdash://story-context/*`
- `agentdash://task-context/*`

## 关键实现面

- 生命周期判定：
  - `crates/agentdash-application/src/session/types.rs`
- prompt pipeline：
  - `crates/agentdash-application/src/session/prompt_pipeline.rs`
- 仓储历史重建：
  - `crates/agentdash-application/src/session/hub.rs`
- route 收敛：
  - `crates/agentdash-api/src/routes/acp_sessions.rs`
- connector 恢复能力：
  - `crates/agentdash-spi/src/connector.rs`
  - `crates/agentdash-executor/src/connectors/composite.rs`
  - `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`
- 持久化：
  - `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
  - `crates/agentdash-infrastructure/src/persistence/sqlite/session_repository.rs`
  - `crates/agentdash-infrastructure/migrations/0003_sessions_bootstrap_state.sql`

## 验收标准

### 自动验证

- `cargo test -p agentdash-application session::hub::tests -- --nocapture`
- `cargo test -p agentdash-api session_prompt_lifecycle -- --nocapture`
- `cargo test -p agentdash-executor prompt_restores_repository_messages_before_new_user_prompt -- --nocapture`
- `cargo check -p agentdash-application -p agentdash-api -p agentdash-executor -p agentdash-infrastructure -p agentdash-local`
- `pnpm run frontend:check`
- `pnpm run frontend:test`

### 手工前端验证

在真实前端页面中验证以下链路：

1. 打开 owner session 并发送首条 prompt。
2. reopen 同一 session，再发第二条 prompt。
3. 重启 server/local 后 reopen 同一 session，再发第三条 prompt。

期望：

- owner context 卡片只出现一次。
- session 仓储流中的 owner resource 计数保持不变。
- 不会因为 reopen / restart 再次触发初始化注入。

## 本轮发现的后续缺口

- `/api/discovery` 会把在线 backend 的 executors 合并进前端下拉。
- 但云端 session prompt 路由当前仍依赖本地 `CompositeConnector`，远程 executor 未直接接入其中。
- 因此前端手工选择 `CODEX` 等远程 executor 时，当前仍可能报：
  - `未知执行器 'CODEX'，无法路由到任何连接器`

这个问题与本次 session 生命周期重构无直接冲突，但应单独收敛 discovery 与 prompt routing 契约。
