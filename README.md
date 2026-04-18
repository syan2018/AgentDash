# AgentDash

> 统一看板系统，管理多设备、多项目中 AI Agent 的协同工作。

AgentDash 是一个面向开发者的 **AI Agent 编排与管理平台**。它采用云端 + 本机双后端架构，将分散在不同机器、不同 IDE 中运行的 AI Agent（如 Claude Code、Codex、Cursor Agent 等）整合到统一的看板视图中，提供从需求规划（Story）到任务执行（Task）再到结果验收的完整工作流管理能力。

## 核心特性

- **多 Agent 统一管理** — 在同一个看板中管理 Claude Code、Codex、Gemini CLI、Cursor Agent 等多种 AI Agent，内置 Pi Agent 作为云端原生标准执行器
- **Story → Task 工作流** — 以用户价值为中心的 Story 模型，支持上下文注入、任务拆解、Agent 执行、产物验收的完整闭环
- **云端 / 本机双后端** — 云端负责数据持久化与调度编排，本机后端通过 WebSocket 注册并执行 Agent 任务，天然穿透 NAT/防火墙
- **VFS 统一文件系统** — 基于挂载点（Mount）+ Provider 的虚拟文件系统，统一跨设备、跨 Provider 的 Read/Write/List/Search/Exec/Watch 操作
- **Lifecycle DAG 编排** — 支持多步骤有向无环图的 Agent 生命周期编排，节点可并行执行，通过端口级依赖自动推进
- **Hook Runtime** — 可扩展的生命周期钩子系统，在 Agent 执行的关键节点注入自定义逻辑（Workflow / Lifecycle / Constraint）
- **Canvas 系统** — Project 级可运行前端资产，Agent 可创建和绑定 Canvas 数据，实现交互式可视化产出
- **Routine 自动触发** — 支持 Cron 定时、Webhook、插件事件源三种方式自动触发 Agent 执行
- **ACP 协议集成** — 基于 [Agent Client Protocol](https://github.com/anthropics/agent-client-protocol) 标准，实现与不同 Agent 的统一通信
- **MCP 工具注入** — 通过 [Model Context Protocol](https://spec.modelcontextprotocol.io/) 向 Agent 会话动态注入外部工具

## 核心设计假设

以下是项目的关键架构决策和长期方向：

### 1. Pi Agent 是标准实现

Pi Agent 是 AgentDash 的云端原生执行器，也是整套 Agent Runtime 的**标准参考实现**。它在进程内运行完整的 Agent Loop（LlmBridge → Tool Registry → Compaction → Runtime Delegate），拥有最完整的生命周期钩子集成（Hook Runtime、Workflow 注入、Companion 协作）。第三方执行器（Claude Code、Codex 等）当前通过 [vibe-kanban](https://github.com/BloopAI/vibe-kanban) 的 `executors` crate 驱动子进程执行，未来将根据各方 SDK 成熟度逐步迁移到原生 ACP 适配。

### 2. VFS — 统一虚拟文件系统

所有 Agent 对文件的操作都通过 VFS 层路由，而非直接访问物理文件系统。VFS 由多个 `MountProvider` 组成：

| Provider | 用途 |
|---|---|
| `relay_fs` | 通过 WebSocket 中继访问本机后端的真实文件系统 |
| `inline_fs` | Context Container 中的内联文件（持久化到数据库） |
| `lifecycle_vfs` | Lifecycle Step 间的数据传递（Input/Output Port） |
| `canvas_fs` | Canvas 资产文件管理 |
| 插件 Provider | 企业插件可实现自定义 Provider（如知识库桥接） |

每个 Mount 声明自己的能力集（Read/Write/List/Search/Exec/Watch），Agent Tool 层按能力做裁剪。MountLink 提供声明式跨 Mount 引用（类 symlink）。

### 3. 双后端中继架构

云端后端（agentdash-server）持有全局状态和编排逻辑，通过 REST + SSE 服务前端。本机后端（agentdash-local）是部署在开发者机器上的轻量进程，通过 WebSocket 主动连接云端，注册自身能力（可用执行器、MCP Server、可访问目录）。这种设计确保：

- 本机后端不需要公网 IP 或端口映射
- 多台开发机可同时注册，按能力路由任务
- Pi Agent 的工具调用（file_read / shell_exec 等）通过 Relay 协议委托到本机执行

### 4. Session 与执行模型

Session 是 Agent 一次完整对话的运行时抽象。通过 `SessionBinding` 将 Session 与业务实体（Project / Story / Task）解耦关联。Session 支持：

- 流式 SSE 推送（ACP SessionNotification）
- 冷启动恢复（从仓储重建消息历史）
- Companion Agent 协作（Session 内 spawn 子 Session）
- Hook 注入（上下文注入 / 约束注入 / Workflow Step 绑定）

### 5. 嵌入式 PostgreSQL → 可替换外部数据库

当前使用 `postgresql_embedded` 做零配置启动（适合原型开发），未来将迁移到外部 PostgreSQL 以支持多用户 / 团队部署。

## 架构概览

```
┌─────────────────────────────────────────────────┐
│              Frontend (React + Vite)             │
│  Dashboard · Story · Session · Canvas · Editor  │
└───────────────────────┬─────────────────────────┘
                        │ REST + SSE
┌───────────────────────┴─────────────────────────┐
│              Cloud Backend (Rust/Axum)           │
│ API · Relay Hub · SessionHub · Hook Runtime     │
│ VFS (MountProviderRegistry) · Plugin System     │
│ Pi Agent (in-process LLM → Tool → Compaction)   │
└─────────┬─────────────────┬─────────────────────┘
          │ WebSocket        │ LLM API (OpenAI/Anthropic/...)
          │ (Relay Protocol) │
┌─────────┴─────────┐       │
│  Local Backend A   │       │
│  ┌──────────────┐  │       │
│  │ Tool Executor│──│───────┘
│  │ (fs/shell)   │  │
│  ├──────────────┤  │
│  │ SessionHub   │  │  (本机可选：直接运行第三方 Agent)
│  │ Connectors   │  │
│  │ (vibe-kanban)│  │
│  ├──────────────┤  │
│  │ MCP Client   │  │  (本机 MCP Server relay 到云端)
│  └──────────────┘  │
└────────────────────┘
```

## 领域模型

```
Project
 ├── Workspace[]           逻辑工作空间（可绑定多个 backend/物理目录）
 ├── Agent[]               通过 ProjectAgentLink 关联，支持 per-project 配置覆写
 ├── Story[]               用户价值单元
 │    ├── Task[]           最小执行单元，绑定 Agent 和 Workspace
 │    └── Context          结构化设计上下文（Acceptance Criteria / Design Decisions）
 ├── Workflow[]            单 Session 级行为契约（注入/约束/完成条件）
 ├── Lifecycle[]           多 Step DAG 编排（Step → Workflow 映射）
 ├── Canvas[]              可运行前端资产（React sandbox + 数据绑定）
 ├── Routine[]             自动触发规则（Cron / Webhook / Plugin Event）
 └── ContextContainer[]    可复用上下文资源（inline 文件 / 外部服务挂载）
```

**Session 归属** 通过 `SessionBinding` 独立管理（owner_type + owner_id + label），支持一个实体拥有多种角色的 Session（execution / companion / review）。

## 技术栈

| 层 | 技术 |
|---|---|
| **后端** | Rust 2024, Axum 0.8, SQLx (PostgreSQL Embedded + SQLite), Tokio, serde |
| **Agent 引擎** | agentdash-agent (自研 Agent Loop), Rig 0.34 (LLM SDK), Rhai (脚本引擎) |
| **前端** | React 19, TypeScript 5.9, Vite 7, Tailwind CSS 4, Zustand 5 |
| **协议** | ACP (Agent Client Protocol), MCP (Model Context Protocol), 自研 Relay Protocol (WebSocket) |
| **第三方 Agent** | vibe-kanban executors (Claude Code / Codex / AMP / Gemini CLI / Opencode) |
| **测试** | Vitest (单元), Playwright (E2E), cargo test |
| **工具链** | pnpm 10, Cargo workspace, ESLint 9, clippy |

## 项目结构

```
AgentDash/
├── crates/                              # Rust 后端 workspace
│   ├── agentdash-domain/                #   领域模型（62 文件：实体/值对象/仓储 trait）
│   ├── agentdash-spi/                   #   跨层 SPI 契约（Connector / Hook / MountProvider / Lifecycle）
│   ├── agentdash-agent-types/           #   Agent 领域通用类型（Message / Tool / Delegate / Compaction）
│   ├── agentdash-agent/                 #   Pi Agent 引擎（Agent Loop + LlmBridge trait + ToolRegistry）
│   ├── agentdash-application/           #   应用服务（VFS / SessionHub / Hook Runtime / Context 构建 / 编排）
│   ├── agentdash-executor/              #   执行器 Hub（CompositeConnector 路由 + Pi/VibeKanban 连接器）
│   ├── agentdash-api/                   #   HTTP API 层（Axum 路由 / SSE / 认证 / 插件加载 / Bootstrap）
│   ├── agentdash-relay/                 #   WebSocket 中继协议（消息类型定义 + 序列化）
│   ├── agentdash-local/                 #   本机后端（WS 客户端 / 工具执行 / MCP 管理 / 可选 SessionHub）
│   ├── agentdash-infrastructure/        #   基础设施（PostgreSQL + SQLite 仓储实现 / 迁移）
│   ├── agentdash-acp-meta/              #   ACP 元数据解析（AgentDash 扩展事件/溯源标注）
│   ├── agentdash-mcp/                   #   MCP 协议支持
│   ├── agentdash-plugin-api/            #   插件 API 定义（AgentDashPlugin trait）
│   └── agentdash-first-party-plugins/   #   内置插件
├── frontend/                            # React 前端
│   └── src/
│       ├── pages/                       #   9 个页面（Dashboard / Story / Session / Settings 等）
│       ├── features/                    #   按领域划分的功能模块（103 文件）
│       │   ├── acp-session/             #     ACP 会话管理（stream / chat view / event cards）
│       │   ├── workflow/                #     Workflow + Lifecycle DAG 编辑器
│       │   ├── canvas-panel/            #     Canvas 运行时预览与绑定编辑
│       │   ├── executor-selector/       #     执行器选择与配置
│       │   ├── story/                   #     Story 看板/详情/Session 面板
│       │   ├── task/                    #     Task 列表/卡片/Agent 绑定
│       │   ├── vfs/                     #     VFS 浏览器
│       │   ├── workspace/               #     工作空间管理
│       │   └── ...
│       ├── stores/                      #   13 个 Zustand store
│       ├── components/                  #   通用 UI 组件
│       ├── hooks/                       #   自定义 Hooks
│       ├── services/                    #   API 客户端
│       └── types/                       #   TypeScript 类型定义
├── tests/e2e/                           # Playwright E2E 测试
├── scripts/                             # 开发脚本（启动 / 端口管理 / 构建）
├── docs/                                # 设计文档
└── .trellis/                            # AI 任务管理（Trellis 工作流）
    └── spec/                            #   项目开发规范
```

## 快速开始

### 前置条件

- **Rust** (edition 2024, 推荐 nightly 或最新 stable)
- **Node.js** ≥ 20
- **pnpm** ≥ 10

> 数据库无需额外安装 — 云端使用 `postgresql_embedded` 自动下载管理嵌入式 PostgreSQL，本机后端使用 SQLite。

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
1. 清理遗留端口和残留进程
2. `cargo build` 编译后端
3. 启动 `agentdash-server`（云端后端 :3001）
4. 启动 `agentdash-local`（本机后端，自动 WS 注册）
5. 启动 Vite 前端开发服务器 (:5380)

> **注意**：修改 Rust 代码后需要完整重启 `pnpm dev`，否则浏览器仍运行旧后端。

### 其他启动模式

```bash
pnpm run dev:mono              # 仅云端 + 前端（跳过本机后端）
pnpm run dev:joint:skip-build  # 跳过 cargo build（已手动编译时使用）
pnpm run dev:frontend          # 仅前端
pnpm run dev:backend           # 仅后端
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

## 核心概念详解

### Project

顶层组织单元。一个 Project 关联多个 Agent 和 Story，拥有独立的配置（默认执行器、Workspace、MCP 工具等）。Project 通过 `ProjectAgentLink` 建立与 Agent 的多对多关系，支持 per-project 配置覆写和 Lifecycle 绑定。

### Workspace

逻辑工作空间聚合。表达 Project 依赖的"工作空间身份"，而非某个 backend 上的单一目录。物理目录、backend 与探测事实都通过 `WorkspaceBinding` 挂在该聚合下，支持多 backend 绑定和自动解析策略。

### Story

用户价值单元。承载需求描述、上下文资源（验收标准、设计决策、参考文件）、任务拆解。可拆解为多个 Task 交由不同 Agent 执行。

### Task

最小执行单元。绑定到具体 Agent 实例和 Workspace，通过 Session 驱动 Agent 完成工作。支持自动重试策略（`TaskExecutionMode`）和结构化产物（`Artifact`）收集。

### Session

Agent 的一次完整对话会话。支持 Prompt 发送、工具调用、流式输出。Session 通过 ACP 协议与 Agent 通信，通过 `SessionBinding` 与业务实体建立归属关系。

### VFS (Virtual File System)

统一虚拟文件系统，通过 `MountProvider` 抽象层统一所有文件操作。每个 Mount 声明 provider、backend、根路径和能力集。Agent 的所有文件类工具（fs_read / fs_apply_patch / shell_exec 等）都经过 VFS 路由，确保跨设备、跨 Provider 的一致性。

### Workflow & Lifecycle

两层编排机制。**Workflow** 定义单个 Session 的行为契约（上下文注入、约束规则、完成条件）。**Lifecycle** 编排多个 Step 组成的 DAG，每个 Step 可绑定一个 Workflow，Step 之间通过 Port + Edge 声明依赖关系，支持并行节点执行和 all-complete join 语义。

### Canvas

Project 级可运行前端资产。Agent 可通过工具创建和修改 Canvas 文件（React + 沙箱配置），支持数据绑定（`CanvasDataBinding`），在看板中实时预览渲染结果。

### Routine

项目级 Agent 自动触发规则。支持三种触发源：Cron 定时、HTTP Webhook（带 Bearer Token 认证）、插件事件源（如 GitHub PR 事件）。每次触发产生 `RoutineExecution` 记录，支持 Fresh / Reuse / PerEntity 三种 Session 策略。

## 支持的执行器

| 执行器 | 类型 | 运行位置 | 说明 |
|---|---|---|---|
| Pi Agent | 云端原生 | Cloud Backend | 标准参考实现，完整 Hook/Lifecycle 集成 |
| Claude Code | 第三方 | Local Backend | 通过 vibe-kanban executors 驱动 |
| Codex (OpenAI) | 第三方 | Local Backend | 通过 vibe-kanban executors 驱动 |
| Cursor Agent | 第三方 | Local Backend | 通过 vibe-kanban executors 驱动 |
| Gemini CLI | 第三方 | Local Backend | 通过 vibe-kanban executors 驱动 |
| AMP | 第三方 | Local Backend | 通过 vibe-kanban executors 驱动 |
| Opencode | 第三方 | Local Backend | 通过 vibe-kanban executors 驱动 |

> 第三方执行器当前通过 vibe-kanban 的 `executors` crate 统一接入，未来将根据各方 SDK 成熟度逐步迁移到原生适配层。

## 开发规范

项目维护了完整的开发规范文档，位于 `.trellis/spec/`：

- **前端**: 目录结构、组件规范、类型安全、质量标准
- **后端**: 分层架构、错误处理、Hook Runtime 设计、数据库迁移
- **协作**: 中文沟通、代码注释规范、Git 提交规范

## License

MIT
