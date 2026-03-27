# AgentDash

> 统一看板系统，管理多设备、多项目中 AI Agent 的协同工作。

AgentDash 是一个面向开发者的 **AI Agent 编排与管理平台**。它采用云端 + 本机双后端架构，将分散在不同机器、不同 IDE 中运行的 AI Agent（如 Claude Code、Codex、Cursor Agent 等）整合到统一的看板视图中，提供从需求规划（Story）到任务执行（Task）再到结果验收的完整工作流管理能力。

## 核心特性

- **多 Agent 统一管理** — 在同一个看板中管理 Claude Code、Codex、Gemini、Cursor Agent 等多种 AI Agent，内置 Pi Agent 作为云端原生执行器
- **Story → Task 工作流** — 以用户价值为中心的 Story 模型，支持上下文注入、任务拆解、Agent 执行、产物验收的完整闭环
- **云端 / 本机双后端** — 云端负责数据持久化与调度编排，本机后端通过 WebSocket 注册并执行 Agent 任务，天然穿透 NAT/防火墙
- **Address Space 统一寻址** — 跨设备、跨项目的文件系统虚拟化，Agent 通过挂载策略访问工作空间
- **Hook Runtime** — 可扩展的生命周期钩子系统，在 Agent 执行的关键节点注入自定义逻辑（Workflow / Lifecycle）
- **ACP 协议集成** — 基于 [Agent Client Protocol](https://github.com/anthropics/agent-client-protocol) 标准，实现与不同 Agent 的统一通信
- **MCP 工具注入** — 通过 [Model Context Protocol](https://spec.modelcontextprotocol.io/) 向 Agent 会话动态注入外部工具

## 架构概览

```
┌─────────────────────────────────────────────────┐
│              Frontend (React + Vite)             │
│  Dashboard · Story · Session · Workflow Editor   │
└───────────────────────┬─────────────────────────┘
                        │ REST + SSE
┌───────────────────────┴─────────────────────────┐
│              Cloud Backend (Rust/Axum)           │
│  API · Relay · Orchestration · State · Plugins   │
└───────────────────────┬─────────────────────────┘
                        │ WebSocket
          ┌─────────────┼─────────────┐
          ▼             ▼             ▼
   ┌────────────┐ ┌────────────┐ ┌────────────┐
   │ Local Dev A│ │ Local Dev B│ │ Local Dev C│
   │ Executors  │ │ Executors  │ │ Executors  │
   └────────────┘ └────────────┘ └────────────┘
```

## 技术栈

| 层 | 技术 |
|---|---|
| **后端** | Rust 2024, Axum 0.8, SQLx (SQLite), Tokio, serde |
| **前端** | React 19, TypeScript 5.9, Vite 7, Tailwind CSS 4, Zustand 5 |
| **协议** | ACP (Agent Client Protocol), MCP (Model Context Protocol) |
| **测试** | Vitest (单元), Playwright (E2E), cargo test |
| **工具链** | pnpm, Cargo workspace, ESLint 9, clippy |

## 项目结构

```
AgentDash/
├── crates/                          # Rust 后端 workspace
│   ├── agentdash-domain/            #   领域模型（Story, Task, Agent, Session 等）
│   ├── agentdash-application/       #   应用服务（上下文构建, Hook 运行时, 任务编排）
│   ├── agentdash-api/               #   HTTP API 层（Axum 路由, SSE 推送, 认证）
│   ├── agentdash-executor/          #   执行器 Hub（连接各类 Agent Connector）
│   ├── agentdash-connector-contract/#   Connector SPI 契约
│   ├── agentdash-relay/             #   WebSocket 中继协议（Cloud ↔ Local）
│   ├── agentdash-local/             #   本机后端（WS 客户端, 工具执行）
│   ├── agentdash-agent/             #   Pi Agent 内置 Agent 实现
│   ├── agentdash-infrastructure/    #   基础设施（数据库 Repo 实现）
│   ├── agentdash-injection/         #   依赖注入 / Address Space
│   ├── agentdash-acp-meta/          #   ACP 元数据解析
│   ├── agentdash-mcp/               #   MCP 协议支持
│   ├── agentdash-plugin-api/        #   插件 API 定义
│   └── agentdash-first-party-plugins/#  内置插件
├── frontend/                        # React 前端
│   └── src/
│       ├── pages/                   #   页面组件
│       ├── features/                #   功能模块（按领域划分）
│       ├── components/              #   通用 UI 组件
│       ├── stores/                  #   Zustand 状态管理
│       ├── hooks/                   #   自定义 Hooks
│       ├── services/                #   API 客户端
│       └── types/                   #   TypeScript 类型定义
├── tests/e2e/                       # Playwright E2E 测试
├── scripts/                         # 开发脚本（启动, 端口管理等）
├── docs/                            # 设计文档
└── .trellis/                        # AI 任务管理（Trellis 工作流）
    └── spec/                        #   项目开发规范
```

## 快速开始

### 前置条件

- **Rust** (edition 2024, 推荐 nightly 或最新 stable)
- **Node.js** ≥ 20
- **pnpm** ≥ 10
- **SQLite** (通过 sqlx 自动管理)

### 安装依赖

```bash
# 前端依赖
pnpm install

# Rust 依赖由 cargo 自动管理
```

### 启动开发环境

```bash
# 一键启动全部服务（推荐）
pnpm run dev

# 启动后自动打开：
#   API:      http://127.0.0.1:3001
#   Frontend: http://127.0.0.1:5380
```

`pnpm run dev` 会依次执行：
1. 清理遗留端口
2. `cargo build` 编译后端
3. 启动 `agentdash-server`（云端后端 :3001）
4. 启动 `agentdash-local`（本机后端，自动 WS 注册）
5. 启动 Vite 前端开发服务器 (:5380)

### 其他启动模式

```bash
pnpm run dev:mono          # 仅云端 + 前端（跳过本机后端）
pnpm run dev:frontend      # 仅前端
pnpm run dev:backend       # 仅后端
```

## 质量门禁

```bash
# 完整检查（CI 级别）
pnpm run check

# 分项检查
pnpm run backend:check     # cargo check
pnpm run backend:clippy    # cargo clippy -D warnings
pnpm run backend:test      # cargo test --workspace
pnpm run frontend:check    # tsc --noEmit
pnpm run frontend:lint     # eslint
pnpm run frontend:test     # vitest
pnpm run e2e:test:critical # playwright (关键路径)
```

## 核心概念

### Project
顶层组织单元。一个 Project 关联多个 Agent 和 Story，拥有独立的配置（默认执行器、MCP 工具等）。

### Agent
AI 执行体的抽象。每个 Agent 绑定一种执行器类型（如 `PI_AGENT`、`CLAUDE_CODE`、`CODEX`），通过 Project Agent Link 关联到项目。

### Story
用户价值单元。承载需求描述、上下文资源、验收标准。可拆解为多个 Task 交由不同 Agent 执行。

### Task
最小执行单元。绑定到具体 Agent 实例，通过 Session 驱动 Agent 完成工作。

### Session
Agent 的一次完整对话会话。支持 Prompt 发送、工具调用、流式输出。Session 通过 ACP 协议与 Agent 通信。

### Workflow / Lifecycle
可扩展的自动化钩子。Lifecycle 定义 Agent 全局行为模板，Workflow 定义特定业务流程中的自动化逻辑。

## 支持的执行器

| 执行器 | 类型 | 运行位置 |
|---|---|---|
| Pi Agent | 云端原生 | Cloud Backend |
| Claude Code | 第三方 | Local Backend |
| Codex (OpenAI) | 第三方 | Local Backend |
| Cursor Agent | 第三方 | Local Backend |
| Gemini CLI | 第三方 | Local Backend |
| AMP | 第三方 | Local Backend |
| Opencode | 第三方 | Local Backend |

## 开发规范

项目维护了完整的开发规范文档，位于 `.trellis/spec/`：

- **前端**: 目录结构、组件规范、类型安全、质量标准
- **后端**: 分层架构、错误处理、Hook Runtime 设计
- **协作**: 中文沟通、代码注释规范、Git 提交规范

## License

MIT
