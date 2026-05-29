# AgentDash 部署基准方案

本文维护 AgentDash 产品化部署预研期间形成的部署基准、运行模型和后续验证项。当前阶段聚焦云端项目的部署、维护、更新，以及桌面端如何指向目标服务器并正确发布。

## 目标

AgentDash 的基准部署方案以 Docker Compose 为第一交付形态，并保持向 Kubernetes 平滑映射的运行模型。Compose 方案需要能够支撑预研期内测、小规模稳定部署和后续部署脚本打磨；Kubernetes 方案在此基础上扩展为 Deployment、Job、Ingress、Secret、ConfigMap 等资源，而不是重写一套运行语义。

部署基准需要同时覆盖两条链路：

- 云端控制面：`agentdash-server`、Web Dashboard、Relay WebSocket endpoint、PostgreSQL、migration、健康检查、版本信息、备份恢复。
- 桌面端与本机 runtime：桌面端默认服务器配置、运行时服务器切换、server discovery、版本兼容检查、本机 runtime 管理和发布包分发。

## 基准部署拓扑

第一版部署拓扑采用单机 Docker Compose：

```text
reverse-proxy
  -> agentdash-cloud
       - agentdash-server
       - Web Dashboard static assets
       - Relay WebSocket endpoint
  -> postgres
```

云端访问优先采用同源模型：

```text
https://agentdash.example.internal/
https://agentdash.example.internal/api/...
https://agentdash.example.internal/relay/...
```

同源模型可以让 Web Dashboard、API、Relay WebSocket、桌面端配置和后续登录回调共享同一个公共入口，降低部署期对 CORS、cookie、WebSocket 反代和公网地址推导的配置成本。

## 云端镜像

基准交付产物建议收敛为一个云端应用镜像：

```text
agentdash-cloud:<version>
```

该镜像包含：

- `agentdash-server` release binary。
- `app-web` 构建后的静态资源。
- 数据库 migration 所需资源。
- 用于健康诊断和版本输出的运行入口。

建议云端二进制逐步形成以下命令：

```bash
agentdash-server serve
agentdash-server migrate
agentdash-server doctor
```

这组命令可以同时服务 Compose 和 Kubernetes：

| 运行意图 | Docker Compose | Kubernetes |
| --- | --- | --- |
| 启动云端服务 | long-running service | Deployment |
| 执行 migration | one-shot service | Job / Helm hook |
| 健康诊断 | exec / run command | probe / debug job |

## 配置契约

部署配置应围绕公共入口和数据库连接建立稳定契约。开发默认值可以继续服务本地调试，部署形态需要显式提供生产配置。

建议配置项：

```env
AGENTDASH_PUBLIC_ORIGIN=https://agentdash.example.internal
AGENTDASH_BIND_HOST=0.0.0.0
AGENTDASH_PORT=3001

DATABASE_URL=postgres://agentdash:change-me@postgres:5432/agentdash

AGENTDASH_SECRET_KEY=change-me
AGENTDASH_ENCRYPTION_KEY=change-me

RUST_LOG=info
```

`AGENTDASH_PUBLIC_ORIGIN` 是部署入口的核心配置。API base URL、Relay WebSocket URL、桌面端 discovery 返回值和外部访问地址都应优先围绕它推导，避免在多个位置重复维护 origin。

## Migration 策略

数据库 migration 是云端更新流程的一等步骤。当前应用启动时已经会运行 PostgreSQL migration 和 schema readiness 检查；产品化部署需要进一步支持显式 migration 步骤。

基准流程：

```text
pull image
backup database
run migration
start new app version
health check
version check
```

Compose 中可以建模为：

```yaml
services:
  migrate:
    image: agentdash-cloud:${AGENTDASH_VERSION}
    command: ["agentdash-server", "migrate"]
    depends_on:
      postgres:
        condition: service_healthy
    env_file:
      - .env
    restart: "no"

  app:
    image: agentdash-cloud:${AGENTDASH_VERSION}
    command: ["agentdash-server", "serve"]
    depends_on:
      migrate:
        condition: service_completed_successfully
    env_file:
      - .env
```

