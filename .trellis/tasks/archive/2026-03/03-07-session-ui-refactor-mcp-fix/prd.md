# 前端会话体验统一重构与 MCP 工具注入修复

## 背景

AgentDash 前端存在三处会话展示场景（SessionPage、StorySessionPanel、TaskAgentSessionPanel），但各自实现不一致，功能参差不齐。同时 Task Agent 的 MCP 工具注入在本地开发环境下因 `mcp_base_url` 未正确配置导致工具无法被发现。

## 目标

1. **统一会话体验**：提取可复用的 `SessionChatView` 组件，消除三处场景的重复实现
2. **优化信息架构**：侧栏会话列表排除已绑定会话；Story 详情页优化 Tab 布局
3. **Task 面板重设计**：将执行语义融入聊天流，消除冗余控件
4. **修复 MCP 注入**：确保本地环境 Task Agent 能正确发现并使用 MCP 工具

## 已完成工作

### 1. SessionChatView 可复用组件提取
- 从 SessionPage 提取核心聊天逻辑为独立 `SessionChatView`
- 支持注入插槽：`headerSlot`、`streamPrefixContent`、`inputPrefix`
- 支持行为定制：`customSend`、`onTurnEnd`、`showStatusBar`、`showExecutorSelector`
- 支持外观定制：`idleSendLabel`、`initialInputValue`、`inputPlaceholder`

### 2. 会话列表优化
- 后端 API 新增 `exclude_bound` 查询参数
- `SessionBindingRepository` 增加 `list_bound_session_ids` 方法
- 侧栏会话列表默认排除已绑定到 Story/Task 的会话

### 3. Story 详情页优化
- 默认 Tab 切换为 "sessions"
- 上下文信息从独立 Tab 移至顶栏可折叠区域
- Story 会话面板支持内联聊天（复用 SessionChatView）

### 4. Task 面板体验重设计
- 移除冗余的执行器/模型选择器（Task 自身已定义）
- 任务上下文卡片作为聊天流首个注入内容
- 发送按钮文本动态切换（"执行" / "发送"）
- 输入框预填充任务默认 prompt（仅初始化时）
- customSend 逻辑：首次发送触发任务执行，后续为普通聊天

### 5. MCP 工具注入修复
- `app_state.rs` 中 `mcp_base_url` 增加自动推导逻辑
- 未设置 `AGENTDASH_MCP_BASE_URL` 时自动使用 `http://127.0.0.1:{PORT}`

## 涉及文件

### 前端
- `frontend/src/features/acp-session/ui/SessionChatView.tsx` — 新增核心组件
- `frontend/src/features/acp-session/ui/index.ts` — 导出
- `frontend/src/pages/SessionPage.tsx` — 重构为 SessionChatView 包装器
- `frontend/src/pages/StoryPage.tsx` — Tab 与布局优化
- `frontend/src/features/story/story-session-panel.tsx` — 内联会话面板
- `frontend/src/features/task/task-agent-session-panel.tsx` — 重设计
- `frontend/src/features/task/task-drawer.tsx` — 布局约束
- `frontend/src/services/session.ts` — excludeBound 参数
- `frontend/src/stores/sessionHistoryStore.ts` — 默认排除绑定会话

### 后端
- `crates/agentdash-api/src/routes/acp_sessions.rs` — exclude_bound 过滤
- `crates/agentdash-api/src/app_state.rs` — MCP base URL 自动推导
- `crates/agentdash-domain/src/session_binding/repository.rs` — 新增 trait 方法
- `crates/agentdash-infrastructure/src/persistence/sqlite/session_binding_repository.rs` — 实现

## 关联 Commits

- `9480169` feat(api): 支持按 exclude_bound 过滤会话列表
- `6783de8` refactor(ui): 提取 SessionChatView 可复用聊天组件
- `258949f` refactor(ui): SessionPage/StorySessionPanel/TaskPanel 统一复用
- `d257edf` feat(ui): StoryPage 默认展示 sessions tab + 上下文折叠
- `fad36fa` refactor(ui): 重设计 SessionChatView 注入架构与 Task 会话体验
- `b988452` refactor(ui): Task 面板体验优化
- `1ebbb60` fix(mcp): 优化本机 mcp 识别
- `82f109b` 删除冗余指令
