# 目录映射：AgentDash ↔ multica

## 总览

AgentDash 是 Rust 分层后端 + Vite 前端 + 本机 relay backend；multica 是 Go server/CLI/daemon + Next web + Electron desktop + packages 单仓。两者目录不是一一同构，但可以按职责建立导航。

| 职责 | AgentDash | multica | 对齐判断 |
| --- | --- | --- | --- |
| 云端 API 入口 | `crates/agentdash-api/src/routes.rs` | `server/cmd/server`, `server/internal/handler` | AgentDash 用 Rust route modules，multica 用 Go handler files |
| 应用服务/编排 | `crates/agentdash-application` | `server/internal/service`, `server/internal/events` | AgentDash 分层更强；multica 把任务状态机集中在 service/handler |
| 领域模型 | `crates/agentdash-domain` | SQL schema + `server/pkg/db/generated` + handler DTO | AgentDash 有显式 domain/repository trait；multica 数据行模型更直接 |
| 数据库/迁移 | `crates/agentdash-infrastructure/migrations` | `server/migrations`, `server/pkg/db/queries`, `sqlc.yaml` | multica 查询与 schema 更贴近产品实体，统计/rollup 更丰富 |
| 本机后端 | `crates/agentdash-local` | `server/internal/daemon`, `server/cmd/multica` | 同类问题不同解法：relay backend vs task runner daemon |
| Relay/协议 | `crates/agentdash-relay`, `agentdash-agent-protocol` | `server/pkg/protocol`, daemon client | AgentDash relay 更偏工具/session transport；multica protocol 更偏业务 WS events |
| Agent 执行抽象 | `crates/agentdash-agent`, `agentdash-executor`, `agentdash-spi` | `server/pkg/agent` | AgentDash 原生 loop/Hook/Capability 更强；multica CLI adapter 经验丰富 |
| MCP/插件 | `agentdash-mcp`, `agentdash-plugin-api`, `first-party-plugins` | skill、MCP config、agent custom args/env | AgentDash 平台扩展更强；multica 更贴近 workspace skill 与 CLI 启动 |
| 前端 App | `frontend/src` | `apps/web` | AgentDash 单 app；multica web 是 Next app shell |
| 前端共享逻辑 | `frontend/src/services`, `stores`, `features` | `packages/core`, `packages/views`, `packages/ui` | multica 的跨 web/desktop 包边界值得学习 |
| Desktop | 暂无正式 app，相关任务在 Trellis 中规划 | `apps/desktop` | multica 已有 Electron daemon manager、tab shell、desktop runtime UI |
| 文档/部署 | `README.md`, `docs`, `scripts/dev-joint.js` | `apps/docs`, `SELF_HOSTING*.md`, Dockerfiles, CLI install scripts | multica 对外部署和 CLI/desktop 入口更完整 |

## 后端目录对应

| AgentDash | multica | 说明 |
| --- | --- | --- |
| `crates/agentdash-api/src/routes/*.rs` | `server/internal/handler/*.go` | HTTP API handler。AgentDash 按路由模块，multica 按领域文件。 |
| `crates/agentdash-api/src/stream.rs` | `server/internal/realtime/hub.go`, `server/pkg/protocol/events.go` | AgentDash 以 SSE/NDJSON 轮询补发为主；multica 以 WS scope rooms + protocol event 为主。 |
| `crates/agentdash-api/src/relay` | `server/internal/handler/daemon*.go`, `server/internal/daemon/client.go` | AgentDash 云端接收本机 backend 注册和命令响应；multica server 与 daemon 通过 daemon API/WS wakeup 协作。 |
| `crates/agentdash-application/src/session` | `server/internal/service/task.go` | 都承载执行状态推进；AgentDash 面向 session/turn/event，multica 面向 task queue lifecycle。 |
| `crates/agentdash-application/src/workflow` | `server/internal/service/task.go`, `autopilot_scheduler.go` | AgentDash Workflow/Lifecycle 更强；multica 主要是 Issue/Autopilot 驱动任务。 |
| `crates/agentdash-domain/src/*` | `server/pkg/db/queries/*.sql`, `server/pkg/db/generated` | AgentDash 领域模型显式；multica 以 SQL/生成类型作为事实源。 |
| `crates/agentdash-infrastructure/src/persistence/postgres` | `server/pkg/db/queries` | AgentDash repository impl；multica sqlc query 文件。 |

