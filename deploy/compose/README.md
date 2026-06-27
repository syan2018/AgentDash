# Docker Compose 基准部署

`deploy/compose/` 维护 AgentDash 第一交付形态。Compose 方案用于预研内测和小规模部署，同时作为 Kubernetes 资源映射的源模型。

## 目标服务

| 服务 | 职责 | Kubernetes 映射 |
| --- | --- | --- |
| `postgres` | 云端业务数据库 | managed PostgreSQL / StatefulSet |
| `migrate` | 使用 cloud image 执行一次性 migration | Job / Helm hook |
| `agentdash-cloud` | API、Web、Relay endpoint | Deployment |
| `reverse-proxy` | HTTPS、同源路径转发、WebSocket upgrade | Ingress |

## 配置入口

目标 `.env.example` 至少覆盖：

```env
AGENTDASH_IMAGE_REPOSITORY=agentdash-cloud
AGENTDASH_VERSION=0.1.0
AGENTDASH_PUBLIC_ORIGIN=https://agentdash.example.internal
AGENTDASH_BIND_HOST=0.0.0.0
AGENTDASH_PORT=3001
DATABASE_URL=postgres://agentdash:change-me@postgres:5432/agentdash
AGENTDASH_SECRET_KEY=0123456789abcdef0123456789abcdef
RUST_LOG=info
```

`AGENTDASH_PUBLIC_ORIGIN` 是部署事实源，API URL、Relay WebSocket URL 和桌面端 discovery response 都从它派生。默认 Relay WebSocket path 为 `/ws/backend`；如果反代暴露不同路径，使用 `AGENTDASH_RELAY_WS_URL` 覆盖。

`AGENTDASH_SECRET_KEY` 是服务端 LLM Provider secret 加密主密钥，必须是 32 字节原文或 32 字节 key 的 base64 表示。

`AGENTDASH_IMAGE_REPOSITORY` 与 `AGENTDASH_VERSION` 共同决定 `migrate` 和 `agentdash-cloud` 使用的 cloud image。默认值服务本地验证；远端部署使用 registry repository，例如 `ghcr.io/<owner>/agentdash-cloud`。

## PostgreSQL 形态

默认 Compose 文件包含 `postgres` 服务，适合单机部署和预研验证：

```bash
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env config
```

连接 managed PostgreSQL 时，`DATABASE_URL` 指向外部数据库，并追加 override：

```bash
docker compose \
  -f deploy/compose/docker-compose.yml \
  -f deploy/compose/docker-compose.managed-postgres.yml \
  --env-file deploy/compose/.env \
  config
```

managed PostgreSQL 模式下，数据库备份由外部数据库快照或托管平台备份承担；Compose 更新脚本要求显式传入 `--skip-backup`。

## 验证入口

Compose scaffold 落地后，最小验证命令为：

```bash
cp .env.example .env
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env config
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env run --rm migrate
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env up -d agentdash-cloud reverse-proxy
```

当前 Compose 使用 `${AGENTDASH_IMAGE_REPOSITORY:-agentdash-cloud}:${AGENTDASH_VERSION}` 作为目标镜像，镜像构建入口为：

```bash
pnpm run docker:cloud:build
```

版本更新入口：

```bash
pnpm run deploy:compose:update:dry-run -- --env-file deploy/compose/.env
pnpm run deploy:compose:update -- --env-file deploy/compose/.env --version 0.2.0
```

升级 runbook 记录在 `deploy/runbooks/release-workflow.md`，备份与恢复 runbook 记录在 `deploy/runbooks/backup-restore.md`。
