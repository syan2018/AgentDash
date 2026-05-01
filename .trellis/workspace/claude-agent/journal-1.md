# Journal - claude-agent (Part 1)

> AI development session journal
> Started: 2026-02-25

---


## Session 1: 项目初始化：前后端骨架搭建与联调验证

**Date**: 2026-02-25
**Task**: 项目初始化：前后端骨架搭建与联调验证

### Summary

完成 AgentDash 项目从零到可调试可运行状态的完整初始化，包括 Rust 后端三层 crate 架构、React 前端看板 UI、前后端联调验证。使用 pnpm 管理依赖，配置 `pnpm run full` 并发启动模式。

### Main Changes

| 模块 | 变更说明 |
|------|---------|
| Rust Workspace | 创建 `Cargo.toml` workspace，含 3 个 crate 成员，排除 third_party |
| agentdash-state | Story/Task 模型、StoryStatus/TaskStatus 枚举（snake_case 序列化）、StateChange 不可变日志、SQLite CRUD、Resume 接口 `get_changes_since()` |
| agentdash-coordinator | BackendConfig 后端管理、ViewConfig 视图配置、UserPreferences 用户偏好、SQLite 持久化 |
| agentdash-api | Axum HTTP 服务（端口 3001）、RESTful 路由、SSE 事件流、Resume 端点、统一错误处理、CORS |
| frontend 项目 | React 19 + TypeScript + Vite 7 + Tailwind CSS v4 + Zustand + React Router |
| frontend/stores | CoordinatorStore（后端管理）、StoryStore（Story/Task CRUD）、EventStore（SSE 事件流） |
| frontend/components | AppLayout 布局、Sidebar 侧边栏、Header 顶栏、KanbanBoard 看板、StoryCard 卡片 |
| 包管理 | 从 npm 切换到 pnpm workspace，配置 `pnpm-workspace.yaml`，解决 esbuild 构建脚本问题 |
| 启动脚本 | `pnpm run full` 并发启动前后端，Vite 代理 `/api` 到 3001 |

**关键修复**：
- Status 枚举添加 `#[serde(rename_all = "snake_case")]` 确保前后端一致
- SSE 事件流移除冗余重连逻辑，利用 EventSource 内建重连机制

### Git Commits

`efa411d` 项目初始化：前后端骨架搭建与联调验证

### Testing

- [OK] `cargo build` 编译通过
- [OK] `cargo run --bin agentdash-server` 后端正常启动
- [OK] `/api/health` 返回 `{"status":"ok","version":"0.1.0"}`
- [OK] `/api/backends` POST/GET/DELETE 功能正常
- [OK] `/api/stories` POST/GET 功能正常，status 返回 snake_case
- [OK] TypeScript 类型检查 (`tsc --noEmit`) 通过
- [OK] Vite dev server 正常启动
- [OK] 浏览器联调验证：侧边栏、看板、Story 卡片渲染正确
- [OK] SSE 事件流连接正常（绿色"已连接"状态）
- [OK] `pnpm run full` 并发启动模式正常

### Status

[OK] **Completed**

### Next Steps

- 实现 Story/Task 的 CRUD 完整流程（前端表单 + 后端验证）
- 完善 NDJSON 实时推送（StateChange 写入时广播事件）
- 集成 Agent Client Protocol 类型定义
- 实现 Task 执行容器（Agent 进程管理）
- 完善看板拖拽交互和视图筛选

## Session 2: 迁移 ABCCraft UI 到 AgentDashboard

**Date**: 2026-02-25
**Task**: 迁移 ABCCraft UI 到 AgentDashboard

### Summary

完成前端 UI 从 ABCCraft 到 AgentDashboard 的迁移，剔除 DAG 依赖，建立 Story/Task 两层扁平结构。

### Main Changes

| 模块 | 变更说明 |
|------|---------|
| 类型层 | CraftTask→Story, AgentTask→Task，移除 DAG 相关类型（AgentTaskDependency 等） |
| 组件迁移 | WorkspaceLayout, StoryListView, StoryDrawer, TaskDrawer |
| ACP 组件 | ContentBlock, ToolCall, Plan, ConfirmationRequest |
| 主题系统 | CSS 变量 + useTheme hook，支持浅色/深色/系统 |
| 状态管理 | storyStore 适配新类型，字段映射 snake_case→camelCase |
| 删除旧组件 | KanbanBoard, StoryCard, AppLayout, Header, Sidebar |

### Git Commits

| Hash | Message |
|------|---------|
| `163ec3e` | feat(frontend): 迁移 ABCCraft UI 到 AgentDashboard，剔除 DAG 依赖 |

### Testing

- [OK] `pnpm lint` 零错误
- [OK] `pnpm build` 编译通过（52 modules, 217KB JS）
- [OK] 浏览器验证：Story 列表按状态分组、中文正确显示
- [OK] Story Drawer 三个 Tab（上下文/任务列表/验收）功能正常
- [OK] 主题切换（浅色/深色/系统）正常
- [OK] 无 DAG/ReactFlow/dagre 残留引用

### Status

[OK] **Completed**

### Next Steps

- 完善 Task Drawer 执行日志渲染
- 实现 Story/Task 的创建和编辑功能
- 对接 Agent 执行引擎

## Session 3: 新增 Mock 数据脚本 + 中文修复

**Date**: 2026-02-25
**Task**: 新增 Mock 数据脚本 + 中文修复

### Summary

新增 `scripts/seed-mock-data.py` 统一维护 mock 数据，直接操作 SQLite 避免 PowerShell HTTP 编码问题，修复中文乱码。

### Main Changes

| 模块 | 变更说明 |
|------|---------|
| scripts/seed-mock-data.py | 新增 Python 脚本，生成 5 个 Story + 11 个 Task 的 mock 数据 |
| 数据修复 | 通过 `--clean` 清空旧乱码数据，重新生成正确中文数据 |

### Git Commits

| Hash | Message |
|------|---------|
| (待提交) | 包含在 Session 2 的 commit `163ec3e` 中 |

### Testing

- [OK] `python scripts/seed-mock-data.py --clean` 执行成功
- [OK] sqlite3 验证中文数据正确
- [OK] 浏览器验证所有中文标题、描述正确显示
- [OK] Story Drawer 上下文 Tab 显示 context items 正确

### Status

[OK] **Completed**

### Next Steps

- None - task complete

## Session 4: 接入 @agentclientprotocol/sdk 实现 ACP 会话渲染

**Date**: 2026-02-25
**Task**: 迁移前端绘制组件到 ACP 协议

### Summary

安装 `@agentclientprotocol/sdk` v0.14.1 npm 包，将前端 `acp-session` 模块的所有类型定义和组件对齐到 SDK 实际导出的类型结构。完成 model 层（types, useAcpStream, useAcpSession）和 UI 层（AcpSessionEntry, AcpToolCallCard, AcpMessageCard, AcpPlanCard, AcpSessionList）的完整实现。

### Main Changes

| 模块 | 变更说明 |
|------|---------|
| package.json | 添加 `@agentclientprotocol/sdk: ^0.14.1` 依赖 |
| model/types.ts | 从 SDK 导出 35+ 核心类型，定义前端扩展类型（AcpDisplayEntry, AcpToolCallState, AggregatedEntryGroup 等） |
| model/useAcpStream.ts | WebSocket 流管理 Hook，处理 SessionNotification，支持消息块合并和工具调用状态跟踪 |
| model/useAcpSession.ts | 会话管理 Hook，支持工具调用/思考/文件编辑三种聚合模式 |
| ui/AcpSessionEntry.tsx | 条目渲染分发组件，处理单条目和聚合组 |
| ui/AcpToolCallCard.tsx | 工具调用卡片，对齐 ToolCallContent 联合类型（content/diff/terminal） |
| ui/AcpPlanCard.tsx | 计划卡片，对齐 PlanEntry 类型（content/priority/status） |
| ui/AcpSessionList.tsx | 会话列表容器，支持自动滚动、连接状态、空/加载/错误状态 |

### Git Commits

| Hash | Message |
|------|---------|
| `8444aa4` | feat(frontend): 接入 @agentclientprotocol/sdk 实现 ACP 会话渲染组件 |

### Testing

- [OK] `tsc -b --noEmit` TypeScript 类型检查零错误
- [OK] `eslint` lint 检查零错误
- [OK] `vite build` 构建成功（52 modules, 217KB JS）

### Status

[OK] **Completed**

### Next Steps

- 后端提供 ACP 格式的 WebSocket 端点
- 对接真实 Agent 会话数据验证渲染效果
- 添加消息审批交互流程


## Session 5: executor integration layer

**Date**: 2026-02-25
**Task**: executor integration layer

### Summary

实现 Rust 后端执行集成层，复用 vibe-kanban executors，将执行日志转换为 ACP SessionNotification 并通过 WebSocket 流推送给前端；同时补齐前端 execute 消息发送与最小服务封装。

### Main Changes

> _注: 早期写入时中文字符因编码错误被替换为 `?`，原表格内容不可恢复，大意记录如下。_

| Area | Note |
|---|---|
| Backend | `crates/agentdash-api/src/executor/` 新建，封装 `third_party/vibe-kanban/crates/executors`，输出 ACP SessionNotification |
| Bridge | NormalizedEntry -> SessionNotification 适配 |
| API | `POST /api/sessions/{id}/prompt` + `GET /api/acp/sessions/{id}/stream` (WebSocket) |
| Frontend | `frontend/src/services/executor.ts` 封装，`useAcpStream` 在 WS open 后发送 `type=execute` |
| third_party | vibe-kanban executors 接入(codex_core / slash 等) + 修正 `Duration::from_mins` const |