## 本机运行时目录对应

| AgentDash | multica | 说明 |
| --- | --- | --- |
| `crates/agentdash-local/src/main.rs` | `server/cmd/multica/main.go`, `cmd_daemon.go` | 本机进程入口。 |
| `crates/agentdash-local/src/ws_client.rs` | `server/internal/daemon/daemon.go`, `client.go`, `wakeup.go` | 连接云端、注册、消息循环、重连。 |
| `crates/agentdash-local/src/handlers/*` | `server/internal/daemon/daemon.go`, `execenv/*` | AgentDash 处理云端命令；multica 处理 task claim 后的执行环境和 provider 启动。 |
| `crates/agentdash-local/src/tool_executor.rs` | `server/internal/daemon/execenv`, `repocache` | AgentDash 直接暴露工具能力；multica 构造 workdir/env 后调用 CLI agent。 |
| `crates/agentdash-local/src/mcp_client_manager.rs` | agent MCP config/custom args/env | AgentDash MCP relay 是一等能力；multica 作为 agent config 注入。 |
| `crates/agentdash-application/src/session/hub` | daemon task session pin/resume | AgentDash 以 SessionHub 持久化和恢复；multica 以 task row 的 session_id/work_dir 恢复。 |

## 前端/desktop 目录对应

| AgentDash | multica | 说明 |
| --- | --- | --- |
| `frontend/src/pages` | `apps/web/app`, `apps/desktop/src/renderer/src/pages` | 页面路由。 |
| `frontend/src/features` | `packages/views/*` | 业务视图组件；multica 抽到跨宿主 package。 |
| `frontend/src/services` / `api` | `packages/core/*/queries.ts`, `mutations.ts`, `api` | multica 把 server state 查询封装成 core 包。 |
| `frontend/src/stores` | `packages/core/*/store.ts`, desktop renderer stores | Zustand client state。 |
| `frontend/src/components/ui` | `packages/ui/components` | 通用 UI。multica 有 shadcn 风格包。 |
| 暂无 desktop main/preload | `apps/desktop/src/main`, `src/preload`, `src/shared` | multica desktop 负责 daemon 管理、CLI bootstrap、IPC bridge。 |

补充事实：AgentDash 当前仓库未发现正式 `src-tauri`、`tauri.conf.json`、Electron desktop 包；根 `pnpm-workspace.yaml` 仅包含 `frontend`。当前 `pnpm dev` 是开发期联合调试入口，不等价于产品化 desktop/local backend manager。

## multica 独有但值得建立阅读入口

- `server/internal/daemon/gc.go`：本机 workdir/产物清理。
- `server/internal/daemon/repocache`：repo cache + worktree。
- `server/internal/handler/inbox.go`, `activity.go`, `subscriber.go`：协作反馈闭环。
- `server/internal/handler/runtime_*`：runtime health、models、local skills、visibility/update。
- `server/internal/service/task.go`：task queue 状态机核心。
- `apps/desktop/src/main/daemon-manager.ts`：desktop 管 daemon。
- `packages/core/realtime/use-realtime-sync.ts`：WS 到 React Query cache 同步。
- `server/internal/events/bus.go`、`server/cmd/server/listeners.go`：领域事件到 realtime fanout 的关键桥。
- `apps/desktop/src/renderer/src/stores/tab-store.ts`：workspace scoped desktop tabs 与独立 memory router。

## AgentDash 独有但对照时要保留

- `crates/agentdash-agent`：Pi Agent Loop、工具审批、compaction、runtime delegate。
- `crates/agentdash-spi`：Hook、MountProvider、Capability 等平台契约。
- `crates/agentdash-application/src/workflow`：Workflow/Lifecycle DAG。
- `crates/agentdash-api/src/mount_providers`, `vfs_*` routes：VFS 统一寻址与 surface。
- `frontend/src/features/session-context`：Context Inspector / Hook Runtime 可视化。
