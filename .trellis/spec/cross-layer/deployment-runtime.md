# Deployment Runtime Contract

## Scope

本文记录 AgentDash 云端部署运行契约。它约束 Docker Compose、Kubernetes、Web Dashboard、桌面端 discovery 和云端后端之间共同消费的运行事实。

## Signatures

| 类型 | 契约 |
| --- | --- |
| Cloud image | `${AGENTDASH_IMAGE_REPOSITORY:-agentdash-cloud}:${AGENTDASH_VERSION}` |
| Image entrypoint | `agentdash-server` |
| Server commands | `serve` / `migrate` / `doctor` |
| Health endpoint | `GET /api/health` |
| Version endpoint | `GET /api/version` |
| Discovery endpoint | `GET /.well-known/agentdash` |
| Relay endpoint | `GET /ws/backend` |

## Environment Contract

| 变量 | 作用 |
| --- | --- |
| `AGENTDASH_IMAGE_REPOSITORY` | cloud image repository；本地默认 `agentdash-cloud`，远端部署使用 registry repository |
| `AGENTDASH_VERSION` | cloud image tag 与部署目标版本 |
| `AGENTDASH_PUBLIC_ORIGIN` | 部署公共入口，作为 API URL、Relay WebSocket URL 和 discovery response 的事实源 |
| `AGENTDASH_BIND_HOST` | 云端服务监听地址 |
| `AGENTDASH_PORT` | 云端服务监听端口 |
| `DATABASE_URL` | 部署期外部 PostgreSQL 连接串 |
| `AGENTDASH_SECRET_KEY` | 服务端签名或会话类 secret |
| `AGENTDASH_ENCRYPTION_KEY` | 服务端加密类 secret |
| `AGENTDASH_WEB_DIST_DIR` | Web Dashboard static assets 目录；存在时由 `agentdash-server` 托管 SPA |
| `AGENTDASH_RELAY_WS_URL` | Relay WebSocket 公开地址覆盖值 |
| `AGENTDASH_MIN_DESKTOP_VERSION` | 服务器声明的最低桌面端版本 |
| `AGENTDASH_RECOMMENDED_DESKTOP_VERSION` | 服务器声明的推荐桌面端版本 |
| `AGENTDASH_RELAY_PROTOCOL_VERSION` | Relay 协议版本 |
| `RUST_LOG` | 云端后端日志过滤 |

## Contract

Compose 和 Kubernetes 使用同一个 cloud image。`serve` 是长驻云端服务入口，`migrate` 是升级流程中的一次性 migration 入口，`doctor` 用于升级后和排障时检查 PostgreSQL 连接与 schema readiness。

Web Dashboard 在云端部署中仍通过 HTTP API 访问业务数据。cloud image 将前端静态产物放入 `AGENTDASH_WEB_DIST_DIR`，由 `agentdash-server` 托管页面入口；API、Relay 和 discovery 继续由同一个服务进程暴露。

部署期数据库是外部 PostgreSQL。Migration 是发布链路的一等步骤，应用启动时的 schema readiness check 用于确认当前服务看到的数据库结构满足运行要求。

`/.well-known/agentdash` 是桌面端识别企业/云端服务器的发现入口。响应中的 public origin、API base URL、Relay WebSocket URL、server version、桌面端版本要求和 Relay 协议版本必须来自部署配置或 release metadata，而不是桌面端本地推断。

## Scenario: Compose Release Update

### 1. Scope / Trigger

- Trigger: 修改 Compose 部署、版本更新、registry image、managed PostgreSQL 或 CI artifact 产出。
- Scope: `deploy/compose/docker-compose.yml`、managed PostgreSQL override、update script、release workflow、cloud image workflow。

### 2. Signatures

```text
AGENTDASH_IMAGE_REPOSITORY=agentdash-cloud
AGENTDASH_VERSION=0.1.0
pnpm run deploy:compose:update -- -EnvFile deploy/compose/.env -Version <version>
pnpm run deploy:compose:update -- -EnvFile deploy/compose/.env -Version <version> -ManagedPostgres -SkipBackup
```

### 3. Contracts

- Compose image 必须由 `AGENTDASH_IMAGE_REPOSITORY` 与 `AGENTDASH_VERSION` 共同决定。
- 默认 Compose 文件保留内置 `postgres` 服务，并让 `migrate` 等待其 healthy。
- managed PostgreSQL override 必须让 `migrate` 不依赖 Compose 内置 `postgres`。
- update script 的执行顺序为 config、pull、backup、migrate、up、health、version、doctor。
- CI workflow 只构建 image / metadata artifact，不执行远端部署。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| 默认 Compose config | `migrate` depends on `postgres: service_healthy` |
| managed PostgreSQL config | `migrate` 无 `postgres` depends_on |
| managed PostgreSQL update 未传 `-SkipBackup` | 脚本失败并要求先完成外部数据库快照 |
| `AGENTDASH_VERSION` 缺失 | Compose / update script 失败 |

### 5. Good/Base/Bad Cases

- Good: registry image 通过 `AGENTDASH_IMAGE_REPOSITORY=ghcr.io/<owner>/agentdash-cloud` 和 `AGENTDASH_VERSION=0.2.0` 部署。
- Base: 本地预研用默认 `agentdash-cloud:0.1.0` 与 Compose 内置 PostgreSQL。
- Bad: CI/CD 直接改写 Compose 文件里的 image 字符串，导致部署目标版本无法由环境事实追踪。

### 6. Tests Required

- `docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env.example config`
- `docker compose -f deploy/compose/docker-compose.yml -f deploy/compose/docker-compose.managed-postgres.yml --env-file deploy/compose/.env.example config`
- update script dry-run 覆盖默认与 managed PostgreSQL 模式。

### 7. Wrong vs Correct

#### Wrong

```yaml
image: ghcr.io/example/agentdash-cloud:0.2.0
```

#### Correct

```yaml
image: ${AGENTDASH_IMAGE_REPOSITORY:-agentdash-cloud}:${AGENTDASH_VERSION:?AGENTDASH_VERSION is required}
```