**Testing**
- cargo check --workspace OK
- pnpm --filter frontend lint OK
- pnpm run frontend:check OK
- pnpm --filter frontend build OK
- pnpm run backend:check OK

### Git Commits

(No commits - planning session)

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 6: ACP 会话 SSE + Session MVP

**Date**: 2026-02-26
**Task**: ACP 会话 SSE + Session MVP

### Summary

后端新增 ACP 会话 SSE 流与 cancel API，前端 SessionPage 接入 EventSource 并优化流式合并/批处理刷新以避免重复与卡顿；完成 lint/build/cargo test 并提交。

### Main Changes



### Git Commits

| Hash | Message |
|------|---------|
| `7ce50ce` | (see git log) |
| `d569e34` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 7: ACP 会话 SSE + Session MVP

**Date**: 2026-02-26
**Task**: ACP 会话 SSE + Session MVP

### Summary

后端新增 ACP 会话 SSE 流与 cancel API，前端 SessionPage 接入 EventSource 并优化流式合并/批处理刷新以避免重复与卡顿；完成 lint/build/cargo test 并提交。

### Main Changes

﻿


### Git Commits

| Hash | Message |
|------|---------|
| `7ce50ce` | (see git log) |
| `d569e34` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 8: 完成SSE稳定化与Fetch Streaming改造

**Date**: 2026-02-26
**Task**: 完成SSE稳定化与Fetch Streaming改造

### Summary

后端新增全局/ACP NDJSON流与resume契约，前端引入transport抽象与HMR连接注册，完成跨层spec更新并归档任务。

### Main Changes



### Git Commits

| Hash | Message |
|------|---------|
| `3068c27` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 9: Project/Workspace/Story/Task 领域模型重构

**Date**: 2026-02-27
**Task**: Project/Workspace/Story/Task 领域模型重构

### Summary

| 变更 | 说明 |
|------|------|
| Project 模块 | 新增实体/Repository/SQLite实现/API路由 |
| Workspace 模块 | 新增实体/Repository/SQLite实现/API路由 |
| Story 扩展 | 添加 project_id，context 结构化为 StoryContext |
| Task 扩展 | workspace_id 替代 workspace_path，AgentBinding/Artifact 结构化 |
| Repository 扩展 | Story/Task 支持完整 CRUD + 按项目/工作空间查询 |
| API 层 | 新增 Project/Workspace 端点，更新 AppState |
| Mock 数据 | seed-mock-data.py 适配新领域模型 |
| Code-Spec | 更新 directory-structure/repository-pattern/index |

**新增文件**: project/(4), workspace/(4), project_repository.rs, workspace_repository.rs, projects.rs, workspaces.rs
**修改文件**: story/entity+repo+vo, task/entity+repo+vo, story_repository.rs, task_repository.rs, app_state.rs, routes.rs, stories.rs, seed-mock-data.py, spec docs(3)

### Main Changes



### Git Commits

| Hash | Message |
|------|---------|
| `026cf37` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 10: Project/Workspace/Story/Task 领域模型重构

**Date**: 2026-02-27
**Task**: Project/Workspace/Story/Task 领域模型重构

### Summary

建立完整的 Project-Workspace-Story-Task 领域模型层次。新增 Project/Workspace 模块（实体+Repository+SQLite+API），扩展 Story（project_id, StoryContext）和 Task（workspace_id, AgentBinding, Artifact），更新 mock 数据脚本和 code-spec 文档。

### Main Changes



### Git Commits

| Hash | Message |
|------|---------|
| `026cf37` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 11: Project/Workspace/Story/Task 领域模型重构

**Date**: 2026-02-27
**Task**: Project/Workspace/Story/Task 领域模型重构

### Summary

建立完整的 Project-Workspace-Story-Task 领域模型层次。新增 Project/Workspace 模块（实体+Repository+SQLite+API），扩展 Story（project_id, StoryContext）和 Task（workspace_id, AgentBinding, Artifact），更新 mock 数据脚本和 code-spec 文档。

### Main Changes

﻿


### Git Commits

| Hash | Message |
|------|---------|
| `026cf37` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 12: 前端适配 Project/Workspace 领域模型

**Date**: 2026-02-27
**Task**: 前端适配 Project/Workspace 领域模型

### Summary

## 前端适配 Project/Workspace 领域模型

| 变更 | 说明 |
|------|------|
| types/index.ts | 重写：新增 Project/Workspace 类型，Story/Task 类型对齐后端 snake_case |
| api/client.ts | 重写：新增 PUT/PATCH/DELETE 方法 |
| stores/projectStore.ts | 新增：Project CRUD + 选择逻辑 |
| stores/workspaceStore.ts | 新增：Workspace CRUD + 状态管理 |
| stores/storyStore.ts | 重写：从 backendId 切换到 projectId 驱动 |
| features/project/ | 新增：项目选择器 + 创建表单 |
| features/workspace/ | 新增：工作空间列表 + 创建面板（含目录选择） |
| workspace-layout.tsx | 重写：侧边栏加入项目/工作空间管理 |
| DashboardPage.tsx | 重写：projectId 驱动 |
| story/task 组件 | 更新：适配新字段结构 |
| spec docs (4) | 更新：index/directory-structure/state-management/type-safety |

**浏览器验证**: 项目创建/切换、工作空间创建、Story 创建、Drawer 展示全部通过

### Main Changes

﻿


### Git Commits

| Hash | Message |
|------|---------|
| `026cf37` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 13: 归档任务: Project 配置增强与 Story 下 Task 创建流程

**Date**: 2026-02-27
**Task**: 归档任务: Project 配置增强与 Story 下 Task 创建流程

### Summary

确认任务已完成并归档到 archive/2026-02/

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `6fa8e43` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 14: feat(file-reference): @ 引用工作空间文件功能实现

**Date**: 2026-03-02
**Task**: feat(file-reference): @ 引用工作空间文件功能实现

### Summary

实现 Session 输入框 @ 引用文件功能，前后端完整闭环

### Main Changes

| 模块 | 变更 |
|------|------|
| 后端 workspace-files API | 新增文件列表/读取/批量读取端点，路径安全校验 |
| 后端 PromptSessionRequest | 支持 promptBlocks (ContentBlock[])，兼容旧 prompt: string |
| 前端 file-reference feature | FilePickerPopup + useFileReference + buildPromptBlocks |
| 前端 SessionPage | 集成 @ 引用交互，引用标签可视化 |

**新增文件**:
- `crates/agentdash-api/src/routes/workspace_files.rs`
- `frontend/src/features/file-reference/` (6 files)
- `frontend/src/services/workspaceFiles.ts`

**修改文件**:
- `crates/agentdash-executor/src/hub.rs`
- `crates/agentdash-api/src/routes.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`
- `frontend/src/pages/SessionPage.tsx`
- `frontend/src/services/executor.ts`

**验证**: TypeScript 编译 ✅ | ESLint ✅ | Rust check ✅ | 浏览器测试 ✅


### Git Commits

| Hash | Message |
|------|---------|
| `0635bb1` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 15: Relay P1-P3: executor aggregation, workspace_files relay, backend management UI

**Date**: 2026-03-10
**Task**: Relay P1-P3: executor aggregation, workspace_files relay, backend management UI

### Summary

P1 discovery API aggregation, P2 remote workspace_files relay, P3 backend management UI with expandable cards

### Main Changes

﻿


### Git Commits

| Hash | Message |
|------|---------|
| `ee29fb8` | (see git log) |
| `612750f` | (see git log) |
| `a327fbc` | (see git log) |
| `f0201fe` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 16: Project/Backend 解耦架构分析 + MCP 连接配置任务规划

**Date**: 2026-03-23
**Task**: Project/Backend 解耦架构分析 + MCP 连接配置任务规划

### Summary

(Add summary)

### Main Changes

## 本次工作内容

纯分析和任务规划，无代码提交。

### 分析：Project ↔ Backend ↔ Workspace 诡异关系

对 Project、Workspace、Story、Task 四个实体的 `backend_id` 字段进行全面审查：

**发现的核心问题：**
- `Project.backend_id` 当前承担两个错误职责：1）创建 Workspace 时自动继承；2）作为 `resolve_task_backend_id` 的最终兜底
- 继承链 `Workspace → Story → Project` 全部是 `backend_id` 字符串传递，绕过 Workspace 实体
- `Story.backend_id` 无独立意义，仅作路由 fallback，掩盖 Task 未绑定 Workspace 的配置缺失
- `coordinatorStore.currentBackendId` 死状态，从未被消费
- `fetchStoriesByBackend` 死函数，无 UI 调用
- Workspace 创建时用户填绝对路径但不知道是哪台机器（UI 缺少 backend 选择）

**目标设计（清晰继承链）：**
```
Task.workspace_id（显式绑定）
  ↓ 未绑定时
Story.default_workspace_id（新增字段）
  ↓ 未设置时
Project.config.default_workspace_id（已存在！）
  ↓ 均无
Error
```
`backend_id` 永远从 Workspace 实体解析，Project/Story 只持有 `default_workspace_id`。

---

### 分析：MCP 连接配置问题

**发现两处 relay 路径硬编码丢弃：**
- `task_execution_gateway.rs:1369` — `mcp_servers: vec![]`（`built.mcp_servers` 未透传）
- `command_handler.rs:164` — `mcp_servers: vec![]`（`payload.mcp_servers` 被忽略）

