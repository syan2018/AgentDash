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
| Desktop update endpoint | `GET /api/desktop/update` |
| Tauri updater endpoint | `GET /api/desktop/update/tauri` |
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
| `AGENTDASH_SECRET_KEY` | 服务端 LLM Provider secret 加密主密钥；必须是 32 字节原文或 32 字节 key 的 base64 表示 |
| `AGENTDASH_WEB_DIST_DIR` | Web Dashboard static assets 目录；存在时由 `agentdash-server` 托管 SPA |
| `AGENTDASH_RELAY_WS_URL` | Relay WebSocket 公开地址覆盖值 |
| `AGENTDASH_DESKTOP_STABLE_MANIFEST_URL` | stable channel 桌面 `latest.json` 的 HTTP URL；服务端运行期读取 |
| `AGENTDASH_DESKTOP_MANIFEST_CACHE_TTL_SECONDS` | stable manifest 短缓存秒数；默认 60 |
| `AGENTDASH_MIN_DESKTOP_VERSION` | 服务器声明的最低桌面端版本 |
| `AGENTDASH_RECOMMENDED_DESKTOP_VERSION` | 服务器声明的推荐桌面端版本 |
| `AGENTDASH_RELAY_PROTOCOL_VERSION` | Relay 协议版本 |
| `RUST_LOG` | 云端后端日志过滤 |

## Contract

Compose 和 Kubernetes 使用同一个 cloud image。`serve` 是长驻云端服务入口，`migrate` 是升级流程中的一次性 migration 入口，`doctor` 用于升级后和排障时检查 PostgreSQL 连接与 schema readiness。

Web Dashboard 在云端部署中仍通过 HTTP API 访问业务数据。cloud image 将前端静态产物放入 `AGENTDASH_WEB_DIST_DIR`，由 `agentdash-server` 托管页面入口；API、Relay 和 discovery 继续由同一个服务进程暴露。

部署期数据库是外部 PostgreSQL。Migration 是发布链路的一等步骤，应用启动时的 schema readiness check 用于确认当前服务看到的数据库结构满足运行要求。

`/.well-known/agentdash` 是桌面端识别企业/云端服务器的发现入口。响应中的 public origin、API base URL、Relay WebSocket URL、server version、桌面端版本要求和 Relay 协议版本必须来自部署配置或 release metadata，而不是桌面端本地推断。

## Scenario: Desktop Stable Update Manifest

### 1. Scope / Trigger

- Trigger: 云端需要向 Tauri 桌面端暴露 stable channel 最新版本、下载 URL、签名和最低版本策略，同时桌面端发布节奏独立于云端服务端发布节奏。
- Scope: `agentdash-api` release info routes、`agentdash-contracts::desktop_release`、部署环境变量、对象存储/CDN 上的 stable `latest.json`。

### 2. Signatures

```text
GET /api/desktop/update?platform=windows&arch=x86_64&current_version=0.1.0
GET /api/desktop/update/tauri?platform=windows&arch=x86_64&current_version=0.1.0

AGENTDASH_DESKTOP_STABLE_MANIFEST_URL=https://updates.example/channels/stable/latest.json
AGENTDASH_DESKTOP_MANIFEST_CACHE_TTL_SECONDS=60
AGENTDASH_MIN_DESKTOP_VERSION=0.1.0
AGENTDASH_RECOMMENDED_DESKTOP_VERSION=0.2.0
```

Stable manifest shape:

```json
{
  "product": "AgentDash",
  "version": "0.2.0",
  "channel": "stable",
  "published_at": "2026-07-06T00:00:00Z",
  "release_notes": "Release notes",
  "platforms": {
    "windows-x86_64": {
      "platform": "windows",
      "arch": "x86_64",
      "updater": {
        "public_url": "https://updates.example/releases/0.2.0/AgentDash_0.2.0_x64.nsis.zip",
        "sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "signature": "minisign-signature"
      },
      "installer": {
        "public_url": "https://updates.example/releases/0.2.0/AgentDash_0.2.0_x64-setup.exe",
        "sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
      }
    }
  }
}
```

### 3. Contracts

