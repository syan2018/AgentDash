# AgentDashboard - 项目总览

> 所有 spec 文档的核心锚点，开始任何开发工作前必须阅读。

---

## 项目定位

AgentDashboard 是一个**统一看板系统**，用于在多设备、多项目中管理 AI Agent 的协同工作。用一个看板控制多设备、多类型项目中 agent 协同，支持任意数字生产 SOP 的维护和管理。

---

## 核心抽象

### Story（aggregate root）

从用户角度描述需求的工作单元。维护完整设计上下文（PRD、规范、资源引用），编排执行，聚合结果。每个 Story 对应一个 durable session（通过 `SessionBinding`）。持有 `Vec<Task>` 作为 child entity（`stories.tasks` JSONB）。

**生命周期**：创建 → 上下文注入 → 拆解完成 → 编排执行 → 验收完成/失败

### Task（Story child entity）

Story aggregate 下的执行单元声明。持有归属关系、agent binding、workspace 约束。`status` / `artifacts` 是 LifecycleRun step state 的只读投影，不承载 runtime 真相。Task 通过 `lifecycle_step_key` 绑定到 workflow step。详见 [story-task-runtime.md](./backend/story-task-runtime.md)。

### Session

执行层的核心单元。Story session 是 story 的运行时外壳，event stream（BackboneEnvelope）是一切状态变更的审计源。child session 覆盖 companion 对话、step 远程执行等场景。

### LifecycleRun

Workflow 推进的运行态，1:1 挂在 Story session 上。`steps: Vec<LifecycleStepState>` 记录 step 运行态，step state 变化投影到 Task。

---

## Story vs Task

| 维度 | Story | Task |
|------|-------|------|
| 定位 | aggregate root、用户价值单元 | Story aggregate 下的 child entity |
| 持久化 | `stories` 表 | `stories.tasks` JSONB 列（无独立表） |
| 上下文 | 维护完整设计信息 | 接收派生上下文 |
| Agent 绑定 | 不直接绑定 | 持有 `AgentBinding` 声明 |
| 状态 | 业务审计字段 | step state 的只读投影 |
| Repository | `StoryRepository`（含 Task CRUD） | 无独立 Repository |

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

- **云端拥有**：Project / Story / Task / Workspace 元数据 / Settings / StateChange / Session 事件
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
