# 部署基准方案与发布流程设计

## Architecture

本任务的目标运行模型以 Docker Compose 为基准，并保持与 Kubernetes 的一一映射关系。

```text
Docker Compose
  reverse-proxy
  agentdash-cloud
  migrate
  postgres

Kubernetes
  Ingress
  Deployment/agentdash-cloud
  Job/agentdash-migrate
  managed PostgreSQL or StatefulSet
```

`agentdash-cloud` 是云端主镜像，负责承载 `agentdash-server` 和 Web Dashboard 静态资源。`migrate` 使用同一镜像，只切换运行命令。PostgreSQL 在 Compose 基准方案中作为独立 service，在 Kubernetes 方案中优先映射为托管数据库或独立 StatefulSet。

## Cloud Runtime Boundary

云端服务需要逐步形成三个稳定入口：

```bash
agentdash-server serve
agentdash-server migrate
agentdash-server doctor
```

- `serve`：启动 API、Web 静态资源服务、Relay endpoint。
- `migrate`：连接部署数据库并执行 migration。
- `doctor`：检查数据库、schema、关键配置、版本信息和基础服务可用性。

当前代码已经在 `build_server` 中运行 migration 与 readiness check。产品化部署需要保留 readiness check，同时将 migration 抽成可独立执行的部署步骤。

## Configuration Contract

部署配置围绕一个公共入口 origin 收束：

```env
AGENTDASH_PUBLIC_ORIGIN=https://agentdash.example.internal
AGENTDASH_BIND_HOST=0.0.0.0
AGENTDASH_PORT=3001
DATABASE_URL=postgres://agentdash:change-me@postgres:5432/agentdash
AGENTDASH_SECRET_KEY=change-me
AGENTDASH_ENCRYPTION_KEY=change-me
RUST_LOG=info
```

`AGENTDASH_PUBLIC_ORIGIN` 用于推导：

- Web Dashboard 访问入口。
- API base URL。
- Relay WebSocket URL。
- desktop discovery response。
- 对外版本信息中的 public origin。

开发默认值和部署必填项需要分离。开发期可以继续使用 `127.0.0.1:3001` 和 embedded PostgreSQL；部署形态应明确要求 `DATABASE_URL` 和 public origin。

## Deployment Update Flow

标准升级链路：

```text
pull image
backup database
run migration
start app
check health
check version
```

失败处理：

- migration 前失败：不改变数据库，可继续使用旧服务。
- migration 失败：停止升级，保留备份，人工检查数据库状态。
- app 启动失败：如果 schema 兼容，可回滚旧镜像；否则走数据库恢复。
- health/version check 失败：保留日志和当前版本信息，按 runbook 决定回滚或恢复。

## Desktop Server Targeting

桌面端构建时可以写入默认服务器地址，但运行时需要支持配置、持久化和切换目标服务器。

首次启动数据流：

```text
load saved server origin
fallback to build-time default origin
fallback to manual server input
GET /.well-known/agentdash
validate product marker
validate desktop compatibility
save server origin
continue login / device pairing / local runtime management
```

Discovery endpoint 返回 API base、Relay WebSocket、server version、desktop version constraints 和 relay protocol version。

## Desktop Release Boundary

桌面端发布包分为：

- 通用包：没有预设服务器地址，首次启动输入服务器。
- 预配置包：构建时带默认服务器地址，运行时仍可修改。

`builtin` API mode 适合个人本机体验、开发调试和演示；连接已部署服务器的桌面包应使用 `external` 作为默认模式。

## Version Compatibility

服务端通过 discovery endpoint 暴露：

- `server_version`
- `min_desktop_version`
- `recommended_desktop_version`
- `relay_protocol_version`

桌面端连接前执行兼容检查：

- 低于最低版本：阻止连接并提示升级。
- 满足最低版本但低于推荐版本：允许连接并提示升级。
- 服务器不可达：保留配置并显示连接失败状态。

## Rollout Shape

第一阶段先完成文档与接口设计确认。后续可拆为独立开发任务：

- 云端镜像与 Compose 基准。
- migration / doctor 子命令。
- `/api/version` 与 `/.well-known/agentdash`。
- 桌面端服务器配置与 discovery。
- 版本兼容检查。
- backup / restore / upgrade runbook。

## Parallel Workstreams

父任务作为协调任务，不直接承载所有实现。并行工作流如下：

```text
deployment-baseline (parent)
  -> deployment-cloud-primitives
  -> deployment-compose-runbook
  -> deployment-desktop-targeting
```

依赖关系：

- `deployment-cloud-primitives` 产出 version/discovery/config/server-command 契约。
- `deployment-compose-runbook` 消费 cloud 契约，落地镜像、Compose 和运维 runbook。
- `deployment-desktop-targeting` 消费 discovery/compatibility 契约，落地桌面端服务器指向和发布语义。
- 父任务同步 `docs/deployment/deployment-baseline.md`，保持三个子任务使用同一套字段和术语。

并行规则：

- 子任务可以先按文档中的目标契约推进草案。
- 一旦 cloud 子任务修改 endpoint 字段或配置命名，必须同步父任务和兄弟子任务。
- Compose 子任务不要实现云端 endpoint；桌面子任务不要实现云端 discovery endpoint。
- 桌面子任务可以用 mock discovery response 设计 UI/配置流，但最终字段以 cloud 子任务为准。

## Repository Deployment Pipeline

仓库级部署环境与发布链路由父任务直接维护，不拆成独立子任务。它是 cloud、Compose、desktop 和后续 Kubernetes 的集成轴。

父任务需要持续收束以下内容：

- release build 入口：
  - backend release binary。
  - Web Dashboard static build。
  - desktop installer。
- cloud image build 入口：
  - Dockerfile 位置。
  - image tag 规则。
  - build args / version args。
- 版本注入：
  - server version。
  - git SHA。
  - build time。
  - desktop version。
  - relay protocol version。
- 部署目录结构：
  - `deploy/compose/`。
  - `deploy/docker/`。
  - `deploy/k8s/` 草案位置。
  - runbook 文档位置。
- CI / release workflow 草案：
  - check。
  - build cloud image。
  - build desktop installer。
  - publish artifacts。
  - release notes / compatibility matrix。

Compose 子任务消费这条链路产出的镜像和目录约定；桌面子任务消费版本注入和发布命令约定；cloud 子任务消费版本字段和 server command 约定。