**MCP 声明位于 `AgentPreset.config["mcp_servers"]`**（不是 Project/Story/Task 层），与 Agent 定义伴随。

**现有类型体系已完整**（`agent_client_protocol` v0.9.4）：
- `McpServer::Http` — 含 `headers: Vec<HttpHeader>` 字段（当前解析草率，只取 `name`+`url`）
- `McpServer::Sse` — 同上
- `McpServer::Stdio` — `command + args + env`（当前完全不支持）

---

### 新建任务

| 任务 | Slug | 主要内容 |
|------|------|---------|
| 从 Project 解耦 backend_id | `03-23-decouple-project-backend-id` | 移除 Project/Story 的 backend_id，建立 default_workspace_id 继承链，重写 resolve_task_backend_id |
| Agent MCP 连接完整支持 | `03-23-workspace-local-mcp-client` | 完整解析三种 transport（Http/SSE/Stdio）、打通 relay 透传、local 端 stdio 进程管理、前端结构化配置 UI |



### Git Commits

| Hash | Message |
|------|---------|
| `none` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 17: Hook 系统前端重构 + Session Context 迁移 + 执行层 Bug 修复

**Date**: 2026-03-23
**Task**: Hook 系统前端重构 + Session Context 迁移 + 执行层 Bug 修复

### Summary

(Add summary)

### Main Changes

## 本轮工作内容

本次 session 围绕 Hook 系统前端展示质量问题展开 review，发现并修复了多个相关问题，同时完成了 session context 注入架构的重构。

---

### 1. Hook 系统前端展示完整重构（refactor/hook）

**问题**：noop/stop/terminal_observed 等无意义 hook 事件无条件渲染为完整大卡片，会话流噪音严重；context_fragments 在 HookRuntimeSurfaceCard 中只显示计数，内容完全不可见。

**后端过滤逻辑重写**（`hook_events.rs`）：
- `should_emit_hook_trace_event` 改为基于"有无实际效果"判断，不再用 matched_rule_keys 数量触发
- 新增静默决策集合：stop / terminal_observed / refresh_requested
- 新增 success severity 映射 phase_advanced 等里程碑事件
- 扩展测试覆盖 7 个场景

**前端三路渲染分级**（`AcpSystemEventCard.tsx` 完整重构）：
- 高优先级干预型（deny/ask/rewrite/continue/block_reason）→ 完整大卡片
- 信息型（context_injected/steering_injected/phase_advanced）→ 可展开细条
- 通用系统事件 → 原有完整卡片

**Guard 重构**（`AcpSystemEventGuard.ts`）：
- turn_started / turn_completed 静默（发送按钮状态已表达）
- hook_event 增加 decision 级二次过滤

**SessionPage 扩展**：
- context_fragments 从纯计数 badge 改为 HookContextFragmentRow 可展开列表
- 新建 `AcpOwnerContextCard.tsx`：agentdash://project-context/ 和 story-context/ 专属渲染，蓝/紫 badge + 可展开

---

### 2. Project/Story 会话上下文迁移到 system_context（refactor/session）

**问题**：每次用户发消息，`augment_prompt_request_for_owner` 都把 Instruction + 来源摘要（`project_core(project), project_agent_identity(project)...`）塞进用户消息的 prompt_blocks，用户看到的消息流包含大量技术性无意义文本。

**架构调整**：
- `PromptSessionRequest` 新增 `system_context: Option<String>` 字段
- `ExecutionContext` 同步新增字段，hub 构建时传递
- `PiAgentConnector::build_runtime_system_prompt` 将 system_context 注入 system prompt 头部
- `build_project/story_system_context` 新函数：构造 system prompt 注入内容
- `build_project/story_owner_prompt_blocks` 重构：移除 instruction text block，仅保留 resource 展示锚点 + 用户原始消息

**效果**：用户消息流不再出现技术文本，resource block 渲染为专属 AcpOwnerContextCard。

---

### 3. 执行层 Bug 修复（fix/executor）

**Bug 1：before_stop 无 workflow 时误发 continue（导致 Agent 重复输出）**

根因：`completion_satisfied` 依赖 `is_some_and()`，无 workflow 时 `completion = None`，返回 false，导致 stop 条件永远不满足，发出空 steering 的 continue，Agent 重新进入对话输出相同内容。

修复：引入 `has_completion_gate`，无 gate 时视为可以停止。

**Bug 2：agent_message_chunk 重复渲染**

根因：`MessageEnd` 发出全量文本 chunk（正确行为），与前面 TextDelta 累积的内容无 overlap，`mergeStreamChunk` 走 `${previous}${incoming}` 拼接，产生重复渲染。

修复：前端 `useAcpStream.applyNotification` 在 chunk 合并前增加 entryIndex upsert。MessageEnd 和所有 TextDelta 共享相同 `entry_index`，通过 `(turnId, entryIndex, sessionUpdate)` 三元组识别同一消息，找到则直接覆盖而非拼接。**MessageEnd 行为不变**。

---

### 4. Spec 文档更新（docs/spec）

- `hook-guidelines.md`：更新可见事件列表、decision 级过滤规则、渲染分级说明、fragments 展示规范
- `execution-hook-runtime.md`：BeforeStop 无 workflow 约束、agent_message_chunk 合并协议
- `quality-guidelines.md`：system_context vs prompt_blocks 分工规范、PromptSessionRequest 扩展字段规范

---

## 关键文件

**后端**：
- `crates/agentdash-executor/src/hook_events.rs`
- `crates/agentdash-executor/src/runtime_delegate.rs`
- `crates/agentdash-executor/src/connector.rs` / `hub.rs`
- `crates/agentdash-executor/src/connectors/pi_agent.rs`
- `crates/agentdash-application/src/project/context_builder.rs`
- `crates/agentdash-application/src/story/context_builder.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`

**前端**：
- `frontend/src/features/acp-session/ui/AcpSystemEventCard.tsx`（完整重构）
- `frontend/src/features/acp-session/ui/AcpSystemEventGuard.ts`
- `frontend/src/features/acp-session/ui/AcpOwnerContextCard.tsx`（新建）
- `frontend/src/features/acp-session/model/useAcpStream.ts`
- `frontend/src/pages/SessionPage.tsx`


### Git Commits

| Hash | Message |
|------|---------|
| `81c701f` | (see git log) |
| `c383f9c` | (see git log) |
| `408bd95` | (see git log) |
| `13be286` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 18: 模型参数清理 + ThinkingLevel 统一 + LLM Provider 扩展

**Date**: 2026-03-23
**Task**: 模型参数清理 + ThinkingLevel 统一 + LLM Provider 扩展

### Summary

完成两个任务：(1) 清理 temperature/max_tokens 等 LLM 底层参数，reasoning_id 改为类型化 ThinkingLevel 枚举，打通 PiAgent 注入链路；(2) PiAgentConnector 扩展为多 provider registry（Anthropic/Gemini/DeepSeek/Groq/xAI/OpenAI），discover_options 动态化，前端 Settings 重构为统一 Provider 管理区，ModelInfo 增加 reasoning 元数据驱动 ThinkingLevel 选择器显示逻辑。新增 spec/backend/llm-model-config.md 记录跨层契约。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `5fedfb0` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 19: decouple-project-backend-id + workspace-local-mcp-client 实现完成

**Date**: 2026-03-23
**Task**: decouple-project-backend-id + workspace-local-mcp-client 实现完成

### Summary

(Add summary)

### Main Changes

## 完成内容

### 任务一：从 Project/Story 移除 backend_id（`b1db9a9`）

**核心变更：**
- `Project` 实体彻底移除 `backend_id` 字段，DB 加 backward-compat `DROP COLUMN IF EXISTS` 迁移
- `Story` 实体移除 `backend_id`，新增 `default_workspace_id: Option<Uuid>`
- `state_changes.backend_id` 改为 nullable
- `resolve_task_backend_id` 重写为清晰继承链：
  `Task.workspace_id → Story.default_workspace_id → Project.config.default_workspace_id → Error`
- `Workspace` 创建改为显式传入 `backend_id`（不再从 Project 继承）
- 前端清理：Project 创建/编辑移除 backend 选择器；Workspace 创建表单新增 backend 必选下拉；`coordinatorStore.currentBackendId` / `fetchStoriesByBackend` 死代码删除

**涉及文件：** 33 个文件，536 增 / 305 删

---

### 任务二：AgentPreset MCP 完整 transport 支持（`bff647d`）

**核心变更：**
- `build_preset_bridge` 重写：完整解析 Http/SSE/Stdio 三种 transport（含 headers/env/args），向后兼容
- 修复 `task_execution_gateway` relay 路径硬编码 `mcp_servers: vec![]`
- 修复 `agentdash-local` `command_handler` 硬编码 `mcp_servers: vec![]`
- 前端 `agent-preset-editor` 新增 `McpServersEditor` 组件，支持三种 transport 结构化配置

**涉及文件：** 9 个文件，447 增 / 18 删



### Git Commits

| Hash | Message |
|------|---------|
| `b1db9a9` | (see git log) |
| `bff647d` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 20: Agent-First 前端重构 + Session 聚合端点 + 执行状态持久化

**Date**: 2026-03-24
**Task**: Agent-First 前端重构 + Session 聚合端点 + 执行状态持久化

### Summary

(Add summary)

### Main Changes

## 本次会话完成内容

### 前端信息架构重构（Agent-First）