应用启动时的 schema readiness check 仍然有价值，它用于确认当前服务看到的数据库结构满足运行要求。部署流程中的显式 migration 则用于把升级步骤变成可观察、可重试、可独立排查的操作。

## 更新与回滚

Compose 基准更新流程：

```bash
docker compose pull
./backup.sh
docker compose run --rm migrate
docker compose up -d app reverse-proxy
docker compose exec app agentdash-server doctor
```

后续可以提供包装脚本：

```bash
./deploy.sh update --version 0.2.3
```

包装脚本应负责：

- 检查当前运行版本。
- 拉取目标版本镜像。
- 执行数据库备份。
- 运行 migration。
- 启动新版本服务。
- 检查 `/api/health`。
- 检查 `/api/version`。
- 在失败时输出恢复路径。

回滚策略以备份恢复为基准。应用镜像可以在 schema 兼容范围内回滚；涉及 schema 变更的版本回退应依赖升级前数据库备份恢复。

## 版本与发现端点

云端需要暴露稳定版本信息，便于部署维护和桌面端兼容检查。

建议增加：

```http
GET /api/version
GET /.well-known/agentdash
```

`/api/version` 示例：

```json
{
  "version": "0.2.3",
  "git_sha": "abc1234",
  "build_time": "2026-05-29T12:00:00Z",
  "schema_version": 67
}
```

`/.well-known/agentdash` 示例：

```json
{
  "product": "AgentDash",
  "public_origin": "https://agentdash.example.internal",
  "api_base_url": "https://agentdash.example.internal/api",
  "relay_ws_url": "wss://agentdash.example.internal/relay/ws",
  "server_version": "0.2.3",
  "min_desktop_version": "0.2.0",
  "recommended_desktop_version": "0.2.3",
  "relay_protocol_version": "3"
}
```

Discovery endpoint 让桌面端只需要知道一个服务器入口 URL。API base URL、Relay WebSocket URL、版本约束和推荐升级信息都由服务器返回。

## 桌面端服务器指向

桌面端发布包可以带默认服务器地址，但运行时必须允许配置和持久化目标服务器。构建时配置应作为默认值，而不是把发布包绑定到唯一服务器。

首次启动建议流程：

```text
读取本机保存的 server origin
没有保存时使用构建时 default origin
仍没有时显示服务器地址输入页
请求 /.well-known/agentdash
校验产品标识和版本兼容性
保存 server origin
进入登录或设备配对流程
```

桌面端连接服务器后需要同时管理本机 runtime：

- 启动或停止 local runtime。
- 读取和维护 machine identity。
- 完成服务器配对。
- 上报 runtime health。
- 管理 workspace roots。
- 展示服务器连接和本机 runtime 状态。

## 桌面端发布包

建议先维护两类桌面端发布包：

| 发布包 | 用途 | 服务器配置 |
| --- | --- | --- |
| 通用包 | 通用下载、未知部署环境 | 首次启动输入服务器地址 |
| 预配置包 | 特定部署环境分发 | 构建时写入默认服务器地址，运行时仍可修改 |

预配置包可以继续复用当前构建参数：

```bash
pnpm run desktop:bundle -- --api-mode external --api-origin https://agentdash.example.internal
```

`external` 在发布语义上表示默认连接外部服务器。桌面端运行时仍应允许用户或管理员切换服务器，并通过 discovery endpoint 验证目标服务器。

`builtin` 适合作为个人本机体验、开发调试或演示包形态。连接已部署服务器的桌面发布包应以 `external` 作为默认模式。

## 版本兼容

桌面端发布后会自然产生 server、desktop、local runtime 的版本错配。产品化部署需要明确兼容策略。

