# 技术选型

> AgentDashboard 技术栈选型。本节只记录已确定的决策，未定事项留待后续讨论。

---

## 概览

| 层级 | 技术 | 说明 |
|------|------|------|
| 后端 | Rust | 性能、可靠性、与 vibe-kanban 生态兼容 |
| 中控 | SQLite or Supabase | 存储连接凭证、全局视图、用户偏好 |
| 前端 | React + TypeScript | 组件化、类型安全、生态丰富 |
| 数据库 | SQLite | 后端（Backend）本地优先，简单可靠 |

---

## 后端

### 已确定

- **语言：Rust**
  - 与 vibe-kanban 代码复用兼容
  - 编译期安全检查适合长期维护

- **Web 框架：Axum**
  - 生态主流，与 vibe-kanban 一致
  - 基于 Tower，中间件生态丰富

- **异步运行时：Tokio**
  - Rust 异步生态标准

- **ORM：SQLx**
  - 编译期 SQL 检查
  - 无需代码生成，简化构建流程

- **数据库：SQLite (Backend) / 预留 BaaS 接口 (Coordinator)**
  - 本地 Backend 首选 SQLite，单文件便于备份
  - Coordinator 可选 Supabase (BaaS) 实现多端同步，或本地并行部署

### 待定 / 预留讨论空间

| 事项 | 当前状态 | 待决策内容 |
|------|---------|-----------|
| 远程数据库存储 | 预留接口 | PostgreSQL / D1 / 其他，待验证需求后再定 |
| 缓存层 | 无 | 是否需要 Redis 等缓存，视性能测试结果 |
| 消息队列 | 无 | 编排层任务调度是否需要独立队列 |

---

## 前端

### 已确定

- **框架：React 18+**
  - 组件化开发
  - 生态成熟

- **语言：TypeScript**
  - 与后端 API 类型对齐

- **构建工具：Vite**
  - 开发体验好
  - 生态主流

- **状态管理：待定**
  - 预留 Zustand / Jotai / Redux Toolkit 的选择空间
  - 根据实际复杂度再定

### 待定 / 预留讨论空间

| 事项 | 当前状态 | 待决策内容 |
|------|---------|-----------|
| UI 组件库 | 待定 | 复用 vibe-kanban 组件 / Radix / shadcn / 自研 |
| 样式方案 | 待定 | CSS Modules / Tailwind / styled-components |
| 路由方案 | 待定 | React Router / TanStack Router |

---

## 前后端通信

### 已确定

- **协议：HTTP/2 + RPC (JSON-RPC)**
  - RPC 负责指令交互与状态恢复（Resume）
  - 与流式响应配合良好

- **流式状态推送：Streaming HTTP (NDJSON)**
  - 配合 `cursor/since_id` 实现增量重连
  - 替代 SSE，更灵活
  - fetch API + ReadableStream

- **交换标准：Agent Client Protocol / MCP**
  - 统一 Artifacts 和 Task 状态的语义结构

- **数据格式：JSON / NDJSON**
  - 简单、易调试

### 待定 / 预留讨论空间

| 事项 | 当前状态 | 待决策内容 |
|------|---------|-----------|
| 认证方案 | 预留接口 | Bearer Token / JWT / Session，待需求明确 |
| 路由中间层 | 预留架构 | 企业部署场景下需要用户路由层，而非前端直连后端 |
| API 版本策略 | 无 | 是否需要版本控制 |

---

## 与 vibe-kanban 的关系

### 复用策略

vibe-kanban 作为 git submodule 引入到 `third_party/` 目录，复用以下模块：

- **Workspace 管理** - Git worktree 创建、锁定、清理
- **Execution 管理** - Agent 进程生命周期
- **部分 UI 组件** - 任务执行面板、日志流等

### 扩展点

AgentDashboard 在 vibe-kanban 基础上新增：

- **Story/Task 双层模型** - 超越扁平任务结构
- **编排引擎** - 任务依赖和流程控制
- **中控层 (Coordinator)** - 多后端连接与全局看板视图
- **信息注入器** - 上下文管理和传递
- **验证层** - 可插拔的结果验证

---

## 模块结构（暂定）

```
crates/
├── agentdash-state/         # [新增] Story/Task 状态管理
├── agentdash-orchestration/ # [新增] 编排引擎
├── agentdash-coordinator/   # [新增] 用户中控（连接/视图）
├── agentdash-injection/     # [新增] 信息注入
└── agentdash-validation/    # [新增] 结果验证

third_party/
└── vibe-kanban/             # [submodule] 基础能力库

frontend/
├── src/
│   ├── components/          # UI 组件
│   ├── hooks/               # 业务逻辑封装
│   ├── stores/              # 状态管理
│   └── views/               # 页面视图
```

> 注：crate 命名和划分在开发过程中可能调整。

---

## 决策记录

| 日期 | 决策 | 理由 | 决策者 |
|------|------|------|--------|
| 2026-02-25 | 技术栈选定为 Rust + React | 与 vibe-kanban 兼容，团队熟悉 | - |
| 2026-02-25 | 数据库先用 SQLite | 本地优先，简单可靠 | - |
| 2026-02-25 | 通信用 RPC + NDJSON | 满足状态恢复与实时推送需求 | - |
| 2026-02-25 | 引入 Coordinator 层 | 解决多后端看板同步与用户偏好存储 | - |

---

## 待决策事项清单

以下事项在开发过程中逐步决定：

1. **远程数据库存储方案** - 需要验证多后端场景下的数据同步需求
2. **路由中间层设计** - 企业部署场景下的用户路由 and 鉴权
3. **前端状态管理库** - 根据实际交互复杂度选择
4. **UI 组件库策略** - 复用 vs 自研的边界
5. **认证方案** - 本地简化版 vs 企业版的需求差异

---

*更新：2026-02-25 - 已根据 Coordinator & Protocol 设想同步更新*
