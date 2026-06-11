# AgentDash 部署学习指南

本文是一份回看用的教学路线，用于理解 AgentDash 从开发态走向产品化部署态时需要掌握的运行模型、发布产物、维护动作和验证顺序。

学习目标不是记住几条 Docker 命令，而是建立一套稳定判断：云端服务如何交付、桌面端如何发现服务器、数据库 schema 如何升级、出现问题时如何诊断和恢复。

## 学习目标

完成这套流程后，应能解释：

- AgentDash 云端部署由哪些产物组成。
- 为什么基准部署先使用 Docker Compose，并保持向 Kubernetes 映射。
- `agentdash-cloud:<version>` 镜像包含什么。
- `serve`、`migrate`、`doctor` 三个命令分别承担什么运维职责。
- 桌面端为什么通过 `/.well-known/agentdash` 获取服务器发现信息。
- 升级、备份、恢复、回滚分别发生在什么边界上。
- Kubernetes 方案如何继承 Compose 中已经验证过的运行语义。

## 前置阅读

先按顺序阅读这些文档，形成部署地图：

| 文档 | 阅读目的 |
| --- | --- |
| [部署基准方案](./deployment-baseline.md) | 理解整体目标、拓扑、配置、版本、桌面端连接和任务拆分 |
| [部署入口](../../deploy/README.md) | 理解 `deploy/` 目录和发布产物 |
| [Compose 基准部署](../../deploy/compose/README.md) | 理解第一交付形态的服务关系 |
| [Cloud Image 构建入口](../../deploy/docker/README.md) | 理解云端镜像内容、tag 和 build args |
| [发布链路 Runbook](../../deploy/runbooks/release-workflow.md) | 理解检查、构建、升级、版本确认的顺序 |
| [备份与恢复 Runbook](../../deploy/runbooks/backup-restore.md) | 理解 PostgreSQL 备份、恢复和回滚边界 |
| [Kubernetes 映射草案](../../deploy/k8s/README.md) | 理解 Compose 角色到 K8s 资源的映射 |
| [Deployment Runtime Contract](../../.trellis/spec/cross-layer/deployment-runtime.md) | 理解跨云端、桌面端和部署工具共享的运行契约 |

## 第 0 课：建立部署心智模型

第一阶段先把系统看成一条完整链路：

```text
Desktop / Browser
      |
      v
reverse-proxy
      |
      v
agentdash-cloud image
  - Web Dashboard static assets
  - agentdash-server serve
  - agentdash-server migrate
  - agentdash-server doctor
      |
      v
PostgreSQL
```

这里的重点是：产品化部署不只是把服务进程跑起来，还要让版本、配置、数据库 schema、健康检查、桌面端发现、升级恢复都成为稳定契约。

本课验收点：

- 能说清浏览器、桌面端、reverse proxy、`agentdash-cloud`、PostgreSQL 的关系。
- 能说清 Web Dashboard、API、Relay WebSocket 为什么共享同一个公共入口。
- 能说清 Compose 是第一交付形态，Kubernetes 是同一运行语义的资源化表达。

## 第 1 课：理解发布产物

当前部署基准涉及四类产物：

| 产物 | 构建入口 | 用途 |
| --- | --- | --- |
| 后端 release binary | `pnpm run backend:build` | 生成 `agentdash-server` |
| Web Dashboard static assets | `pnpm run frontend:build` | 生成云端 Web 静态资源 |
| Cloud image | `pnpm run docker:cloud:build` | Compose / Kubernetes 共用云端镜像 |
| Desktop installer | `pnpm run desktop:bundle` | 桌面端安装包 |

Cloud image 是部署链路里的核心产物。它同时承载云端 API、Relay endpoint、Web Dashboard 静态入口，以及 migration / doctor 运维命令。

练习命令：

```powershell
pnpm run release:metadata
pnpm run docker:cloud:build -- --dry-run
```

观察重点：

- 镜像 tag 是否是 `agentdash-cloud:<version>`。
- build args 是否包含 `AGENTDASH_VERSION`。
- build args 是否包含当前 Git SHA。
- build args 是否包含 `AGENTDASH_BUILD_TIME`。

本课验收点：

- 能解释 release metadata、cloud image tag、`/api/version` 之间的关系。
- 能解释为什么 cloud image 需要同时包含后端二进制和 Web Dashboard 静态资源。

## 第 2 课：理解 Server 命令语义

`agentdash-server` 在部署语义中提供三个命令：

| 命令 | 运行方式 | 职责 |
| --- | --- | --- |
| `serve` | 长驻进程 | 提供 API、Web Dashboard、Relay endpoint |
| `migrate` | 一次性命令 | 执行 PostgreSQL migration，并检查 schema readiness |
| `doctor` | 一次性命令 | 检查 PostgreSQL 连接与 schema readiness，不执行 migration |

