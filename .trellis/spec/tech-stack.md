# 技术选型

> AgentDashboard 技术栈选型。本节记录已确定的决策和当前使用的具体版本。

---

## 概览

| 层级 | 技术 | 说明 |
|------|------|------|
| 后端 | Rust + Axum + Tokio + SQLx | 云端 + 本机双 binary |
| 数据库 | PostgreSQL（embedded）+ SQLite | 云端：嵌入式 PostgreSQL；本机：SQLite 仅限会话缓存 |
| 前端 | React 19 + TypeScript 5.9 | 组件化、类型安全 |
| 构建 | Vite 7 | 开发体验好，HMR 快 |
| 样式 | Tailwind CSS v4 | 主题变量驱动 |
| 状态管理 | Zustand 5 | 轻量、简洁 |
| 路由 | React Router 7 | 前端 SPA 路由 |
| 协议 | REST + SSE/NDJSON + WebSocket | 前端↔云端 REST/SSE；云端↔本机 WebSocket |

---

## 后端

### 已确定

- **语言：Rust**
  - 编译期安全检查适合长期维护

- **Web 框架：Axum**
  - 基于 Tower，中间件生态丰富

- **异步运行时：Tokio**
  - Rust 异步生态标准

- **ORM：SQLx**
  - 编译期 SQL 检查
  - 无需代码生成，简化构建流程

- **数据库：PostgreSQL（embedded）+ SQLite**
  - 云端业务数据：嵌入式 PostgreSQL（`postgresql_embedded` crate，开发期零运维）
  - 本机会话缓存：SQLite（仅 `agentdash-local` 使用）
  - 所有 migration 位于 `agentdash-infrastructure/migrations/`

### 待定 / 预留讨论空间

| 事项 | 当前状态 | 待决策内容 |
|------|---------|-----------|
| 生产级 PostgreSQL | 嵌入式 | 上线后可切换为外部 PostgreSQL 实例 |
| 缓存层 | 无 | 是否需要 Redis 等缓存，视性能测试结果 |
| 消息队列 | 无 | 编排层任务调度是否需要独立队列 |

---

## 前端

### 已确定

- **框架：React 19**（`^19.2.0`）
- **语言：TypeScript**（`~5.9.3`）
- **构建工具：Vite 7**（`^7.3.1`）
- **样式方案：Tailwind CSS v4**（`^4.2.1`）
- **状态管理：Zustand 5**（`^5.0.11`）
- **路由：React Router 7**（`react-router-dom ^7.13.1`）
- **ACP SDK**：`@agentclientprotocol/sdk ^0.14.1`
- **测试：Vitest 4**（`^4.0.18`）
- **拖拽：@dnd-kit**（core + sortable）
- **Markdown 渲染：react-markdown + remark-gfm**

### UI 组件策略

当前使用自研组件，参考 shadcn/ui 模式（直接将组件代码放入 `components/ui/`），不依赖外部组件库 NPM 包。

---

## 前后端通信

### 已确定

- **前端↔云端：REST + SSE/NDJSON**
  - 业务 CRUD 使用 REST API
  - 实时状态推送使用 NDJSON（首选）+ SSE（降级）
  - 会话流使用 `fetch + ReadableStream` 消费 NDJSON
  - 增量恢复：全局流 `Last-Event-ID`，会话流 `x-stream-since-id`

- **云端↔本机：WebSocket**
  - 本机主动连接云端
  - JSON over WebSocket
  - 详见 `docs/relay-protocol.md`

- **交换标准：Agent Client Protocol (ACP) + MCP**
  - 统一 Artifacts 和 Task 状态的语义结构

- **数据格式：JSON / NDJSON**
  - 业务 HTTP DTO 统一使用 `snake_case`

### 待定 / 预留讨论空间

| 事项 | 当前状态 | 待决策内容 |
|------|---------|-----------|
| 认证方案 | 预留接口 | Bearer Token / JWT / Session，待需求明确 |
| 路由中间层 | 预留架构 | 企业部署场景下需要用户路由层 |
| API 版本策略 | 无 | 是否需要版本控制 |

---

## Crate 结构

```
crates/
├── agentdash-agent-types/       # Agent 领域通用类型（零 runtime 核心）
├── agentdash-agent/             # Agent Loop 引擎（纯 loop + bridge trait）
├── agentdash-domain/            # 领域层（实体、值对象、Repository trait）
├── agentdash-application/       # 应用层（用例编排、hooks、context、address space）
├── agentdash-infrastructure/    # 基础设施层（PostgreSQL + SQLite Repository 实现）
├── agentdash-executor/          # 执行器（连接器、RigBridge、hook runtime）
├── agentdash-spi/               # SPI（re-export agent-types + Connector/Hook trait）
├── agentdash-api/               # 接口层（HTTP 路由、DTO、中间件）
├── agentdash-injection/         # 上下文注入（声明式来源解析）
├── agentdash-mcp/               # MCP Server 实现
├── agentdash-relay/             # WebSocket Relay 协议
├── agentdash-acp-meta/          # ACP 元数据 TypeScript 绑定
├── agentdash-local/             # 本机后端执行器
├── agentdash-plugin-api/        # 插件 API 契约
└── agentdash-first-party-plugins/ # 内置插件
```

---

## 决策记录

| 日期 | 决策 | 理由 |
|------|------|------|
| 2026-02-25 | 技术栈选定为 Rust + React | 类型安全、编译期检查 |
| 2026-02-25 | 数据库先用 SQLite | 本地优先，简单可靠 |
| 2026-04-01 | 云端切换到嵌入式 PostgreSQL | 支持完整 SQL 能力 + 多表关系 |
| 2026-02-25 | 通信用 REST + NDJSON | 满足状态恢复与实时推送需求 |
| 2026-02-26 | 前端选定 React 19 + Zustand + Tailwind v4 | 生态成熟，团队熟悉 |
| 2026-02-26 | 前端路由选定 React Router | SPA 路由标准方案 |
| 2026-03-10 | 架构演进为云端/本机双后端 | 支持多设备、PiAgent 云端原生 |

---

*更新：2026-04-14 — 修正数据库为 PostgreSQL（embedded）+ SQLite，对齐实际技术栈*
*更新：2026-03-29 — 对齐项目实际技术栈和 crate 结构*
