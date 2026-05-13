# 设计：AgentDash 与 multica 本地运行时概念对齐学习

## 研究边界

本任务是研究与概念/目录对齐，不进入实现。研究对象分为三层：

1. AgentDash 现有架构：Rust cloud backend、`agentdash-local`、Relay 协议、SessionHub、VFS、Routine、Lifecycle、前端 Vite 应用。
2. multica 参考架构：Go server、CLI/daemon、agent runtime、task queue、Next web、Electron desktop、`packages/core/views/ui`。
3. 后续 AgentDash 可吸收的设计方向：云端协作模型、服务端事件/数据架构、本地连接健壮性、runtime 生命周期、任务执行可观测性、desktop/local 一体化体验。

## 概念映射框架

研究文档应按两条线组织：

1. 概念映射：“概念 -> AgentDash 当前实现 -> multica 实现 -> 可学习内容 -> 不适配点”。
2. 目录映射：“AgentDash 目录 -> multica 目录 -> 各自职责 -> 后续阅读入口”。

优先映射：

| 主题 | AgentDash 入口 | multica 入口 | 关注点 |
| --- | --- | --- | --- |
| 云端服务 | `crates/agentdash-api` | `server/cmd/server`, `server/internal/handler` | API、事件、调度边界 |
| 应用服务 | `crates/agentdash-application` | `server/internal/service`, `server/internal/events` | 业务流程、任务生命周期、事件发布 |
| 领域模型 | `crates/agentdash-domain` | `server/pkg/db/generated`, SQL schema, handler DTO | 强领域模型 vs 数据行模型 |
| 数据层 | `crates/agentdash-infrastructure/migrations`, repository impl | `server/pkg/db/queries`, `server/migrations`, sqlc generated | 迁移、查询、聚合、运行历史 |
| 实时事件 | SSE stream、session stream、relay events | `internal/realtime`, `pkg/protocol`, WS hub | 事件粒度、scope、replay、缓存同步 |
| 权限/组织 | projects/workspaces/grants/users/groups | workspace/member/invitation/PAT | 多租户与成员边界 |
| 本机进程 | `crates/agentdash-local` | `server/internal/daemon`, `server/cmd/multica` | 注册、心跳、重连、任务执行 |
| 本机注册表 | `relay/registry.rs` | `agent_runtime`, daemon register API | 在线状态、能力、重复注册、恢复 |
| 协议 | `crates/agentdash-relay` | `server/pkg/protocol`, daemon client | 消息类型、事件粒度、错误模型 |
| 执行抽象 | `SessionHub`, `AgentConnector`, `agentdash-agent` | `pkg/agent.Backend`, daemon `executeAndDrain` | 原生 loop vs CLI backend |
| 任务模型 | `Story/Task/SessionBinding` | `issue`, `agent_task_queue`, `task_message` | 业务任务与执行队列拆分 |
| 文件/工作区 | VFS / mount provider / relay fs | workspace repos / worktree / env root / GC | 物理工作目录与虚拟文件系统关系 |
| 自动化 | `Routine` | `Autopilot` | 触发、run history、失败治理 |
| 协作产品 | `Story/Task/Canvas/Workflow` | `Issue/Project/Comment/Inbox/Activity/Squad` | 用户协作表面与 AI 工作反馈 |
| 前端状态 | Zustand + services + SSE | TanStack Query + Zustand + WS sync | server state 缓存纪律 |
| 前端目录 | `frontend/src/pages/features/stores/services` | `apps/web`, `packages/core`, `packages/views`, `packages/ui` | app 壳、无头逻辑、业务页面、UI 原子层 |
| 桌面端 | 待统一架构任务 | `apps/desktop` + shared packages | 前端与本机能力合并体验 |
| 文档/部署/CLI | `README.md`, scripts, future desktop tasks | docs, self-host docs, `cmd/multica`, Dockerfiles | 开发者入口、自部署、运维体验 |

## 重点专题

### 1. 全量目录对应

第一阶段不要急着深入实现细节，先建立目录级导航：

- 对每个 AgentDash 顶层 crate / frontend 子目录，找到 multica 最接近的目录。
- 标注“一对一”“一对多”“AgentDash 独有”“multica 独有”“概念相近但职责不同”。
- 对 multica 中 AgentDash 暂无明显对应的能力，也要记录为学习候选，例如 inbox、subscriber、activity、workspace invitation、runtime usage、task usage rollup、docs app、self-hosting。

### 2. 云端能力对齐

云端能力需要与 local/daemon 同等重视。重点比较：

- API/handler 路由如何组织。
- 服务层是否集中承载业务状态机。
- 数据库 schema、查询、迁移与聚合统计如何支撑产品体验。
- 实时事件如何从服务端状态变化传播到前端缓存。
- 多租户、成员、权限、token、workspace 边界如何落地。
- Issue/Comment/Inbox/Activity 这类协作表面与 AgentDash Story/Task/Session/Canvas 的关系。

### 3. local backend vs daemon

需要把两者当作同一类问题的不同解法，而不是简单的一一对应：

- AgentDash local backend 是云端能力的本机执行代理，强调 VFS、工具、终端、MCP、可选 SessionHub。
- multica daemon 是 agent task runner，强调 runtime 注册、任务认领、执行目录、CLI agent 调用、任务状态回报。

研究时应重点比较：

- 连接握手：token、backend/runtime 身份、重复注册处理。
- 能力上报：executor/MCP/root path vs provider/runtime/device/repo/settings。
- 活性判断：AgentDash 当前在线注册表 vs multica heartbeat、last_seen、offline sweeper、runtime_gone 恢复。
- 执行恢复：AgentDash interrupted session recovery vs multica orphan task recovery、session pinning、workdir/session resume。
- 任务可观测：AgentDash Backbone session events vs multica task_message 序列。
- 本地资源治理：AgentDash accessible roots / VFS materialization vs multica repo cache / worktree / env GC。

### 4. desktop/local 一体化

需要研究 multica 是否把 desktop 当作 web shell、daemon controller、或本机 runtime 管理入口。结论应服务于 AgentDash 后续桌面端统一架构：

- 哪些前端模块可以跨 web/desktop 共享。
- 本机 backend 启停、日志、连接状态、可访问目录、MCP server、agent CLI 检测应暴露到什么 UI。
- 桌面端是否应负责拉起/监督 local backend，还是只连接已有 local backend。
- 如何避免 cloud/server/local 三者状态分裂。

### 5. 不照搬原则

AgentDash 已有更强的底层抽象，不应把 multica 的实现按语言或结构复制过来：

- 不用 Go handler/sqlc 的目录形态替代 Rust 分层 crate。
- 不用 issue/task queue 直接替换 Story/Task/SessionBinding，但可以学习“业务任务”和“执行尝试”的拆分。
- 不用 daemon CLI backend 替代 Pi Agent Loop，但可以学习 CLI provider 的执行观测、session pinning、poisoned output 分类。
- 不把物理 worktree 管理绕过 VFS，但可以学习 env root 生命周期与 GC。

## 预期产物

最终研究文档建议放在任务目录下：

- `research/concept-map.md`
- `research/directory-map.md`
- `research/cloud-capability-map.md`
- `research/local-daemon-comparison.md`
- `research/desktop-local-integration.md`
- `research/learning-backlog.md`

如果后续进入实现，再把 backlog 拆成独立 Trellis task。