这三个命令让升级流程可以拆成可观察、可重试、可诊断的步骤。`serve` 面向线上流量，`migrate` 面向 schema 变更，`doctor` 面向升级后确认和排障。

练习命令：

```powershell
cargo run --bin agentdash-server -- --help
cargo run --bin agentdash-server -- serve
cargo run --bin agentdash-server -- doctor
```

本地缺少部署数据库配置时，`doctor` 可能无法通过；观察它检查的对象和错误位置即可。

本课验收点：

- 能说清 `migrate` 与应用启动时 schema readiness check 的区别。
- 能说清 `doctor` 为什么适合放在升级后检查和排障流程里。

## 第 3 课：理解 Compose 拓扑

第一交付形态由 [docker-compose.yml](../../deploy/compose/docker-compose.yml) 表达：

| 服务 | 职责 |
| --- | --- |
| `postgres` | 云端 PostgreSQL 数据库 |
| `migrate` | 使用 cloud image 执行一次性 migration |
| `agentdash-cloud` | 长驻云端服务 |
| `reverse-proxy` | 同源入口、HTTP 转发、WebSocket upgrade 承载点 |

依赖顺序是：

```text
postgres healthy
   -> migrate completed successfully
      -> agentdash-cloud healthy
         -> reverse-proxy
```

练习命令：

```powershell
Copy-Item deploy/compose/.env.example deploy/compose/.env
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env config
Remove-Item deploy/compose/.env
```

观察重点：

- `AGENTDASH_VERSION` 是否展开到 `agentdash-cloud:<version>`。
- `migrate` service 的 command 是否是 `migrate`。
- `agentdash-cloud` service 的 command 是否是 `serve`。
- `agentdash-cloud` 是否依赖 `migrate` 成功完成。
- `reverse-proxy` 是否依赖 `agentdash-cloud` healthy。

本课验收点：

- 能解释 Compose 里的 service 不是简单并排启动，而是有发布顺序。
- 能解释为什么 migration 被建模为 one-shot service。

## 第 4 课：完整跑通本机 Compose

这一课用于把文档和 scaffold 变成真实运行链路。

构建 cloud image：

```powershell
pnpm run docker:cloud:build -- --tag agentdash-cloud:0.1.0
```

准备配置并启动：

```powershell
Copy-Item deploy/compose/.env.example deploy/compose/.env
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env up -d postgres
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env run --rm migrate
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env up -d agentdash-cloud reverse-proxy
```

检查服务：

```powershell
curl.exe -fsS http://127.0.0.1:8080/api/health
curl.exe -fsS http://127.0.0.1:8080/api/version
curl.exe -fsS http://127.0.0.1:8080/.well-known/agentdash
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env run --rm agentdash-cloud doctor
```

本课验收点：

- `/api/health` 可以返回健康状态。
- `/api/version` 可以返回服务版本、schema version、build metadata。
- `/.well-known/agentdash` 可以返回 public origin、API base URL、Relay WebSocket URL 和桌面端版本要求。
- `doctor` 可以确认数据库连接和 schema readiness。

## 第 5 课：理解 Discovery 与桌面端连接

桌面端连接企业或云端服务器时，核心输入应是服务器公共入口：

```text
https://agentdash.example.internal
```

桌面端随后请求：

```text
GET https://agentdash.example.internal/.well-known/agentdash
```

典型 discovery response：

```json
{
  "product": "agentdash",
  "public_origin": "https://agentdash.example.internal",
  "api_base_url": "https://agentdash.example.internal/api",
  "relay_ws_url": "wss://agentdash.example.internal/ws/backend",
  "server_version": "0.1.0",
  "min_desktop_version": "0.1.0",
  "recommended_desktop_version": "0.1.0",
  "relay_protocol_version": "1"
}
```

这种方式让桌面端只持有服务器入口，由服务器声明 API、Relay、版本兼容和协议版本。域名、HTTPS、反向代理路径和企业部署差异都收敛到 discovery 契约。

本课验收点：

- 能解释桌面端为什么只需要配置服务器 origin。
- 能解释 `api_base_url` 与 `relay_ws_url` 为什么由服务器声明。
- 能解释 `min_desktop_version` 与 `recommended_desktop_version` 如何服务桌面端发布。

## 第 6 课：学习升级流程

Compose 基准升级顺序：

```text
1. 拉取或构建目标版本 cloud image
2. 备份 PostgreSQL
3. 更新 AGENTDASH_VERSION
4. 执行 migrate one-shot service
5. 启动 agentdash-cloud 和 reverse-proxy
6. 检查 /api/health
7. 检查 /api/version
8. 执行 doctor
9. 记录升级结果
```

对应 runbook：

