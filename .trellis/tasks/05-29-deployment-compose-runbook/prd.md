# Compose 交付基准与运维 Runbook

## Goal

建立 AgentDash 的 Docker Compose 基准交付形态，并维护云端部署、更新、备份、恢复的操作文档。

## Parent Task

- `.trellis/tasks/05-29-deployment-baseline`
- 主文档：`docs/deployment/deployment-baseline.md`

## Assigned Workstream

建议由其它机器上的部署/DevOps Agent 处理。该子任务优先碰 `deploy/`、Dockerfile、Compose、runbook 和脚本，不负责后端 endpoint 实现和桌面端 UI。

## Requirements

- 设计或实现 `deploy/compose/docker-compose.yml`。
- 设计或实现 `deploy/compose/.env.example`。
- 设计或实现云端 Dockerfile：
  - 包含 `agentdash-server`。
  - 包含 Web Dashboard 静态资源。
  - 能复用同一镜像运行 `serve` 与 `migrate`。
- Compose 基准至少表达：
  - `postgres`。
  - `migrate` one-shot service。
  - `app` / `agentdash-cloud`。
  - `reverse-proxy`。
- 明确 reverse proxy 的选择和路径约定。
- 维护 upgrade runbook。
- 维护 backup / restore runbook。
- 维护 Compose 到 Kubernetes 的映射说明。

## Acceptance Criteria

- [x] Compose 基准能表达 cloud app、PostgreSQL、migration、reverse proxy 的关系。
- [x] `.env.example` 覆盖 public origin、database URL、secret、日志等核心配置。
- [x] upgrade runbook 包含 pull、backup、migrate、restart、health/version check。
- [x] rollback / restore 边界写清楚。
- [x] K8s 映射保持在资源级别，不实现完整 Helm chart。
- [x] 不实现 `/api/version` 或 discovery endpoint。
- [x] 不修改桌面端连接服务器流程。

## Current Scaffold

- 已新增 `deploy/compose/docker-compose.yml`。
- 已新增 `deploy/compose/.env.example`。
- 已新增 `deploy/compose/reverse-proxy/Caddyfile`。
- 已新增 `deploy/docker/Dockerfile.cloud` 与 `pnpm run docker:cloud:build`。
- 已新增 `deploy/runbooks/backup-restore.md`。
- 已验证 `docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env config`。
- Compose 当前消费目标镜像 `agentdash-cloud:${AGENTDASH_VERSION}`。

## Dependencies

- 依赖云端发布原语子任务确认 `migrate`、`doctor`、`/api/version` 的最终契约。
- 在契约未落定前，可以先以文档中的目标命令编写 Compose 草案。

## Open Questions

- reverse proxy 首选 Caddy、Nginx 还是只提供通用接入样例。
- 第一版是否要求 Compose 本地真正跑通，还是先形成 deploy scaffold。
- Web Dashboard 静态资源由 `agentdash-server` 托管还是由 reverse proxy 托管。
