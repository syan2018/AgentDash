# 部署基准方案与发布流程

## Goal

建立 AgentDash 产品化部署基准，并用文档和后续实现任务持续维护这条部署线：

- Docker Compose 作为第一交付形态。
- Kubernetes 作为后续扩展形态，继承 Compose 的运行模型。
- 云端项目具备明确的部署、migration、更新、备份、恢复和版本检查流程。
- 桌面端能够正确指向目标服务器，并形成通用包与预配置包两类发布策略。

## Background

当前仓库已经新增讨论文档 `docs/deployment/deployment-baseline.md`，记录了本轮讨论形成的初始判断。该任务用于持续跟进这些部署相关尝试，让后续代码、脚本和文档变更有统一任务上下文。

当前讨论已经明确不在本任务中展开企业形态、商业授权、SSO、组织治理等扩展话题。本任务聚焦基准部署工程化。

## Confirmed Facts

- 开发调试使用 `pnpm dev`，它会编译 Rust binary，并依次拉起云端后端、本机后端和前端。
- Rust 后端无法热重载，更新后需要重新启动相关进程。
- 当前仓库已有 Tauri 桌面端构建入口：
  - `pnpm run desktop:build`
  - `pnpm run desktop:bundle`
- 桌面构建脚本已支持 `builtin`、`external`、`sidecar` 三种 API mode。
- 当前后端启动时会解析 `HOST`、`PORT`，并通过 `DATABASE_URL` 或开发期 PostgreSQL runtime 获得数据库连接。
- `agentdash-infrastructure` 已维护 PostgreSQL migration，并在 API server 构建时运行 migration 与 schema readiness check。
- 当前分支为 `codex/deployment-baseline`。

## Requirements

- 维护一份可持续更新的部署基准文档，作为部署尝试的入口材料。
- 将部署基准任务拆成可并行的子任务，并在父任务中维护职责边界、依赖关系和交接契约。
- 父任务直接维护仓库级部署环境与发布链路的总体设计：
  - release build 入口。
  - cloud image build 入口。
  - 版本信息注入方式。
  - `deploy/` 目录结构。
  - CI / release workflow 草案。
  - Compose 与后续 Kubernetes 交付的流水线入口。
- 明确 Docker Compose 基准部署的组件边界：
  - cloud app。
  - PostgreSQL。
  - reverse proxy。
  - migration one-shot service。
  - backup / restore 脚本位置。
- 明确云端 release 镜像的目标形态：
  - 包含 `agentdash-server`。
  - 包含 Web Dashboard 静态资源。
  - 支持显式 migration / serve / doctor 运行入口。
- 明确部署配置契约，特别是公共入口 origin、数据库连接、secret、日志配置和开发默认值的边界。
- 明确云端更新流程：
  - 拉取镜像。
  - 数据库备份。
  - 显式运行 migration。
  - 启动新版本。
  - 健康检查。
  - 版本检查。
- 明确 rollback / restore 的现实策略：同 schema 兼容范围内可回滚应用镜像，涉及 schema 变更时依赖升级前数据库备份恢复。
- 明确桌面端如何指向目标服务器：
  - 构建时默认服务器地址只作为默认值。
  - 运行时允许配置和持久化服务器地址。
  - 通过 discovery endpoint 获取 API / Relay / 版本兼容信息。
- 明确桌面端发布包策略：
  - 通用包。
  - 预配置包。
  - `builtin` 与 `external` 的发布语义。
- 明确 server、desktop、local runtime 的版本兼容策略。
- 明确 Compose 到 Kubernetes 的资源映射关系。

## Acceptance Criteria

- [x] 创建部署尝试分支 `codex/deployment-baseline`。
- [x] 新增初始部署基准文档 `docs/deployment/deployment-baseline.md`。
- [x] 创建 Trellis 任务并绑定当前分支。
- [x] 拆分可并行子任务，并明确其它机器上的 Agent 可以认领的工作范围。
- [ ] 部署基准文档覆盖 Compose、Kubernetes 映射、云端更新维护、桌面端服务器配置和桌面发布策略。
- [ ] 形成后续实现切分建议，能把文档落地为小步可验证的开发任务。
- [ ] 后续启动实现前补齐或确认 `design.md` 与 `implement.md`。
- [ ] 后续涉及代码或脚本改动时运行相应质量检查。

## Out of Scope

- 企业形态、组织治理、SSO、授权商业逻辑、私有扩展仓边界。
- 多租户 SaaS 部署方案。
- 完整 Helm chart 实现。
- 自动更新系统的完整实现。
- 部署平台选型比较。

## Child Task Map

父任务负责协调和保持部署基准文档一致。具体实现或深入设计按以下子任务并行推进：

| 子任务 | 建议处理方 | 主要职责 | 不负责 |
| --- | --- | --- | --- |
| `05-29-deployment-cloud-primitives` | 其它机器上的后端/部署 Agent | `/api/version`、`/.well-known/agentdash`、server CLI/subcommand、部署配置契约、外部 PostgreSQL 边界 | Compose 文件、桌面配置 UI |
| `05-29-deployment-compose-runbook` | 其它机器上的部署/DevOps Agent | Dockerfile、Compose、`.env.example`、reverse proxy、upgrade/backup/restore runbook、K8s 映射 | 后端 endpoint、桌面连接体验 |
| `05-29-deployment-desktop-targeting` | 其它机器上的桌面/前端 Agent | 桌面端默认服务器、运行时可配置 server origin、discovery/compatibility check、通用包/预配置包发布 | 云端 endpoint、Compose |

当前会话默认维护父任务、主文档、仓库级部署链路和跨子任务契约，不抢占子任务内部实现，除非用户明确要求。

仓库部署环境与发布链路不再拆为独立子任务，直接归父任务维护。原因是它横跨 cloud、Compose、desktop 和后续 K8s，只适合作为父任务的集成轴，而不是第四条并行实现线。

## Open Questions

- 第一轮实现应该先落地到“可运行 Compose 文件”，还是先补齐 server 子命令和 discovery/version endpoint？
- `agentdash-server` 是否直接托管 Web Dashboard 静态资源，还是由 reverse proxy 托管静态文件？
- 部署配置命名是否统一迁移到 `AGENTDASH_*` 前缀，还是短期保留当前 `HOST`、`PORT`、`DATABASE_URL`？
- 开发期 embedded PostgreSQL 和部署期外部 PostgreSQL 的边界是否需要用 profile 明确隔离？
