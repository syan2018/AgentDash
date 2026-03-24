# Backend Development Guidelines

> Best practices for backend development in this project.

---

## Overview

<!-- PROJECT-SPECIFIC-START: Backend Overview -->
> **AgentDashboard 后端开发规范。**

### 项目总览

AgentDashboard 采用**云端 + 本机**双后端架构，两个 binary 共享部分 crate：

| Binary | 定位 | 核心职责 |
|--------|------|---------|
| `agentdash-cloud` | 云端后端（数据中枢 + 云端原生 Agent） | REST API、MCP、状态管理、编排、WebSocket 服务端、**PiAgent AgentLoop** |
| `agentdash-local` | 本机后端（第三方 Agent 执行 + 工具环境） | 第三方 Agent 执行、PiAgent tool call 执行、工作空间文件、WebSocket 客户端 |

> 详见 `docs/relay-protocol.md` 和 `docs/modules/09-relay.md`

---

### 数据归属（编码前必读）

| 数据实体 | 归属方 | 说明 |
|---------|--------|------|
| `Project` | **云端** | 项目定义和配置 |
| `Workspace`（元数据） | **云端** | 逻辑工作空间身份（`identity_kind` / `identity_payload` / `resolution_policy`）以及其 `WorkspaceBinding` 物理绑定 |
| Workspace 物理文件 | **本机** | 实际的代码文件、Git worktree |
| `Story` | **云端** | 用户价值单元 |
| `Task` | **云端** | 执行单元定义（只引用逻辑 `workspace_id`，运行时再解析到具体 binding/backend） |
| `Backend` | **云端** | 已注册本机列表、鉴权 token、在线状态 |
| `View` / `UserPreferences` | **云端** | 跨后端视图和偏好 |
| `Settings` | **云端** | 系统级和用户级设置 |
| `StateChange` | **云端** | 不可变事件日志 |
| `MCP` | **云端** | 对外暴露的 Model Context Protocol |
| `ExecutorHub`（运行时） | **本机** | Agent 会话内存态 + JSONL 缓存 |
| `AgentConnector` | **本机** | 第三方 Agent 进程管理（Claude Code、Codex 等） |
| `ToolExecutor` | **本机** | PiAgent tool call 的本地执行环境 |
| `PiAgent AgentLoop` | **云端** | 云端原生 Agent，直接访问 DB，tool call 路由到本机 |
| `SessionNotification` | **流经** | 第三方 Agent：本机产出 → 云端转发；PiAgent：云端直接产出 |

**规则：云端代码不应直接访问本地文件系统。本机代码不应直接读写业务数据库。**

**路由模型：**
- 一个本机后端 = 一台物理机器，可管理多个 WorkspaceBinding 根目录
- 命令路由基于 `Task.workspace_id → WorkspaceResolution → WorkspaceBinding.backend_id`
- `Project` / `Story` 不再直接持有运行时 `backend_id`，只持有逻辑 `workspace_id` 或其默认配置

---

### 核心数据实体

云端持有的核心实体，遵循 Project → Workspace → Story → Task 的领域层次：

```
Project {
  id: Uuid
  name: String
  description: String
  config: ProjectConfig       // Agent 预设、默认 Workspace 等
  created_at / updated_at
}

Workspace {
  id: Uuid
  project_id: Uuid            // 所属项目
  name: String
  identity_kind: GitRepo | P4Workspace | LocalDir
  identity_payload: JsonValue
  resolution_policy: PreferDefaultBinding | PreferOnline
  default_binding_id: Option<Uuid>
  status: Pending | Preparing | Ready | Active | Archived | Error
  bindings: Vec<WorkspaceBinding>
  created_at / updated_at
}

WorkspaceBinding {
  id: Uuid
  workspace_id: Uuid
  backend_id: String          // ⚡ 物理文件所在的本机后端（命令路由依据）
  root_ref: String            // backend 上的物理根目录
  status: Pending | Ready | Offline | Error
  detected_facts: JsonValue
  last_verified_at: Option<DateTime>
  priority: i32
  created_at / updated_at
}

Story {
  id: Uuid
  project_id: Uuid            // 所属项目
  default_workspace_id: Option<Uuid>
  title: String
  context: StoryContext        // PRD、规范引用、资源清单
  status: StoryStatus
  created_at / updated_at
}

Task {
  id: Uuid
  story_id: Uuid              // 归属的 Story
  workspace_id: Option<Uuid>  // 关联的 Workspace（外键）
  status: TaskStatus
  agent_binding: AgentBinding  // 结构化 Agent 绑定
  artifacts: Vec<Artifact>    // 执行产物
  created_at / updated_at
}
```