路由与导航：
- Layout Route 重构：WorkspaceLayout 升为真正的 Layout Route，`<Outlet />` 替代 `children`
- 默认路由 `/` 重定向至 `/dashboard/agent`，Tab 切换用 URL 表达（`/dashboard/agent`、`/dashboard/story`）
- 侧边栏精简：移除"看板/会话"导航，改为项目选择器（置顶）+ Tab 导航（Agent/Story）+ 后端状态
- Tab 高亮继承：`/session/:id` 时 Agent Tab 高亮，`/story/:storyId` 时 Story Tab 高亮（用 `useMatch` 各自独立调用，避免 Hook 顺序问题）

Agent Tab 主视图：
- 双栏布局：左栏 ProjectAgentView（完整 Agent Hub，含创建/编辑/删除预设）+ 右栏活跃 Session 列表
- 左栏复用现有 ProjectAgentView，新增逻辑：有活跃 session 时只显示"新对话"按钮，单列竖排，对齐 h-14 Header 设计语言
- 右栏 ActiveSessionList：Companion 树形嵌套（父子会话连接线）、Task/Story 归属链接（`e.stopPropagation` + 导航）、running 脉冲动画、状态 badge
- 点击 Session 卡片右栏原地展开 SessionChatView + 面包屑返回，"全屏↗"导航到 `/session/:id`

状态管理：
- `activeSessionsStore`：竞态保护（`loadedProjectId` 比对丢弃旧请求），`clearForProject` 切换时立即清空旧数据

### 后端 Session 聚合端点（GET /api/projects/{id}/sessions）

性能重构（O(N×M) → O(1 DB + N parallel IO + 1 lock)）：
- Domain 层新增 `ProjectSessionBinding` 类型，携带归属上下文
- `SessionBindingRepository` 新增 `list_by_project` 接口：一条 UNION SQL 查出所有层级 bindings + 归属上下文（JOIN stories/tasks）
- `ExecutorHub` 新增 `get_session_metas_bulk`（并发读取）和 `inspect_execution_states_bulk`（单次 lock 读内存 + meta）
- Handler 重写：`tokio::join!` 并发拉取项目信息和 bindings，批量处理替代串行

### Session 执行状态持久化

- `SessionMeta` 新增 `last_execution_status` 字段（`serde(default)` 兼容旧文件）
- 在 turn 开始时写入 `running`，turn 结束时写入 `completed/failed/interrupted`
- 新增 `recover_interrupted_sessions()`：启动时扫所有 meta，修正残留 `running` 为 `interrupted`
- 两个 binary（cloud + local）启动时都调用恢复
- 移除 `Ok(None)|Err(_)` fallback 构造，session 不存在或 IO 失败均返回明确错误
- 修复 3 个测试：改为先 `create_session` 再 `start_prompt`，用返回 ID 替代硬编码

### Spec 文档更新

- `spec/backend/quality-guidelines.md`：新增 Session 执行状态持久化规范（字段约束、启动恢复机制、合法值枚举）


### Git Commits

| Hash | Message |
|------|---------|
| `30fb074` | (see git log) |
| `e3f79c6` | (see git log) |
| `69a2e55` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 21: 上下文容器创建优化 + Address Space 搜索能力增强

**Date**: 2026-03-25
**Task**: 上下文容器创建优化 + Address Space 搜索能力增强

### Summary

1. 前端容器创建逻辑优化：ID 改为域+自增序号自动生成，display_name 仅作备注，移除创建卡片的 inline_files 编辑区，默认能力增加 write/search，挂载策略默认全部能力打开。2. 后端搜索增强：relay 协议新增 CommandToolSearch/ResponseToolSearch，本机后端优先 ripgrep 搜索+自动降级 fallback，云端按 provider 路由分发，inline_fs 升级正则匹配，FsSearchTool 新增 regex/include/context_lines 参数。

### Main Changes

**前端容器创建优化（`475dfc8`）**
- `context-config-editor.tsx`: 移除 slugify，新增 generateNextId（域+自增序号），display_name 仅作备注不驱动 ID
- `context-config-editor.tsx`: 移除创建卡片的 inline_files 文件编辑区，Provider 内容编辑全部移入高级选项后改为只读展示
- `context-config-defaults.ts`: 挂载策略默认能力改为全部五项
- `ProjectSettingsPage.tsx` / `StoryPage.tsx`: 传入 domain prop

**搜索增强（`65a0a92`）**
- `agentdash-relay/protocol.rs`: 新增 CommandToolSearch/ResponseToolSearch + ToolSearchPayload/ToolSearchResponse/SearchHit
- `agentdash-local/tool_executor.rs`: 新增 search() 方法，detect_ripgrep + run_ripgrep + fallback_search
- `agentdash-local/command_handler.rs`: 新增 CommandToolSearch 路由 + handle_tool_search
- `agentdash-api/address_space_access.rs`: search_text 重构为 search_text_extended，relay_fs 走专用搜索命令，inline_fs 升级正则匹配，FsSearchTool 新增 regex/include/context_lines

### Git Commits

| Hash | Message |
|------|---------|
| `475dfc8` | fix(container): 优化上下文容器前端创建呈现 |
| `fae1b6f` | docs(trellis): 追踪搜索实现重构 |
| `65a0a92` | feat(search): 实现 Address Space 搜索能力增强——本地 ripgrep + 正则支持 |

### Testing

- [OK] 前端 TypeScript 类型检查通过
- [OK] agentdash-relay 编译通过
- [OK] agentdash-local 编译通过
- [OK] agentdash-api 编译通过（预存 workflow 错误与本次无关）

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 23: Workflow Editor 全栈实现 + 独立视图/编辑器页面

**Date**: 2026-03-25
**Task**: Workflow Editor 全栈实现 + 独立视图/编辑器页面

### Summary

实现 Workflow Definition 编辑器全栈支持：后端领域模型扩展、结构化校验、CRUD API、SQLite schema 自动迁移；前端编辑器组件和 Zustand store；新增 Workflow 顶级视图和独立编辑器页面

### Main Changes




### Git Commits

| Hash | Message |
|------|---------|
| `851055b` | (see git log) |
| `300107f` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 24: Workflow/Lifecycle 运行时重构收口

**Date**: 2026-03-25
**Task**: Workflow/Lifecycle 运行时重构收口

### Summary

(Add summary)

### Main Changes

> _注: 早期写入时中文字符因编码错误被替换为 `?`, 原表格仅剩以下可恢复片段。_

| Area | Note |
|------|------|
| Workflow Contract | `injection / hook_policy / completion` 相关字段;`outputs` 语义调整 |
| Lifecycle Runtime | workflow / lifecycle authority 下的 `EffectiveSessionContract` |
| Hook Runtime | hook runtime / `instructions` fragment / constraints |
| Frontend | Workflow / Lifecycle 相关视图 |
| Builtins / Docs | workflow JSON / spec 同步更新 |

**Testing**:
- `cargo check -p agentdash-api -p agentdash-executor -p agentdash-local`
- `cargo test -p agentdash-api execution_hooks`
- `cargo test -p agentdash-executor hub`
- `pnpm --filter frontend exec tsc --noEmit`
- `pnpm --filter frontend exec vitest run src/pages/SessionPage.hook-runtime.test.tsx`
- `pnpm --filter frontend lint`
- `pnpm --filter frontend build`
- `pnpm --filter frontend test`


### Git Commits

| Hash | Message |
|------|---------|
| `06e6163371cb029e0251cf8437aa5dd0e7fc1d78` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 25: Agent Entity + Lifecycle Binding: Full Migration

**Date**: 2026-03-26
**Task**: Agent Entity + Lifecycle Binding: Full Migration

### Summary

Promoted Agent to independent entity with Agent+ProjectAgentLink model. Full backend CRUD, rewrote frontend Agent Hub, auto-start lifecycle run, removed legacy code, fixed Zustand bug and layout. E2E verified.

### Main Changes




### Git Commits

| Hash | Message |
|------|---------|
| `0a3c401` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 26: 统一 Mount Patch 能力推进到组合式 Apply Patch

**Date**: 2026-03-31
**Task**: 统一 Mount Patch 能力推进到组合式 Apply Patch

### Summary

完成 MountProvider 编辑 primitive/capability 扩展、共享 apply_patch 组合执行、relay/local delete/rename 链路与测试文档同步。

### Main Changes

> _注: 早期写入时中文字符因编码错误被替换为 `?`, 原表格及要点不可恢复, 核心变更参见 Summary 与对应 commit。_

| Area | Note |
|------|------|
| MountProvider | 扩展 edit_capabilities / delete_text / rename_text, 引入 MountEditCapabilities |
| apply_patch | `address_space/apply_patch.rs` 组合式执行 + provider native fallback |
| relay_fs | 新增 file_delete / file_rename primitive, create/delete/rename 对齐 |
| inline_fs | overlay target 的 patch 适配 |
| Protocol | relay protocol / local command handler / tool executor 同步 delete/rename |
| Docs/Tasks | address-space-access spec / 03-31-mount-apply-patch-capability PRD |
| Testing | `cargo test -p agentdash-application -p agentdash-local -p agentdash-api` 通过 |


### Git Commits

(No commits - planning session)

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 27: Agent 创建面板配置项补全 + ToolCluster 簇化注入管线

**Date**: 2026-04-02
**Task**: Agent 创建面板配置项补全 + ToolCluster 簇化注入管线

### Summary

(Add summary)

### Main Changes

## 概述

Review Agent 创建面板后发现大量后端已支持的配置未暴露给前端，同时 FlowCapabilities 的硬编码 bool 字段模式不利于扩展。本次会话完成了两大块工作：

