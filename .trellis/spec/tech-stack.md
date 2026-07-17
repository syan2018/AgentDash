# 技术选型

> 已确定的技术栈与版本。

---

## 概览

| 层级 | 技术 | 说明 |
|------|------|------|
| 后端 | Rust + Axum + Tokio + SQLx | 云端 + 本机双 binary |
| 数据库 | PostgreSQL（embedded）+ SQLite | 云端业务数据 / 本机会话缓存 |
| 前端 | React 19 + TypeScript 5.9 | 组件化、类型安全 |
| 构建 | Vite 7 | HMR |
| 样式 | Tailwind CSS v4 | 主题变量驱动 |
| 状态管理 | Zustand 5 | 轻量 |
| 路由 | React Router 7 | SPA 路由 |
| 测试 | Vitest 4 | 前端测试 |
| 协议 | REST + NDJSON + WebSocket | 前端↔云端 / 云端↔本机 |
| 内部事件流 | Backbone Protocol | `BackboneEnvelope` / `BackboneEvent` |
| 对外能力 | MCP | Agent 工具暴露 |

---

## 后端

- **Rust**：编译期安全检查
- **Axum**：基于 Tower，中间件生态丰富
- **SQLx**：编译期 SQL 检查，无需代码生成
- **PostgreSQL**：云端业务数据，`postgresql_embedded` crate，开发期零运维；上线后可切换为外部实例
- **SQLite**：仅本机会话缓存（`agentdash-local`）

---

## 前端

- **React 19**（`^19.2.0`）+ **TypeScript**（`~5.9.3`）
- **Vite 7**（`^7.3.1`）+ **Tailwind CSS v4**（`^4.2.1`）
- **Zustand 5**（`^5.0.11`）+ **React Router 7**（`react-router-dom ^7.13.1`）
- **ACP SDK**（`@agentclientprotocol/sdk ^0.14.1`）：仅 relay adapter 使用，前端主路径消费 `generated/backbone-protocol.ts`
- **Vitest 4**（`^4.0.18`）、**@dnd-kit**（拖拽）、**react-markdown + remark-gfm**
- UI 组件自研，参考 shadcn/ui 模式（组件代码放入 `components/ui/` 或 `@agentdash/ui` 包）

---

## 前后端通信

- **前端↔云端**：REST（业务 CRUD）+ NDJSON（实时推送），会话流用 `fetch + ReadableStream`
- **增量恢复**：Project 流和会话流统一使用 `x-stream-since-id`
- **云端↔本机**：WebSocket（本机主动连接），JSON over WebSocket
- **Agent执行协议**：Agent Runtime Contract + RuntimeWire；Backbone只承载产品/资源presentation事件
- **对外能力**：MCP
- **DTO 格式**：JSON / NDJSON，统一 `snake_case`

---

## Crate 结构

```
crates/
├── agentdash-agent-protocol/      # Backbone Protocol 事件流定义 + 外部协议 adapter
├── agentdash-domain/              # 领域层（实体、值对象、Repository trait）
├── agentdash-application/         # 应用层（用例编排、hooks、context、VFS）
├── agentdash-infrastructure/      # 基础设施层（PostgreSQL + SQLite 实现）
├── agentdash-api/                 # 接口层（HTTP 路由、DTO、中间件）
├── agentdash-executor/            # 执行器（连接器、hook runtime）
├── agentdash-spi/                 # SPI（Connector / Hook trait）
├── agentdash-agent/               # Agent Loop 引擎
├── agentdash-agent-types/         # Agent 领域通用类型
├── agentdash-mcp/                 # MCP Server 实现
├── agentdash-relay/               # WebSocket Relay 协议
├── agentdash-local/               # 本机后端
├── agentdash-local-tauri/         # Tauri 桌面端封装
├── agentdash-plugin-api/          # 插件 API 契约
└── agentdash-first-party-plugins/ # 内置插件
```
