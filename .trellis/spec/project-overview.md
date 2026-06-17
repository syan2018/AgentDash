# AgentDashboard - 项目总览

> 所有 spec 文档的核心锚点，开始任何开发工作前必须阅读。

---

## 项目定位

AgentDashboard 是一个**统一看板系统**，用于在多设备、多项目中管理 AI Agent 的协同工作。用一个看板控制多设备、多类型项目中 agent 协同，支持任意数字生产 SOP 的维护和管理。

---

## 核心抽象

### Project

多设备、多工作区、多 Agent 协作的业务与权限根。Project 维护 workspace、agent profile、settings、shared assets、capability baseline 与协作上下文。业务对象只通过 `SubjectRef` 接入控制面，不在总览层定义为核心抽象。

### SubjectRef

业务对象进入 runtime / lifecycle 的稳定引用，形如 `SubjectRef(kind, id)`。SubjectRef 只表达“哪个业务对象被处理或投影”，不携带业务 aggregate body，不拥有 runtime state。具体业务对象的 spec 应放在对应模块文档中。

### LifecycleRun

被追踪的执行生命过程 / control ledger / context container。一个 `LifecycleRun` 可以包含 0..N 个 `OrchestrationInstance`，用于在同一生命过程中承载 root flow、executor graph、companion/review graph、routine phase graph 或 plain agent control state。`LifecycleRun` 不直接拥有 `RuntimeSession`，也不承诺唯一 `WorkflowGraph`；runtime session trace 通过 `RuntimeSessionExecutionAnchor` 回到 run / agent / frame / orchestration node 坐标。

### WorkflowGraph / AgentProcedure

`WorkflowGraph` 是静态 definition input：可执行 Activity graph 配置在进入 runtime 前由 application compiler 编译为不可变 `OrchestrationPlanSnapshot`，再 materialize 为 `LifecycleRun.orchestrations[]` 中的 `OrchestrationInstance`。`AgentProcedure` 表示单个 Agent Activity 的行为、context、capability、hook 契约。

### LifecycleAgent / AgentFrame

`LifecycleAgent` 是 `LifecycleRun` 内的一等 Agent 运行身份。`AgentFrame` 是某个 revision 的 effective runtime surface，拥有 procedure、capability、context slice、VFS 与 MCP。Runtime trace/delivery refs 由 `RuntimeSessionExecutionAnchor` 投影，原因是 runtime session 到 run/agent/frame 的索引需要独立于 frame revision。

### RuntimeSession

运行轨迹容器。`RuntimeSession` 承载 event stream（BackboneEnvelope）、turn、tool call、resume、debug replay、projection 与 trace lineage；不拥有 business ownership、permission scope 或 Lifecycle progress truth。业务入口必须从 `ExecutionIntent`、`SubjectRef`、run/agent/frame refs 或 graph instance refs 开始。

---

## 系统架构

### 云端/本机双后端模型

```
┌──────────────────────────────────────────────────────────┐
│                    云端后端（Cloud）                       │
│  ┌────────────────────────────────────────────────────┐  │
│  │  编排层 · 状态层 · 连接层                           │  │
│  │  REST API · MCP · NDJSON · WebSocket 服务端        │  │
│  │  PiAgent AgentLoop（云端原生 Agent）               │  │
│  │  BackendRegistry（在线本机管理）                    │  │
│  │  Relay（命令路由 + 执行输出 + tool call 转发）      │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────┬───────────────────────────────┘
                           │ WebSocket（本机主动连接）
            ┌──────────────┼──────────────┐
            ▼              ▼              ▼
    ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
    │ 本机 A         │ │ 本机 B       │ │ 本机 C       │
    │ 第三方Agent执行│ │ 第三方Agent  │ │ 第三方Agent  │
    │ PiAgent工具执行│ │ PiAgent工具  │ │ PiAgent工具  │
    │ 工作空间文件   │ │ 工作空间文件 │ │ 工作空间文件 │
    └──────────────┘ └──────────────┘ └──────────────┘
```

- **云端后端**：数据中枢 + 用户入口 + 调度中继 + PiAgent（云端原生 Agent）。持有所有业务数据，暴露 REST API / MCP，通过 WebSocket 管理和调度本机。PiAgent 的 AgentLoop 在云端运行，tool call 路由到本机执行。
- **本机后端**：per-machine 进程，管理本机上的多个 Workspace 目录。主动连接云端 WebSocket，对第三方 Agent 执行 `command.prompt`，对 PiAgent 执行 `command.tool.*`（文件读写、Shell 执行等）。

### 数据归属

- **云端拥有**：Project / Workspace 元数据 / Settings / StateChange / Lifecycle 控制面事实 / RuntimeSession 事件
- **本机拥有**：Agent 进程、工作空间物理文件
- 命令路由基于 `Workspace.backend_id`（物理文件所在本机）

### 三层结构（云端内部）

| 层 | 职责 |
|----|------|
| 编排层（Orchestration） | 任务拆解策略、执行流程编排、人机协作 |
| 状态层（State Management） | 状态容器管理、状态迁移控制、验证规则 |
| 连接层（Connectivity + Relay） | 前端连接（REST + NDJSON）、本机管理（WebSocket）、中继路由、MCP 暴露 |

代码按整洁架构分层到多个 crate，详见 [目录结构](./backend/directory-structure.md)。

---

## 技术栈概要

- **后端**：Rust + Axum + Tokio + SQLx + PostgreSQL（embedded）/ SQLite（本机会话）
- **前端**：React 19 + TypeScript 5.9 + Vite 7 + Tailwind v4 + Zustand 5
- **协议**：Backbone Protocol（内部事件流）+ MCP（对外能力暴露）+ REST + NDJSON + WebSocket
- **Agent**：PiAgent（云端原生）+ 第三方（Claude Code、Codex、AMP 等，通过 AgentConnector 可扩展）

详见 [tech-stack.md](./tech-stack.md)。

---

## 核心设计原则

1. **策略可插拔**：状态存储、隔离方式、编排策略、验证方式都是可替换的策略接口
2. **状态即真相**：所有业务数据归云端，历史轨迹完整记录（StateChange 不可变日志），内部事件流统一使用 Backbone Protocol
3. **接口稳定，实现可变**：模块间通过稳定接口交互
4. **连接透明化与可恢复性**：支持 Resume 机制（基于 ID 的增量恢复），网络断连后无缝衔接
5. **预研阶段**：当前完全未上线，不需要兼容性方案，保持项目最正确状态