1. **Agent 创建面板配置项补全** — 复用 PresetFormFields 组件，创建时即可配置模型、MCP 工具集、权限策略等
2. **ToolCluster 簇化注入管线** — 将 FlowCapabilities 从固定字段改为枚举集合，工具按簇动态裁剪

## Commit 1: feat — Agent 创建面板补全

| 改动 | 说明 |
|------|------|
| CreateAgentDialog 增强 | 复用 PresetFormFields，创建时可配 display_name/description/模型/权限/thinking_level/MCP servers |
| Agent 卡片「配置」按钮 | 打开 SinglePresetDialog 编辑已有 agent 的 base_config |
| Story/Task 默认 toggle | is_default_for_story / is_default_for_task 从只读 badge 改为可点击 toggle |
| thinking_level bug fix | executor_config_from_agent_config 遗漏 thinking_level 提取，已修复 |
| 导出共享组件 | PresetFormFields / presetToForm / formToPreset / useAgentTypeOptions 从 agent-preset-editor 导出 |

## Commit 2: refactor — ToolCluster 簇化注入管线

| 改动 | 说明 |
|------|------|
| ToolCluster 枚举 | 6 个簇：Read / Write / Execute / Workflow / Collaboration / Canvas |
| FlowCapabilities 改造 | 从 5 个 bool → BTreeSet\<ToolCluster\>，提供 all() / from_clusters() / intersect() |
| build_tools 按簇注入 | 7 个 fs 工具拆分到 Read/Write/Execute 簇，resolve_hook_action 合并入 Collaboration |
| agent 级 tool_clusters | AgentConfig 新增 tool_clusters 字段，provider 层做 session 默认 ∩ agent 限制 交集 |
| 前端工具权限 UI | PresetFormFields 新增「工具权限」折叠区（checkbox 组，空 = 不限制） |
| 死代码清理 | 删除从未实例化的 BuiltinToolset（builtins.rs / support.rs），移除 PiAgentConnector.tools |

## 关键文件

- `crates/agentdash-spi/src/connector.rs` — ToolCluster 枚举 + FlowCapabilities 重构
- `crates/agentdash-application/src/address_space/tools/provider.rs` — build_tools 按簇注入
- `crates/agentdash-domain/src/common/agent_config.rs` — tool_clusters 字段
- `crates/agentdash-api/src/routes/project_agents.rs` — thinking_level fix + tool_clusters 提取
- `frontend/src/features/project/project-agent-view.tsx` — 增强 CreateAgentDialog + 卡片编辑/toggle
- `frontend/src/features/project/agent-preset-editor.tsx` — 导出 + 工具权限 UI
- `frontend/src/types/index.ts` — ToolCluster 类型 + TOOL_CLUSTER_OPTIONS

## 发现与决策

- **来源 A 工具是死代码**：BuiltinToolset 从未被实例化，PiAgentConnector.tools 始终为空 Vec，安全删除
- **Hook 合并入 Collaboration**：resolve_hook_action 唯一生产者是 companion 协作流程，单独建簇无意义
- **簇名选择 Collaboration**：涵盖 companion dispatch/complete + hook resolve，未来 ask-user 也可纳入


### Git Commits

| Hash | Message |
|------|---------|
| `eab63d3` | (see git log) |
| `3ccfe9b` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 28: Session 生命周期与仓储恢复重构

**Date**: 2026-04-03
**Task**: Session 生命周期与仓储恢复重构

### Summary

完成 session 生命周期与仓储恢复主线重构，补齐 migration/spec/task，并通过真实前端 reopen/restart 回归验证 owner bootstrap 不重复注入。

### Main Changes

> _注: 早期写入时中文字符因编码错误被替换为 `?`, 原表格与要点不可恢复, 关键改动大意如下, 详情参见对应 commit。_

| Area | Note |
|------|------|
| Session lifecycle | owner bootstrap / repository rehydrate / plain follow-up 三种 session 启动 route 划分 |
| Events | `SessionHub` 与 `session_events` 聚合 user / assistant / tool_call / tool_result, 并带 owner resource block |
| Connector | `AgentConnector` 新增 `supports_repository_restore()` / `has_live_session()`;`PiAgentConnector` 处理 `restored_session_state` |
| Storage | PostgreSQL / SQLite session repository 加 `bootstrap_state`, 新增 Postgres migration `0003_sessions_bootstrap_state.sql` |
| Frontend | 调整 prompt / reopen / restart / reopen+prompt / owner context 行为 |
| Schema 修正 | Postgres `stories.task_count INTEGER` 与 `i64/INT8` 对齐 |
| Discovery | executors discovery 对 session prompt 与 `CODEX` 的处理 |

**Spec / Knowledge Capture**:
- 更新 `backend/quality-guidelines.md`: session prompt lifecycle / repository rehydrate 相关约束
- 补充 `guides/cross-layer-thinking-guide.md`: session != 线性消息 / continuation 场景说明
- 调整 `AGENTS.md`: discovery 与 prompt 相关指南
- 新建并归档 task `04-03-session-lifecycle-repository-rehydrate`

**Testing**:
- `cargo test -p agentdash-application session::hub::tests -- --nocapture`
- `cargo test -p agentdash-api session_prompt_lifecycle -- --nocapture`
- `cargo test -p agentdash-executor prompt_restores_repository_messages_before_new_user_prompt -- --nocapture`
- `pnpm run frontend:check`
- `pnpm run frontend:test`
- `pnpm run frontend:lint` (早期写入的附注因编码错误丢失)


### Git Commits

| Hash | Message |
|------|---------|
| `b0bdb9f` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 29: 5 条 session 启动路径统一到 SessionRequestAssembler

**Date**: 2026-04-21
**Task**: 5 条 session 启动路径统一到 SessionRequestAssembler
**Branch**: `main`

### Summary

引入 session/assembler.rs 作为统一组装终点，迁移 ACP Story/Project、Task、Routine、LifecycleOrchestrator AgentNode、Companion 共 5 条路径。PR4-G 清理删除 plan_builder.rs、死代码 load_available_presets / build_companion_flow_capabilities、以及 RepositorySet 收敛后残留的 trait import。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `0b352f0` | (see git log) |
| `e7f96c5` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 30: Workflow LifecycleEdge flow/artifact 双维度重构

**Date**: 2026-04-21
**Task**: Workflow LifecycleEdge flow/artifact 双维度重构
**Branch**: `main`

### Summary

LifecycleEdge 拆分 flow/artifact kind，移除运行时 fallback 线性推进，migration 0017 补齐存量。前端 DAG 编辑器按 kind 分派渲染（flow 实线 primary、artifact 虚线 + port label）；连接创建按 handle 是否落在 port 上判定 kind。builtin_workflow_admin 补齐 plan→apply flow edge，修复前端 DAG 预览空荡、后端靠 fallback 偷跑的语义割裂。spec/backend/workflow-lifecycle-edge.md 记录契约与校验规则，配套开 04-21-workflow-lifecycle-branching-design 占位任务承接 condition/fork-join 讨论（锚点：倾向 hook/agent tool 信号而非 DSL 表达式）。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `f8defc9` | (see git log) |
| `62f2a4f` | (see git log) |
| `0b56c20` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete

---

## 2026-04-30 · Session Pipeline Architecture Refactor — PR 1 启动与 Phase 1a/1b 交接

### 上下文

用户请求：
1. 对 session→agent 管线做彻底 review；
2. 产出重构方案；
3. 归档错激活的旧 task（`04-29-cloud-agent-context-bundle-convergence`）并起新的 epic；
4. 开分支并按阶段 commit，用户暂时离开。

### 前半段工作（main 分支，commit `c5c1bf4`）

- 派 3 路 research 子 agent 并行摸清 runtime / context / connector+hook 三层，产出：
  - `.trellis/tasks/04-30-session-pipeline-architecture-refactor/research/pipeline-review/`
    下 `01-runtime-layer.md` / `02-context-layer.md` / `03-connector-hook-layer.md`。
- 汇总成 `00-refactor-plan.md`（~900 行）：5 条正交轴、7 个 PR、6 个决策点、PR 依赖图、风险与顺序。
- AskUserQuestion 对齐：单一大 epic、命名 `session-pipeline-architecture-refactor`、
  D1-D6 写入推荐方案可后续调整。
- 归档旧 task → `archive/2026-04/`；创建新 task 并写入 PRD；把 pipeline-review 迁至新 task。
- 全部文档打包在 `c5c1bf4` 提交。

### 后半段工作（分支 `refactor/session-pipeline-pr1`）

#### Phase 1a · commit `cfa26c9` — finalize_request 对称化 + 11 条单测

**改动**：`crates/agentdash-application/src/session/assembler.rs`

- `relay_mcp_server_names` 从 `.extend()` 改为整体替换，与 `mcp_servers` 对称。
  所有生产路径 base 里这两个字段都是 `PromptSessionRequest::from_user_input` 的 default 空集，
  零行为变化。
- `vfs` 三重分支简化为"prepared 非空则覆盖"（行为等价）。
- 新增 `#[cfg(test)] mod finalize_request_tests` 11 条用例，覆盖 mcp / relay_mcp /
  vfs / prompt_blocks / context_bundle / workspace_defaults 6 个字段的替换 vs 保留语义。
- 文档注释重写为合并策略表格。

**验证**：`cargo test -p agentdash-application --lib` → 272 passed（baseline 261 + 我新增 11）。

#### Phase 1b · commit `c922b87` — routine identity 设计级缺口转为 Open Question