建议规则：

```text
同一 minor 版本内保持桌面端与服务器协议兼容。
跨 minor 版本允许服务器要求桌面端升级。
服务器通过 discovery endpoint 暴露 min_desktop_version 和 recommended_desktop_version。
桌面端连接前执行兼容检查。
```

桌面端行为：

| 检查结果 | 行为 |
| --- | --- |
| 满足最低版本 | 正常连接 |
| 低于最低版本 | 阻止连接并提示升级 |
| 满足最低版本但低于推荐版本 | 允许连接并提示升级 |
| 服务器不可达 | 保留服务器配置并显示连接失败状态 |

云端和桌面端可以使用同一个产品版本号发布：

```text
AgentDash 0.3.0
  - cloud image: agentdash-cloud:0.3.0
  - desktop installer: AgentDash_0.3.0_x64-setup.exe
  - local runtime: agentdash-local 0.3.0
```

Release notes 需要注明版本矩阵：

```text
Server 0.3.1
Compatible Desktop: >=0.3.0 <0.4.0
Compatible Local Runtime: >=0.3.0 <0.4.0
Schema: 68
```

## Kubernetes 映射

Kubernetes 方案应继承 Compose 运行模型：

| Docker Compose | Kubernetes |
| --- | --- |
| `postgres` service | managed PostgreSQL / StatefulSet |
| `migrate` one-shot service | Job / Helm hook |
| `app` service | Deployment |
| `reverse-proxy` | Ingress |
| `.env` | ConfigMap + Secret |
| volume | PVC / external storage |
| healthcheck | readiness / liveness probe |
| `docker compose pull && up` | Helm upgrade |

后续 Kubernetes 最小资源：

```text
Deployment/agentdash-cloud
Service/agentdash-cloud
Ingress/agentdash
Job/agentdash-migrate
Secret/agentdash-secrets
ConfigMap/agentdash-config
```

Compose 阶段需要保持配置、migration 和健康检查语义清晰，以便后续映射到 Kubernetes 时只转换承载形式。

## 运维清单

第一版部署维护需要补齐以下能力：

- `deploy/compose/docker-compose.yml`。
- `deploy/compose/.env.example`。
- 云端 release Dockerfile。
- `agentdash-server migrate`。
- `agentdash-server doctor`。
- `/api/health` 健康检查。
- `/api/version` 版本信息。
- `/.well-known/agentdash` discovery。
- PostgreSQL backup / restore 脚本。
- Compose upgrade runbook。
- Compose restore runbook。
- 桌面端服务器配置说明。
- 桌面端与服务器版本兼容矩阵。
- Release checklist。

## 待验证项

- 当前 `agentdash-server` 是否应直接托管 `app-web` 静态资源，还是由 reverse proxy 托管静态文件。
- `HOST`、`PORT`、`DATABASE_URL` 与建议的 `AGENTDASH_*` 部署配置如何收敛命名。
- `PostgresRuntime::resolve` 在部署形态下如何明确禁用开发期 embedded PostgreSQL。
- migration 是否需要从启动流程中拆成可独立调用的子命令。
- desktop `external` API mode 如何从构建时默认值演进为运行时可配置服务器。
- discovery endpoint 的路径、字段和版本兼容语义。
- local runtime 与 desktop shell 的发布边界：独立 headless runtime、桌面托管 runtime、sidecar runtime 的关系。
- Compose 中是否需要内置 reverse proxy，或只提供 app/postgres 并让部署方接入已有网关。

## 并行推进拆分

当前部署基准任务作为父任务维护整体方向，后续由三个子任务并行推进：