**实体关系**：
```
Project (1) → (*) Workspace
Project (1) → (*) Story
Story (1)   → (*) Task
Workspace (1) ← (*) Task（多 Task 可共享同一逻辑 Workspace）
Workspace (1) → (*) WorkspaceBinding
```
<!-- PROJECT-SPECIFIC-END -->

This directory contains guidelines for backend development. Fill in each file with your project's specific conventions.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | ✅ 已更新（整洁架构分层） |
| [Repository Pattern](./repository-pattern.md) | Repository trait 定义、依赖注入、测试 | ✅ 已创建 |
| [ACP Meta Warp](./acp-meta-warp.md) | ACP `_meta.agentdash` 跨层契约（可扩展消息语义层） | ✅ 已更新 |
| [Address Space Access](./address-space-access.md) | 统一 Address Space / Provider / Runtime Tool 跨层契约 | ✅ 已创建 |
| [LLM Model Config](./llm-model-config.md) | ThinkingLevel 枚举、ModelInfo 元数据、Provider Registry 架构跨层契约 | ✅ 已创建 |
| [Execution Hook Runtime](./execution-hook-runtime.md) | Hook Runtime / Workflow Policy / Session Surface 跨层契约 | ✅ 已创建 |
| [Database Guidelines](./database-guidelines.md) | ORM patterns, queries, migrations | To fill |
| [Error Handling](./error-handling.md) | Error types, handling strategies | To fill |
| [Quality Guidelines](./quality-guidelines.md) | Code standards, forbidden patterns | ✅ 已更新（含业务 API DTO 命名契约） |
| [Logging Guidelines](./logging-guidelines.md) | Structured logging, log levels | To fill |
| [Relay Protocol](../../docs/relay-protocol.md) | 云端↔本机 WebSocket 通信协议 | ✅ 已创建 |
| [Plugin API](./plugin-api.md) | 开源核心 + 企业扩展插件架构、各扩展点实现指南 | ✅ 已创建 |

---

## How to Fill These Guidelines

For each guideline file:

1. Document your project's **actual conventions** (not ideals)
2. Include **code examples** from your codebase
3. List **forbidden patterns** and why
4. Add **common mistakes** your team has made

The goal is to help AI assistants and new team members understand how YOUR project works.

<!-- PROJECT-SPECIFIC-START: Design Constraints -->
---

## 设计约束（编码前必读）

### 策略可插拔原则

后端的核心设计是**策略可插拔**，不要将实现细节硬编码：

```
✅ 正确：定义 StateManager 接口，实现可替换
❌ 错误：在业务逻辑中直接调用具体数据库 API
```

### 模块边界原则

- 每个模块只能调用接口，不能依赖其他模块的内部实现
- 状态变更必须通过 StateManager，不能直接操作存储
- 编排层不能直接操作执行层的 Agent

### 状态变更原则

- 所有状态变更必须记录 StateChange（不可省略）
- 变更必须包含 `reason` 字段说明原因
- 严禁直接覆盖状态而不记录历史

---

## 语言要求

> **必须使用中文**

- 所有与用户的交流必须使用中文
- 所有文档更新必须使用中文
- 代码注释必须使用中文
- 提交信息必须使用中文
<!-- PROJECT-SPECIFIC-END -->

---

**Language**: All documentation should be written in **English**.