**背景**：原计划"顺手修复 routine/executor.rs identity 漏填"。深入发现 routine 是
cron/webhook/plugin 触发的无人类身份场景，填 identity 要先在三种路径里拍板：

- (a) 保持 None —— 审计无归属；
- (b) 合成 system identity `user_id = "system:routine:<id>"` —— 推荐，但需 review auth 政策；
- (c) 查 routine/project owner —— schema 需 `created_by` 字段，改动大。

决策有 auth 政策含义（is_admin / groups / provider），不宜在无 review 下拍板。

**改动**：

- `crates/agentdash-application/src/routine/executor.rs`：在 base 构造点打 TODO 注释
  列出三种备选，指向 PRD Open Questions。
- `.trellis/tasks/04-30-session-pipeline-architecture-refactor/prd.md` Open Questions 段新增
  "Routine 身份问题"条目，推荐方案 (b)。

无实际行为变化。

#### Phase 1c · **未推进**（主动停住）

**为什么停**：

- Phase 1c 的 `SessionAssemblyBuilder` 承载 identity/post_turn_handler 涉及 5 入口文件迁移；
- 其设计与 PR1 的 D1 决策（`PromptSessionRequest` 是否保留为 wire DTO）强耦合；
- routine identity 语义未定，migration 会陷入"先 mock None 再改"的双次重构；
- 留下半迁状态给用户的代价 > 停住的代价。

### 当前分支状态

- `main` 尖端：`c5c1bf4`（docs/任务管理）
- `refactor/session-pipeline-pr1` 尖端：`c922b87`，领先 main 2 个 commit：
  - `cfa26c9` refactor(session): finalize_request 对称化 + 单测
  - `c922b87` docs(session): routine identity 缺口 TODO + Open Question
- 测试：272 passed / 0 failed；clippy 全绿（报错在未触 crate `agentdash-domain`，pre-existing）。

### 回归风险

- 本次两个 commit 均为**零行为变化**：对称化前后 base 所有字段 default 的生产路径语义等价；
  TODO 注释不改代码；Open Question 只改 PRD。
- 即使恢复 `main`（`git reset --hard main`）也无功能影响。

### 恢复工作的第一步

1. `git log --oneline main..refactor/session-pipeline-pr1` 确认两个 commit 在；
2. 读 PRD Open Questions（尤其 "Routine 身份问题"）并选定路径；
3. 按 PRD PR 1 的 Implementation Plan 往下做：
   - Phase 1c.i：`SessionAssemblyBuilder` 加 identity / post_turn_handler 字段 + method；
   - Phase 1c.ii：`PreparedSessionInputs` 同步加字段；
   - Phase 1c.iii：`finalize_request` 扩展合入新字段的逻辑（注意 post_turn_handler 的 Option 所有权）；
   - Phase 1c.iv：按文件逐一迁移 5 条入口（HTTP / task / workflow / companion / routine）。
4. D1 决策（保留 `PromptSessionRequest` 作 wire DTO 还是合并）影响 1c 的实现形态——
   若决定合并为 `SessionStartupPlan`，Phase 1c 要把目标从"扩展现有 DTO"改成"引入新类型"。

### 关键文件定位（回流用）

- PRD：`.trellis/tasks/04-30-session-pipeline-architecture-refactor/prd.md`
- 重构蓝图：`.trellis/tasks/04-30-session-pipeline-architecture-refactor/research/pipeline-review/00-refactor-plan.md`
- 三层研究：同目录 `01-runtime-layer.md` / `02-context-layer.md` / `03-connector-hook-layer.md`
- 当前 Phase 1 产出：`crates/agentdash-application/src/session/assembler.rs`（`finalize_request` 与
  `#[cfg(test)] mod finalize_request_tests`）
- routine TODO 现场：`crates/agentdash-application/src/routine/executor.rs:493`


---

## 2026-04-30 (续) · PR 1 全程完成：入口节拍统一收口

### 前置：架构决策锁定

用户在 PR 1 执行前一次性锁完所有 Open Question，避免中途中断：
- **E1-E8** 决策写入 `prd.md` Decisions 段（commit `14f8e95`）
- **target-architecture.md**（~650 行，commit `5529732` + mermaid 语法修复 `新版`）——DoD 级交付物，12 核心概念辞典 + 5 正交轴 + 10 Invariant + before/after 对比 + 顶层架构 mermaid + 3 张 per-phase 序列图

### Phase 1c · commit `b537450` · SessionAssemblyBuilder 扩字段

- `PreparedSessionInputs` 扩 3 字段（env / identity / post_turn_handler），去 Debug derive（DynPostTurnHandler trait 不要求 Debug）
- `SessionAssemblyBuilder` 扩同 3 字段 + 新方法：
  - `with_env` / `with_identity` / `with_optional_identity`
  - `with_post_turn_handler` / `with_optional_post_turn_handler`
  - `with_user_input` 一次吸收 UserPromptInput 的 4 字段便捷方法
- `apply_companion_slice` 修复 struct 字面量保留已注入字段不被 slice 清空
- `finalize_request` 合入新字段，doc 更新合并策略表格
- `AuthIdentity::system_routine(id)` 构造器（PRD E1 契约）：auth_mode=Personal / user_id=system:routine:<id> / is_admin=false / provider=system.routine

新增 7 条测试（accum 18 条 finalize_request_tests）。

### Phase 1d · commit `d5a249a` · 5 入口迁移

| 入口 | 旧模式 | 新模式 |
|---|---|---|
| HTTP `acp_sessions.rs:932` | `req.identity = Some(current_user)` 在 augment 之后 | identity 前置到 base_req，augment 链透传 |
| `task/service.rs:269` | `req.identity = identity; req.post_turn_handler = Some(...)` 在 finalize 之后 | `prepared.identity / post_turn_handler = ...` 前置 |
| `routine/executor.rs` | TODO 占位（Phase 1b） | `prepared.identity = Some(AuthIdentity::system_routine(routine.id))` —— E1 落地 |
| `companion/tools.rs:382` & `:1534` | `PromptSessionRequest { ... 11 字段 }` 字面量 | `PromptSessionRequest::from_user_input(...)` 工厂 |
| `local/command_handler.rs:199` | 同上 11 字段字面量 | `from_user_input` + 逐项 `req.x = ...` |
| `workflow/orchestrator.rs` | 无 identity 写入 | 保持现状（backend-driven 嵌套，无 human identity 语义） |

**Invariant I3 当前状态**：`PromptSessionRequest { ... }` 字面量构造仅余 `session/hub.rs` 的 3 个测试 helper / fixture（`simple_prompt_request` / `owner_bootstrap_request` / 测试字面量）；生产代码零 struct-literal。

### PR 1 全流程汇总

分支 `refactor/session-pipeline-pr1` 从 `c5c1bf4`（main）起，共 6 个 commit：

| # | Hash | Title | 阶段 |
|---|---|---|---|
| 1 | `cfa26c9` | finalize_request 对称化 + 11 单测 | Phase 1a |
| 2 | `c922b87` | routine identity TODO + Open Question | Phase 1b（后被 1d 撤回） |
| 3 | `7c1ac46` | journal · Phase 1a/1b 中期交接 | 中期 |
| 4 | `14f8e95` | PRD Decisions E1-E8 锁定 | kickoff |
| 5 | `5529732` | target-architecture.md 撰写 | kickoff |
| 6 | `b537450` | SessionAssemblyBuilder 扩字段 | Phase 1c |
| 7 | `d5a249a` | 5 入口迁移 | Phase 1d |

### 测试状态

- `cargo test --workspace --lib`: 所有 crate tests 全绿
- `cargo test -p agentdash-application --lib`: 279 passed / 0 failed（baseline 261 + 11 Phase 1a + 7 Phase 1c = 279）
- `cargo build --workspace --lib`: 绿，唯一 warning 是 `cleanup_cloned_project` dead_code（pre-existing，与本 PR 无关）
- `cargo clippy` 对已改 crate 无新增 warning

### PR 1 Acceptance Criteria 对齐

从 PRD Acceptance Criteria 核对：

- [x] 6 条 `start_prompt` 入口全部通过 `SessionAssemblyBuilder` 装配（workflow 继承无 identity 保持现状）
- [x] `routine/executor.rs` identity 非空（system_routine）
- [x] `finalize_request` 的单测覆盖：mcp / relay_mcp / vfs / prompt_blocks / context_bundle / workspace_defaults / env / identity 全字段

PR 1 其他目标（ExecutionContext split 等）属于 PR 2-7。

### 下一步（PR 2 起）

- PR 2: ExecutionContext 拆 SessionFrame + TurnFrame + 删 effective_capability_keys dead_code
- PR 3: Bundle 进 TurnFrame + PiAgent 读 Bundle
- PR 4: Hook 三类语义分离 + turn_delta + 废 SKIP_SLOTS + SessionBootstrapAction → HookSnapshotReloadTrigger
- PR 5: contribute_* 去重 + companion bundle 裁剪 + continuation markdown 双包清理
- PR 6: hub.rs 拆子模块
- PR 7: turn_processor 净化 + SessionRuntime per-turn 下沉
- DoD: 3 份 spec + journal 完结

用户暂停要求 compact 上下文后，PR 2 起建议开新 conversation 继续，避免 context pollution。

---

## 2026-04-30 (续) · PR 2 完成：ExecutionContext 拆 SessionFrame/TurnFrame

### 分支改名