| 子任务 | 处理方 | 产出 |
| --- | --- | --- |
| `.trellis/tasks/05-29-deployment-cloud-primitives` | 后端/部署 Agent | 云端版本信息、discovery、server command、配置契约 |
| `.trellis/tasks/05-29-deployment-compose-runbook` | 部署/DevOps Agent | Dockerfile、Compose、`.env.example`、upgrade/backup/restore runbook、K8s 映射 |
| `.trellis/tasks/05-29-deployment-desktop-targeting` | 桌面/前端 Agent | 桌面端服务器配置、discovery/compatibility check、通用包和预配置包发布 |

父任务继续维护本文档和跨子任务契约。子任务之间的关键依赖是：

- Compose 方案依赖云端子任务确认 `migrate`、`doctor`、`/api/version` 的契约。
- 桌面端方案依赖云端子任务确认 `/.well-known/agentdash` 字段。
- Compose 和桌面端可以先按本文档中的目标契约做草案，最终字段以云端子任务落定后的契约为准。

## 仓库部署环境与发布链路

仓库级部署环境与发布链路由父任务直接维护，不再拆成独立子任务。它横跨 cloud、Compose、desktop 和后续 Kubernetes，是整个部署基准的集成轴。

父任务需要维护：

- release build 入口：后端 release binary、Web Dashboard static build、desktop installer。
- cloud image build 入口：Dockerfile 位置、image tag 规则、build args / version args。
- 版本注入：server version、git SHA、build time、desktop version、relay protocol version。
- `deploy/` 目录规划：`deploy/compose/`、`deploy/docker/`、`deploy/k8s/`、runbook 位置。
- CI / release workflow 草案：check、build cloud image、build desktop installer、publish artifacts、release notes、compatibility matrix。

三个子任务消费这条父任务链路：

- 云端子任务负责让 server 暴露版本和 discovery 契约。
- Compose 子任务负责把 cloud image 和 server command 组织成可运行部署。
- 桌面子任务负责把桌面产物连接到目标服务器和版本兼容策略。

### 当前目录骨架

部署链路入口位于 `deploy/`：

| 路径 | 职责 |
| --- | --- |
| `deploy/README.md` | 部署入口、发布产物、版本字段、基准发布顺序 |
| `deploy/docker/README.md` | cloud image 内容、运行命令、镜像标签和后续 Dockerfile 落点 |
| `deploy/compose/README.md` | Compose 服务角色、配置入口和验证命令 |
| `deploy/k8s/README.md` | Kubernetes 资源映射草案 |
| `deploy/runbooks/release-workflow.md` | artifact inventory、release metadata、发布和升级流程 |

### 发布产物清单

| 产物 | 当前构建入口 | 目标用途 |
| --- | --- | --- |
| `agentdash-server` release binary | `pnpm run backend:build` | 云端 API、Relay endpoint、migration / doctor 命令承载体 |
| Web Dashboard static assets | `pnpm run frontend:build` | 云端 Web 入口 |
| `agentdash-cloud:<version>` image | 待新增 `deploy/docker` 构建入口 | Compose / Kubernetes 共用云端镜像 |
| Windows desktop installer | `pnpm run desktop:bundle` | 桌面端通用包或预配置包 |

### 版本字段来源

| 字段 | 来源 | 消费方 |
| --- | --- | --- |
| `version` | 根 `package.json` 与 Cargo workspace version 对齐 | cloud image tag、server version、desktop release |
| `git_sha` | 发布时的 Git commit | `/api/version`、release notes、排障 |
| `build_time` | 发布流水线生成 | `/api/version`、artifact manifest |
| `relay_protocol_version` | 云端/本机 relay 契约版本 | discovery endpoint、desktop compatibility check |
| `desktop_version` | 桌面构建产物版本 | installer、compatibility matrix |

当前仓库提供发布元数据入口：

```bash
pnpm run release:metadata
pnpm run release:metadata -- --out dist/release/agentdash-release.json
```

该命令从根 `package.json`、Cargo workspace metadata 和当前 Git commit 生成 artifact manifest。后续云端版本端点、镜像标签和 release notes 都围绕这份元数据继续收敛。