- `AGENTDASH_DESKTOP_STABLE_MANIFEST_URL` 是运行期配置；cloud image 或 server binary 不内嵌桌面 `latest.json`，原因是桌面端 release 可以独立推进 stable 指针。
- 服务端读取 stable manifest 使用 HTTP URL 和短缓存，不持有对象存储 AK/SK，也不读取 bucket listing，原因是对象存储凭据属于部署/发布流程而不是 runtime API。
- `GET /api/desktop/update` 返回 generated `DesktopUpdateCheckResponse`，包含 product update 状态、latest release、policy 和 diagnostics，供桌面 UI 与强制更新 gate 消费。
- `GET /api/desktop/update/tauri` 返回 Tauri updater 原生 JSON：`version`、`notes`、`pub_date`、`platforms[target].url/signature`；当没有可安装更新或 manifest 不可用时返回 `204 No Content`，原因是 Tauri updater 只需要可安装更新，产品诊断由 `/api/desktop/update` 承载。
- `AGENTDASH_MIN_DESKTOP_VERSION` 是强制更新唯一事实源。未显式配置时不从 server version、recommended version 或 manifest latest version 推导最低版本。
- `recommended_desktop_version` 在 update endpoint 中优先读取 `AGENTDASH_RECOMMENDED_DESKTOP_VERSION`，否则使用 stable manifest version；discovery 只返回显式配置值，原因是 discovery 不应引入 manifest 拉取依赖。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `AGENTDASH_DESKTOP_STABLE_MANIFEST_URL` 未配置 | `/api/desktop/update` 返回 `status="unconfigured"` 和诊断；`/api/desktop/update/tauri` 返回 204 |
| manifest HTTP 拉取失败 | product endpoint 返回 `status="fetch_failed"`；Tauri endpoint 返回 204 |
| manifest JSON 或 schema 无效 | product endpoint 返回 `status="invalid_manifest"`；Tauri endpoint 返回 204 |
| manifest 缺少请求的 `platform-arch` | product endpoint 返回 `status="unsupported_target"`；Tauri endpoint 返回 204 |
| `current_version` 低于 manifest version | product endpoint `update_available=true`；Tauri endpoint 返回可安装 release |
| `current_version` 等于或高于 manifest version | product endpoint `update_available=false`；Tauri endpoint 返回 204 |
| `AGENTDASH_MIN_DESKTOP_VERSION` 未配置 | `policy.min_desktop_version_configured=false`，客户端不得强制阻断 |

### 5. Good/Base/Bad Cases

- Good: 私有发布流程只更新对象存储 `channels/stable/latest.json`，云端短缓存过期后 `/api/desktop/update` 返回新桌面版本。
- Good: 本地 `pnpm dev` 没有配置 stable manifest URL，桌面端仍可进入 Dashboard 和 runtime 开发链路。
- Base: stable manifest 可用但当前平台尚未发布，桌面 UI 展示 unsupported diagnostic，不启动对象存储 list 查询。
- Bad: 把对象存储 endpoint、bucket 或 AK/SK 写入主仓配置或 server runtime env。

### 6. Tests Required

- `cargo test -p agentdash-api release_info --lib` 覆盖 unconfigured、fetch failed、invalid manifest、success mapping、cache 和 Tauri endpoint schema。
- `pnpm run contracts:check` 覆盖 `DesktopUpdateCheckResponse` generated TS drift。
- 发布脚本测试覆盖 stable manifest 与 Tauri updater artifact signature 字段。

### 7. Wrong vs Correct

#### Wrong

```text
cloud image build -> embed channels/stable/latest.json
desktop app -> read object storage latest.json directly
```

#### Correct

```text
private release job -> upload versioned artifacts -> update channels/stable/latest.json
agentdash-server runtime -> GET AGENTDASH_DESKTOP_STABLE_MANIFEST_URL
desktop app -> GET /api/desktop/update and /api/desktop/update/tauri
```

## Scenario: Compose Release Update

### 1. Scope / Trigger

- Trigger: 修改 Compose 部署、版本更新、registry image、managed PostgreSQL 或 CI artifact 产出。
- Scope: `deploy/compose/docker-compose.yml`、managed PostgreSQL override、update script、release workflow、cloud image workflow。

### 2. Signatures

```text
AGENTDASH_IMAGE_REPOSITORY=agentdash-cloud
AGENTDASH_VERSION=0.1.0
pnpm run deploy:compose:update -- --env-file deploy/compose/.env --version <version>
pnpm run deploy:compose:update -- --env-file deploy/compose/.env --version <version> --managed-postgres --skip-backup
```

### 3. Contracts

- Compose image 必须由 `AGENTDASH_IMAGE_REPOSITORY` 与 `AGENTDASH_VERSION` 共同决定。
- 默认 Compose 文件保留内置 `postgres` 服务，并让 `migrate` 等待其 healthy。
- managed PostgreSQL override 必须让 `migrate` 不依赖 Compose 内置 `postgres`。
- update script 的执行顺序为 config、pull、backup、migrate、up、health、version、doctor。
- Cloud image CI 由 release tag 或手动 dispatch 表达发布意图，只构建 image / metadata artifact，不执行远端部署。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| 默认 Compose config | `migrate` depends on `postgres: service_healthy` |
| managed PostgreSQL config | `migrate` 无 `postgres` depends_on |
| managed PostgreSQL update 未传 `--skip-backup` | 脚本失败并要求先完成外部数据库快照 |
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