compact 后开工第一件事：把 `refactor/session-pipeline-pr1` → `refactor/session-pipeline`
（用户要求所有 PR 都在同一分支内完成，单 PR 名字太窄）。分支是本地未推的，
`git branch -m` 即可。

### PR 2 改动

**commit `76d67fc`** · 13 文件 / +190 / -152

**SPI 层（agentdash-spi/src/connector.rs）**：

- 新增 `ExecutionSessionFrame` — Who + Where 不可变视图：
  `turn_id / working_directory / environment_variables / executor_config /
   mcp_servers / vfs / identity`
- 新增 `ExecutionTurnFrame` — How + 运行时控制面 per-turn 动态：
  `hook_session / flow_capabilities / runtime_delegate / restored_session_state /
   assembled_system_prompt / assembled_tools`（派生 `Default` 以支持
   `..Default::default()` 语法）
- `ExecutionContext` 只剩两个字段：`{ session: ExecutionSessionFrame,
   turn: ExecutionTurnFrame }` → **Invariant I2 达成**
- `impl Debug for ExecutionContext` 内部访问路径更新
- `workspace_path_from_context` 读 `context.session.vfs`
- `lib.rs` re-export `ExecutionSessionFrame` / `ExecutionTurnFrame`

**hub_support.rs（session/ActiveSessionExecutionState 瘦身）**：

- 持 `session_frame: ExecutionSessionFrame` 取代原 6 个扁平字段
  （mcp_servers / vfs / working_directory / executor_config / identity / turn_id）
- `relay_mcp_server_names` + `flow_capabilities` 保留为 peer 字段
  （MCP 热更新路径需要）
- **删除 `effective_capability_keys` 字段** — E5 决策落地；field 是 `#[allow(dead_code)]`
  从未被读，req 级字段还在（prompt_pipeline 构造 `initial_caps` 用）
- 清理未用 import：`BTreeSet / HashMap / McpServer / Meta / PathBuf / AgentConfig /
  Vfs / AuthIdentity / ContentBlock`

**prompt_pipeline.rs**：

- 把 `ExecutionContext { ...13字段... }` 拆成 `session_frame` + `turn_frame`
  两次构造再合并
- `ActiveSessionExecutionState { ... }` 构造改成 `session_frame: context.session.clone()`
- 所有 `context.x` 读取改为 `context.session.x` / `context.turn.x`

**hub.rs（`replace_runtime_mcp_servers` ghost 清理）**：

- 不再手拼 13 字段 ExecutionContext 字面量
- 基于 `active.session_frame.clone()`（覆盖 `turn_id` + `mcp_servers`）+
  `ExecutionTurnFrame { hook_session, flow_capabilities, ..Default::default() }`
  重建，语义更清晰：**tool 构建只关心 session 环境 + turn.hook_session + flow_caps**，
  其他运行时字段（runtime_delegate / restored / assembled_*）都不是 tool provider 关心的
- `get_runtime_mcp_servers` 读 `active.session_frame.mcp_servers`
- 3 处 hub 测试 assert 更新：`active.session_frame.working_directory /
  executor_config / mcp_servers`；`context.session.working_directory /
  executor_config`；`context.turn.restored_session_state`
- 测试 assertion `active.effective_capability_keys == effective_keys` 删除（字段已删）

**连接器侧**：

- `pi_agent/connector.rs`：5 处 `context.x` 改路径
  （executor_config → session / assembled_tools/system_prompt/hook_session/
   runtime_delegate → turn / restored_session_state → turn / turn_id → session）
- `pi_agent/connector_tests.rs`：2 处 fixture struct-literal 改成嵌套形式
  `{ session: ExecutionSessionFrame {...}, turn: ExecutionTurnFrame::default() }`
- `vibe_kanban.rs`：6 处路径适配（assembled_system_prompt / executor_config /
  environment_variables / working_directory / turn_id）
- `composite.rs`：1 处 `context.session.executor_config.executor`
- `relay_connector.rs`：6 处访问路径 + test fixture 改成嵌套结构

**其他 application**：

- `companion/tools.rs`：`CompanionRequestTool::new` + `CompanionRespondTool::new`
  + `relative_working_dir` 共 3 个点，访问路径分流
- `workflow/tools/advance_node.rs`：`CompleteLifecycleNodeTool::new` 构造
- `vfs/tools/provider.rs`：`RelayRuntimeToolProvider::build_tools` + 多处
  `context.session.vfs / executor_config / identity`，`context.turn.flow_capabilities /
   hook_session`；`project_id_from_context` 同理

### 测试 / 构建验证

- `cargo build --workspace --lib` 绿（只剩 pre-existing `cleanup_cloned_project` dead_code）
- `cargo test --workspace --lib`：
  - application: 279 passed / 0 failed
  - executor: 53 passed / 0 failed
  - spi / domain / agent-types / api / companion / executor-composite / 全 crate 全绿
- 未跑 clippy（改动全是 struct field reshape，无新增 warning 风险；pre-existing
  warning 不受影响）

### 下一步（PR 3 起）

- **PR 3**: Bundle 进 TurnFrame + PiAgent 读 Bundle + `assembled_system_prompt`
  标 `#[deprecated]` + `update_session_context_bundle` 可选 trait 方法
- PR 4: Hook 三类语义分离 + `turn_delta` + 废 `HOOK_USER_MESSAGE_SKIP_SLOTS` +
  `SessionBootstrapAction` → `HookSnapshotReloadTrigger`
- PR 5: contribute_* 去重 + companion bundle 裁剪 + continuation markdown 双包清理
- PR 6: hub.rs 拆子模块（`hub/facade.rs` ≤ 500 行）
- PR 7: turn_processor 净化 + SessionRuntime per-turn 字段下沉
- DoD: 3 份 spec + journal 完结

当前分支状态：`refactor/session-pipeline` 领先 main 9 个 commit
（PR 1 的 8 个 + PR 2 的 `76d67fc`）。

---

## 2026-04-30 (续) · PR 3 完成：Bundle 进 TurnFrame + PiAgent 感知 bundle_id

### commit `e07798c` · 6 文件 / +209 / -14

**SPI 层**：
- `ExecutionTurnFrame.context_bundle: Option<SessionContextBundle>` 新增
  → Bundle 正式进入 connector 可感知的主数据面
- `assembled_system_prompt` 加 `#[deprecated]` 注解（note 指向 PR 8 下线计划）
- `AgentConnector::update_session_context_bundle(session_id, bundle)` trait 方法
  新增，default no-op —— 预留 application 层在非 prompt 边界（MCP 热更 / hook
  snapshot 刷新）主动推送 Bundle 的接口

**PiAgent**：
- `PiAgentSessionRuntime` 加 `last_bundle_id: Option<Uuid>` 字段
- 三种 bundle_id 变化的决策：
  - is_new_agent → `set_system_prompt` + cache `incoming_bundle_id`
  - 既有 agent + bundle_id 与 cache 不同 → `set_system_prompt` + update cache
  - 既有 agent + bundle_id 相同 → 跳过，保留上轮结果（节省 cache prefix 失效）
- text 渲染源仍来自 `turn.assembled_system_prompt`（`#[allow(deprecated)]`），
  因为 PiAgent 内不持有 base_system_prompt / user_prefs / guidelines 等
  session-level state，"full Bundle-only render" 必须 PR 8 通过
  `update_session_context_bundle` 协议由 Hub 推送完整 prompt 才能完成

**Application**：
- `prompt_pipeline.rs`：`turn_frame.context_bundle = req.context_bundle.clone()`
- `system_prompt_assembler.rs`：抽出 pub `render_runtime_section(bundle)` helper，
  负责 `## Project Context` 段落（RuntimeAgent scope / RUNTIME_AGENT_CONTEXT_SLOTS
  白名单）的结构化产出；`assemble_system_prompt` 内部调用它保持一处单源

**vibe_kanban**：保留 `assembled_system_prompt` fallback（协议侧把 SP 前置拼接到
  user_text 给外部进程），加 `#[allow(deprecated)]` + 解释性注释

### 测试验证

- 新增 `prompt_refreshes_system_prompt_when_bundle_id_changes`（executor lib）：
  - Turn 1：bundle_id=A + SP_A → set 生效
  - Turn 2：bundle_id=B + SP_B → 检测到变化，set 再次生效
  - Turn 3：bundle_id=B（复用）+ SP_STALE → **不 set**，agent 仍用 turn 2 的 SP_B
  - + 末尾 assert runtime.last_bundle_id == bundle_b_id
- 旧 fixture `update_session_tools_replaces_all_tools` 补 last_bundle_id: None
- `cargo test --workspace --lib` 全绿：application 279 / executor 32（+1）

### Invariant 进展

- **I1 · 单一主数据面** 部分达成：
  - ✅ PiAgent 读 `context_bundle.bundle_id` 作为 refresh 决策变量
  - ⏳ PiAgent 渲染 text 仍来自 `assembled_system_prompt` — PR 8 将通过
    `update_session_context_bundle` 推送 application 渲染产物彻底切走

### 下一步（PR 4 起）

- **PR 4**: Hook 三类语义分离 + Bundle `turn_delta` 字段 +
  `HOOK_USER_MESSAGE_SKIP_SLOTS` 删除 + `session-capabilities://` user_blocks
  路径废除 + `SessionBootstrapAction::OwnerContext` → `HookSnapshotReloadTrigger::Reload`
- PR 5: contribute_* 去重 + companion bundle 裁剪 + continuation markdown 双包清理
- PR 6: hub.rs 拆子模块（hub/facade.rs ≤ 500 行）
- PR 7: turn_processor 净化 + SessionRuntime per-turn 下沉
- DoD: 3 份 spec + journal 完结