- [发布链路 Runbook](../../deploy/runbooks/release-workflow.md)
- [备份与恢复 Runbook](../../deploy/runbooks/backup-restore.md)

升级记录至少包含：

- cloud image tag。
- Git SHA。
- schema version。
- 备份文件路径。
- `/api/version` 输出。
- `doctor` 结果。

本课验收点：

- 能指出哪一步改变数据库 schema。
- 能指出哪一步验证运行中的服务版本。
- 能指出哪一步验证数据库连接与 schema readiness。

## 第 7 课：学习备份、恢复和回滚边界

备份和恢复围绕 PostgreSQL 进行。应用镜像版本和数据库 schema version 共同决定运行状态，因此恢复时需要匹配备份对应的应用版本。

逻辑备份示例：

```powershell
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env exec -T postgres sh -c 'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" --format=custom --no-owner' > backups/agentdash-<timestamp>.dump
```

回滚边界：

| 场景 | 处理方式 |
| --- | --- |
| 同一个 schema version 内的应用异常 | 切换 `AGENTDASH_VERSION` 并重启服务 |
| 涉及 schema migration 的升级异常 | 恢复升级前数据库备份，再启动与备份匹配的 cloud image |

本课验收点：

- 能解释应用镜像回滚和数据库恢复的区别。
- 能解释为什么 schema migration 后的回滚以备份恢复为准。
- 能说清恢复后需要检查 `/api/version` 与 `doctor`。

## 第 8 课：从 Compose 映射到 Kubernetes

Kubernetes 方案继承 Compose 已验证的运行语义：

| Compose 角色 | Kubernetes 资源 |
| --- | --- |
| `agentdash-cloud` | Deployment + Service |
| `migrate` | Job / Helm hook |
| `reverse-proxy` | Ingress |
| `.env` | ConfigMap + Secret |
| `postgres` | managed PostgreSQL / StatefulSet |

Kubernetes 阶段关注资源表达：副本、滚动更新、Secret 管理、Ingress、Job 生命周期和 probe。运行语义仍来自同一个 cloud image、同一组 server commands、同一组 health/version/discovery endpoints。

本课验收点：

- 能把 Compose 四个服务映射到 K8s 资源。
- 能解释 migration 在 K8s 中为什么适合 Job 或 Helm hook。
- 能解释 ConfigMap / Secret 如何承接 `.env` 中的配置。

## 第 9 课：理解桌面端发布形态

桌面端发布可以分成两类：

| 发布形态 | 说明 |
| --- | --- |
| 通用安装包 | 通过 `pnpm run desktop:bundle` 构建，不绑定具体服务器 |
| 预配置安装包 | 携带默认服务器 origin，启动后仍通过 discovery 获取 API / Relay / 兼容信息 |

桌面端与云端服务器之间的稳定边界是 discovery endpoint。预配置安装包只需要提供默认服务器入口，运行时仍以服务器返回的 discovery response 为准。

本课验收点：

- 能解释通用安装包和预配置安装包的差异。
- 能解释桌面端版本兼容检查由服务器 discovery response 驱动。
- 能解释企业服务器地址变化时，为什么应更新服务器 origin 或预配置，而不是改 API/Relay 硬编码。

## 协作推进路线

多人或多 Agent 协作时，按可独立验收的工作流拆分：

| 方向 | 负责内容 | 验收方式 |
| --- | --- | --- |
| Cloud primitives | `/api/version`、discovery、`serve` / `migrate` / `doctor`、静态托管 | Rust check + endpoint test |
| Compose / runbook | Dockerfile、Compose、备份恢复、升级流程 | Compose config + image build + smoke test |
| Desktop targeting | 服务器入口配置、discovery 读取、兼容性提示 | 桌面端连接真实 Compose 服务 |
| Release verification | 从干净环境完整构建镜像并启动 Compose | health / version / discovery / Web 全通过 |
| K8s mapping | 把 Compose 语义映射成 K8s 资源草案 | 保持 image、command、env、probe、migration 语义一致 |

## 推荐下一步练习

最有价值的下一步是完成一次真实 Compose smoke test：

```text
1. 构建 agentdash-cloud:0.1.0
2. 复制 deploy/compose/.env.example 为 deploy/compose/.env
3. 执行 docker compose config
4. 启动 postgres
5. 执行 migrate
6. 启动 agentdash-cloud 和 reverse-proxy
7. 检查 /api/health
8. 检查 /api/version
9. 检查 /.well-known/agentdash
10. 打开 Web Dashboard
11. 执行 doctor
12. 记录需要修正的 Dockerfile、Compose、env 或后端配置问题
```

这一步完成后，部署链路就从文档和 scaffold 进入真实可运行的产品化基准。后续桌面端服务器指向、CI 发布和 Kubernetes 映射都应以这条链路为事实源。
