# 后端开发指南

> AgentDashboard 后端开发规范。

---

## 架构概览

云端 + 本机双后端架构：

| Binary | 定位 | 核心职责 |
|--------|------|---------|
| `agentdash-cloud` | 云端后端 | REST API、MCP、状态管理、编排、WebSocket 服务端、PiAgent AgentLoop |
| `agentdash-local` | 本机后端 | 第三方 Agent 执行、PiAgent tool call 执行、工作空间文件、WebSocket 客户端 |

**规则：云端代码不应直接访问本地文件系统。本机代码不应直接读写业务数据库。**

---

## 数据归属

| 数据实体 | 归属 | 说明 |
|---------|------|------|
| Project / Story / Task | 云端 | 业务数据 |
| Workspace（元数据） | 云端 | 逻辑身份 + WorkspaceBinding 物理绑定 |
| Workspace 物理文件 | 本机 | 代码文件、Git worktree |
| Settings / StateChange | 云端 | 系统配置、不可变事件日志 |
| Backend | 云端 | 已注册本机列表、在线状态 |
| PiAgent AgentLoop | 云端 | 云端原生 Agent，tool call 路由到本机 |
| BackboneEnvelope | 流经 | 第三方：本机→云端；PiAgent：云端直接产出 |

路由模型：命令路由基于 `Task.workspace_id → WorkspaceResolution → WorkspaceBinding.backend_id`

---

## 规范索引

### 通用开发规范

| 文档 | 说明 |
|------|------|
| [目录结构](./directory-structure.md) | 整洁架构分层、Crate 布局 |
| [架构演进](./architecture-evolution.md) | 历次重大架构变更记录 |
| [Repository 模式](./repository-pattern.md) | Repository trait 定义、依赖注入 |
| [数据库指南](./database-guidelines.md) | PostgreSQL + SQLite + SQLx |
| [错误处理](./error-handling.md) | 分层错误体系 |
| [领域类型化标准](./domain-payload-typing.md) | `serde_json::Value` 治理 |
| [Embedded Skill Bundles](./embedded-skill-bundles.md) | 源码内嵌 skill 文档包 |
| [质量规范](./quality-guidelines.md) | DTO 命名、Session 持久化 |
| [日志规范](./logging-guidelines.md) | 结构化日志 |
| [Runtime Gateway](./runtime-gateway.md) | Runtime Action 跨层调用 |
| [Story Task Runtime](./story-task-runtime.md) | Story/Task/Session 架构（Model C） |
| [Shared Library](./shared-library.md) | 公共配置资产、builtin seed、Project 安装来源 |

### 模块专属契约

#### `session/` — Session 核心子系统

| 文档 | 说明 |
|------|------|
| [流式协议](./session/streaming-protocol.md) | SSE/NDJSON 流式推送 |
| [Pi Agent 流式合并](./session/pi-agent-streaming.md) | Pi Agent chunk 合并协议 |
| [Session 运行态](./session/runtime-execution-state.md) | runtime registry、turn supervisor |
| [Session Startup Pipeline](./session/session-startup-pipeline.md) | LaunchCommand → ExecutionContext |
| [ExecutionContext Frames](./session/execution-context-frames.md) | connector projection |
| [Bundle 主数据面](./session/bundle-main-datasource.md) | SessionContextBundle |

#### `hooks/` — Hook 运行时

| 文档 | 说明 |
|------|------|
| [Execution Hook Runtime](./hooks/execution-hook-runtime.md) | Hook Runtime 跨层契约 |
| [Hook Script Engine](./hooks/hook-script-engine.md) | Rhai 脚本引擎 |

#### `workflow/` — Workflow 引擎

| 文档 | 说明 |
|------|------|
| [Activity Lifecycle](./workflow/activity-lifecycle.md) | Activity / Executor / Attempt / Scheduler 运行契约 |
| [Lifecycle Edge 设计](./workflow/lifecycle-edge.md) | DAG edge 语义与校验 |

#### `vfs/` — 虚拟文件系统

| 文档 | 说明 |
|------|------|
| [VFS Access](./vfs/vfs-access.md) | 统一 VFS 跨层契约 |
| [VFS Materialization](./vfs/vfs-materialization.md) | URI 物化契约 |

#### `capability/` — 能力管线与插件

| 文档 | 说明 |
|------|------|
| [工具能力管线](./capability/tool-capability-pipeline.md) | ToolCapability 协议 |
| [Plugin API](./capability/plugin-api.md) | 插件架构 |
| [LLM Model Config](./capability/llm-model-config.md) | Provider Registry |