当前分支状态：`refactor/session-pipeline` 领先 main 10 个 commit
（PR 1 的 8 个 + PR 2 的 2 个 + PR 3 的 `e07798c`）。

---

## 2026-04-30 (续) · PR 4 完成：Hook 三类语义分离 + Bundle turn_delta + 废除 SKIP_SLOTS

### 总览

PR 4 拆分为 4 个 commit，按"低耦合 → 高耦合"顺序推进。当前分支领先 main
14 个 commit（PR 1 的 8 + PR 2 的 2 + PR 3 的 1 + PR 4 的 4 + 先前 journal commit）。

| Commit | Hash | 主题 | 文件 / 行数 |
|---|---|---|---|
| 4a | `9dd3560` | Bundle 拆 bootstrap_fragments + turn_delta | 5 / +127 -40 |
| 4e | `e38159c` | SessionBootstrapAction → HookSnapshotReloadTrigger | 7 / +49 -38 |
| 4b+4c | `0f73b6a` | companion_agents 单路径 + 废 SKIP_SLOTS + session-caps | 5 / +56 -70 |
| 4d | `36f441f` | TransformContextOutput.messages → steering_messages | 4 / +33 -9 |

### Commit 1 (4a) — Bundle 双字段落地

**核心改动**（`agentdash-spi/src/session_context_bundle.rs`）：

- `fragments` → `bootstrap_fragments`（组装期产出，跨 turn 复用）
- 新增 `turn_delta: Vec<ContextFragment>`（per-turn 增量，运行期 Hook 回灌）
- `render_section` 合并两路（按 order 升序）
- `filter_for` / `iter_fragments` 迭代两路
- 新 API：`push_turn_delta` / `extend_turn_delta`（不去重，允许同 slot 多条）
- `upsert_by_slot` / `merge` / `push_raw` 继续作用在 bootstrap_fragments

**波及**：application 层 `context/builder.rs`（sort 改打在 bootstrap_fragments）、
`context/audit.rs`（emit 遍历 `iter_fragments`）、`session/assembler.rs` 两处
`source_summary`；executor 层 1 处 fixture struct literal 补 turn_delta。

### Commit 2 (4e) — SessionBootstrapAction 重命名

按 PRD E7 把 session 层概念重命名：

- `SessionBootstrapAction` → `HookSnapshotReloadTrigger`
- 变体 `OwnerContext` → `Reload`（去掉"owner 上下文注入"的糅合，语义纯化为
  "本轮重新 load hook snapshot + 触发 SessionStart hook"）
- `PromptSessionRequest.bootstrap_action` → `hook_snapshot_reload`
- `SessionAssemblyBuilder::with_bootstrap_action` → `with_hook_snapshot_reload`
- `SessionMeta.bootstrap_state`（Plain/Pending/Bootstrapped）保持：这是持久化
  的 session 生命周期阶段，与本轮 hook 触发器语义独立

**波及 7 文件**：application 的 types / assembler / hub / prompt_pipeline /
mod / augmenter；agentdash-api 的 acp_sessions.rs。无 serde wire 依赖（字段
只在 internal `PromptSessionRequest` 上，无 Serialize derive），安全 rename。

### Commit 3 (4b + 4c) — companion_agents 单路径

**核心命题**：静态 hook 上下文（companion_agents / workflow / constraint 等）
只有一条路径：Bundle → SP `## Project Context`。user_message 路径不再承载
静态 slot 内容。

**改动**：

1. `RUNTIME_AGENT_CONTEXT_SLOTS` 新增两个 slot：
   - `companion_agents`（原由 SP 独立 section 渲染）
   - `constraint`（单数）与 `constraints`（复数）并存避免 workflow provider
     产出的 constraint injection 在 Bundle 路径下丢失
2. `AppExecutionHookProvider` 的 `HookTrigger::UserPromptSubmit` 分支不再
   复制 `snapshot.injections` 到 `resolution.injections`，改走
   `apply_hook_rules` 只保留 per-turn 动态 rule 产出
3. `prompt_pipeline.start_prompt_with_follow_up` 签名改 `mut req`；在
   `hook_session` 初始化之后把 `From<&SessionHookSnapshot> for Contribution`
   接入 —— `snapshot.injections` → `Bundle.bootstrap_fragments`（通过
   `bundle.merge(contribution.fragments)`）
4. 删除 `prompt_pipeline.rs:388-408` 的 `session-capabilities://` resource
   block 注入代码路径；`build_user_message_notifications` 使用原
   `resolved_payload.user_blocks`
5. 删除 `HOOK_USER_MESSAGE_SKIP_SLOTS` 常量及其 filter 分支；
   `build_hook_injection_message` 全量交给 `build_hook_markdown`
6. 删除 `system_prompt_assembler.rs` 的 `## Companion Agents` 独立 section；
   `SessionBaselineCapabilities.companion_agents` 结构保留（companion 工具
   参数校验仍可能依赖），只是不再二次渲染进 SP

**Invariant I4 验证**：
- `grep -r "HOOK_USER_MESSAGE_SKIP_SLOTS" crates/` 代码路径零命中（仅剩 doc
  注释的历史引用）
- `grep -r "session-capabilities://" crates/` 代码路径零命中（同上）
- companion_agents 在代码库中只有两条产出路径（hook provider `build_*` /
  Bundle render），且它们是同一数据流的上下游关系

### Commit 4 (4d) — TransformContextOutput 语义命名

**改动**：`TransformContextOutput.messages` → `steering_messages`，类型
doc 上明确三轴语义独立（与 target-architecture.md §C11 对齐）：

1. **Bundle 改写** — 不通过此结构体承载，走 `Bundle.turn_delta` +
   `ContextAuditBus` 两条独立数据面
2. **Per-turn steering** — `steering_messages`（命名本身宣告禁令：不得塞
   已被 Bundle 承载的静态 slot）
3. **控制决策** — `blocked: Option<String>`（未来可演化为枚举）

**为什么不加 `bundle_delta: Vec<ContextFragment>` 字段（PRD 原版期望）**：
`ContextFragment` 位于 `agentdash-spi`，而 `spi` 依赖 `agent-types`。把 SPI
类型塞入 `agent-types` 会形成循环依赖。处理方式：
- **字段层面**：仅 rename `messages` → `steering_messages`，以命名声明
  三轴独立性
- **数据面层面**：物理分离已达成（Bundle.turn_delta + ContextAuditBus），
  PR 4a 落地了 Bundle.turn_delta；hook_delegate 在 emit_hook_injection_fragments
  内已把 HookInjection 通过 audit_bus emit 为 ContextFragment（PR 3 已完成
  audit 路径）

后续 PR 5+ 可在 ContextFragment 迁入 agent-types（或引入共享 shim 类型）
后把字段形式补齐；当前实现的"数据面三轴分离 + 命名层声明"已满足 I4
不变式的语义级约束。

**波及**：`agent-types/decisions.rs` 1 处 + `agent/agent_loop.rs` 1 处 +
`runtime_alignment.rs` 2 处 fixture + `hook_delegate.rs` 3 处字段构造与
断言。

### 测试 / 构建验证

- `cargo build --workspace --lib` ✅
- `cargo test --workspace --lib`：application 279 / executor 32 / SPI 37 /
  全 crate 全绿
- `cargo test --workspace`：含 integration 全绿（agent 11 passed）
- `HOOK_USER_MESSAGE_SKIP_SLOTS` grep 代码路径零命中 ✅
- `session-capabilities://` grep 代码路径零命中 ✅
- `SessionBootstrapAction` grep 只剩 types.rs doc 注释的历史引用 ✅

### Invariant 清单（截至 PR 4 完成）

| Inv | 状态 | 来源 |
|---|---|---|
| I1 单一主数据面 | 🟡 PiAgent 读 Bundle，渲染 text 仍 fallback `assembled_system_prompt`（PR 8） | PR 3 |
| I2 ExecutionContext 分层 | ✅ | PR 2 |
| I3 入口单一节拍 | ✅ | PR 1 |
| I4 Hook 语义分离 | ✅ （SKIP_SLOTS 零命中 / session-caps 零命中 / 三轴语义独立） | **PR 4** |
| I5 SessionRuntime 纯 session 级 | ⏳ PR 7 |  |
| I6 turn_processor 职责单一 | ⏳ PR 7 |  |
| I7 hub.rs ≤ 500 行 | ⏳ PR 6 |  |
| I8 contribute_* 单源 | ⏳ PR 5 |  |
| I9 slot order 集中 | ⏳ PR 5 |  |
| I10 Routine identity 非 None | ✅ | PR 1 |

### 下一步（PR 5 起）

- **PR 5**: contribute_* 去重 + workflow_injection 共享 helper + workspace
  单源 + SessionPlan 统一外挂 + source_resolver/workspace_sources 合并 +
  slot_orders.rs 集中 + companion bundle 裁剪 + continuation markdown 双包
  清理
- PR 6: hub.rs 拆 facade/factory/tool_builder/hook_dispatch/cancel 子模块
- PR 7: turn_processor 净化 + SessionRuntime per-turn 字段下沉到 TurnExecution
- DoD: `.trellis/spec/backend/` 三份 spec（session-startup-pipeline.md /
  execution-context-frames.md / bundle-main-datasource.md）+ journal 完结

当前分支状态：`refactor/session-pipeline` 领先 main 14 个 commit（PR 1 的
8 + PR 2 的 2 + PR 3 的 2 + PR 4 的 4）。
