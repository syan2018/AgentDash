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

