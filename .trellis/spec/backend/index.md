# 后端开发指南

> **AgentDashboard 后端开发规范。**

---

## 项目总览

AgentDashboard 采用**云端 + 本机**双后端架构，两个 binary 共享部分 crate：

| Binary | 定位 | 核心职责 |
|--------|------|---------|
| `agentdash-cloud` | 云端后端（数据中枢 + 云端原生 Agent） | REST API、MCP、状态管理、编排、WebSocket 服务端、**PiAgent AgentLoop** |
| `agentdash-local` | 本机后端（第三方 Agent 执行 + 工具环境） | 第三方 Agent 执行、PiAgent tool call 执行、工作空间文件、WebSocket 客户端 |

> 详见 `docs/relay-protocol.md` 和 `docs/modules/09-relay.md`

---

## 数据归属（编码前必读）

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

## 核心数据实体

云端持有的核心实体，遵循 Project → Workspace → Story → Task 的领域层次：

```
Project {
  id: Uuid
  name: String
  description: String
  config: ProjectConfig
  created_at / updated_at
}

Workspace {
  id: Uuid
  project_id: Uuid
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
  backend_id: String
  root_ref: String
  status: Pending | Ready | Offline | Error
  detected_facts: JsonValue
  last_verified_at: Option<DateTime>
  priority: i32
  created_at / updated_at
}

Story {
  id: Uuid
  project_id: Uuid
  default_workspace_id: Option<Uuid>
  title: String
  context: StoryContext
  status: StoryStatus
  created_at / updated_at
}

Task {
  id: Uuid
  story_id: Uuid
  workspace_id: Option<Uuid>
  status: TaskStatus
  agent_binding: AgentBinding
  artifacts: Vec<Artifact>
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

---

## 规范索引

### 通用开发规范

跨模块适用的编码标准和架构约定。

| 文档 | 说明 | 状态 |
|------|------|------|
| [目录结构](./directory-structure.md) | 整洁架构分层、Crate 布局、添加模块步骤 | ✅ 已更新 |
| [架构演进](./architecture-evolution.md) | 历次重大架构变更记录 | ✅ 已拆分 |
| [Repository 模式](./repository-pattern.md) | Repository trait 定义、依赖注入、测试 | ✅ 已更新 |
| [数据库指南](./database-guidelines.md) | PostgreSQL + SQLite + SQLx 存储规范 | ✅ 已更新 |
| [错误处理](./error-handling.md) | 分层错误体系、错误边界规则 | ✅ 已更新 |
| [领域类型化标准](./domain-payload-typing.md) | `serde_json::Value` 类型化盘点和迁移路线 | ✅ 已创建 |
| [质量规范](./quality-guidelines.md) | 代码标准、DTO 命名契约、Session 状态持久化 | ✅ 已更新 |
| [日志规范](./logging-guidelines.md) | 结构化日志、级别约定 | ✅ 已更新 |
| [Relay Protocol](../../../docs/relay-protocol.md) | 云端↔本机 WebSocket 通信协议 | ✅ 已创建 |

### 模块专属契约

按子系统分目录组织，仅在特定模块上下文中适用。

#### `session/` — Session / ACP 核心子系统

| 文档 | 说明 | 状态 |
|------|------|------|
| [流式协议](./session/streaming-protocol.md) | SSE/NDJSON 流式推送跨层契约 | ✅ 已拆分 |
| [Pi Agent 流式合并](./session/pi-agent-streaming.md) | Pi Agent streaming chunk 合并协议 | ✅ 已拆分 |

#### `hooks/` — Hook 运行时

| 文档 | 说明 | 状态 |
|------|------|------|
| [Execution Hook Runtime](./hooks/execution-hook-runtime.md) | Hook Runtime / Workflow Policy / Session Surface 跨层契约 | ✅ 已更新 |
| [Hook Script Engine](./hooks/hook-script-engine.md) | Rhai 脚本引擎、preset 编写指南、沙箱契约 | ✅ 已创建 |

#### `workflow/` — Workflow 引擎

| 文档 | 说明 | 状态 |
|------|------|------|
| [Lifecycle Edge 设计](./workflow/lifecycle-edge.md) | Lifecycle DAG edge 语义、校验规则、运行时推进 | ✅ 已创建 |

#### `vfs/` — 虚拟文件系统

| 文档 | 说明 | 状态 |
|------|------|------|
| [VFS Access](./vfs/vfs-access.md) | 统一 Address Space / Provider / Runtime Tool 跨层契约 | ✅ 已创建 |

#### `capability/` — 能力管线与插件

| 文档 | 说明 | 状态 |
|------|------|------|
| [工具能力管线](./capability/tool-capability-pipeline.md) | ToolCapability 协议、CapabilityResolver、session 工具集治理 | ✅ 已创建 |
| [Plugin API](./capability/plugin-api.md) | 开源核心 + 企业扩展插件架构 | ✅ 已创建 |
| [LLM Model Config](./capability/llm-model-config.md) | ThinkingLevel / ModelInfo / Provider Registry 架构 | ✅ 已创建 |

### 跨层契约（前后端共享）

详见 [跨层契约索引](../cross-layer/index.md)

| 文档 | 说明 | 状态 |
|------|------|------|
| [ACP Meta Warp](../cross-layer/acp-meta-warp.md) | ACP `_meta.agentdash` 跨层序列化契约 | ✅ 已更新 |

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

---

*更新：2026-04-21 — 按子系统分目录重组（session/hooks/workflow/vfs/capability），跨层契约移至 cross-layer/*
