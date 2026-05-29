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
AGENTDASH_PUBLIC_ORIGIN=https://agentdash.example.internal
AGENTDASH_BIND_HOST=0.0.0.0
AGENTDASH_PORT=3001
DATABASE_URL=postgres://agentdash:change-me@postgres:5432/agentdash
AGENTDASH_SECRET_KEY=change-me
AGENTDASH_ENCRYPTION_KEY=change-me
RUST_LOG=info
```

`AGENTDASH_PUBLIC_ORIGIN` 是部署事实源，API URL、Relay WebSocket URL 和桌面端 discovery response 都从它派生。默认 Relay WebSocket path 为 `/ws/backend`；如果反代暴露不同路径，使用 `AGENTDASH_RELAY_WS_URL` 覆盖。

## 验证入口

Compose scaffold 落地后，最小验证命令为：

```bash
docker compose config
docker compose run --rm migrate
docker compose up -d agentdash-cloud reverse-proxy
```

升级 runbook 统一记录在 `deploy/runbooks/release-workflow.md`。
