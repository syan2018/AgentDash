# 云端发布原语与发现端点

## Goal

补齐云端部署维护所需的基础原语，让后续 Docker Compose 和桌面端服务器指向工作都能依赖稳定契约推进。

## Parent Task

- `.trellis/tasks/05-29-deployment-baseline`
- 主文档：`docs/deployment/deployment-baseline.md`

## Assigned Workstream

建议由其它机器上的后端/部署 Agent 处理。该子任务优先碰云端后端、配置解析、版本/discovery endpoint、migration/doctor 入口，不负责 Compose 文件和桌面端 UI。

## Requirements

- 明确 `agentdash-server serve`、`agentdash-server migrate`、`agentdash-server doctor` 的目标语义。
- 为云端部署提供显式版本信息 endpoint：
  - `GET /api/version`
- 为桌面端和部署入口提供 discovery endpoint：
  - `GET /.well-known/agentdash`
- Discovery response 至少能表达：
  - product marker。
  - public origin。
  - API base URL。
  - Relay WebSocket URL。
  - server version。
  - min / recommended desktop version。
  - relay protocol version。
- 梳理部署配置契约：
  - `AGENTDASH_PUBLIC_ORIGIN`。
  - bind host / port。
  - `DATABASE_URL`。
  - secret / encryption key。
  - `RUST_LOG`。
- 明确开发期 embedded PostgreSQL 与部署期外部 PostgreSQL 的边界。
- 保留启动时 schema readiness check。
- migration 子命令和启动时 migration 的关系需要写清楚。

## Acceptance Criteria

- [x] `docs/deployment/deployment-baseline.md` 中的版本/discovery/配置契约与实际实现或最终设计一致。
- [x] 有明确的 `/api/version` response schema。
- [x] 有明确的 `/.well-known/agentdash` response schema。
- [x] 有明确的 server CLI/subcommand 方案，或说明第一轮为什么暂缓实现。
- [x] 有测试或手动验证步骤覆盖 version/discovery endpoint。
- [x] 不修改桌面端服务器配置 UI。
- [x] 不创建 Compose 基准文件，除非只是为验证 endpoint 做最小说明。

## First Implementation Notes

- 第一轮已实现 `GET /api/version` 与 `GET /.well-known/agentdash`。
- 第一轮暂缓拆出 `agentdash-server migrate` / `doctor` 子命令；当前启动时 migration 与 schema readiness check 仍保留，后续由部署 runbook 和 server CLI 切片继续推进显式 migration。
- Discovery 默认 Relay WebSocket path 为 `/ws/backend`，可用 `AGENTDASH_RELAY_WS_URL` 覆盖。
- `AGENTDASH_BIND_HOST` / `AGENTDASH_PORT` 已作为部署命名进入 server options；`HOST` / `PORT` 仍服务当前开发脚本入口。

## Coordination Notes

- Compose 子任务会依赖 `migrate`、`doctor`、`/api/version` 的契约。
- 桌面端子任务会依赖 `/.well-known/agentdash` 的字段和兼容性语义。
- 如果字段命名发生变化，必须同步更新父任务文档和两个兄弟子任务。

## Open Questions

- 第一轮是否实现真实 CLI subcommands，还是先实现 endpoint 与文档契约。
- `agentdash-server` 是否托管 Web Dashboard 静态资源。
- `AGENTDASH_PUBLIC_ORIGIN` 缺失时，部署 profile 是否直接启动失败。
