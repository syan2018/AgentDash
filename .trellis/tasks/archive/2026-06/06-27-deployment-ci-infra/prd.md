# 补齐部署版本更新与 CI 基建

## Goal

补齐 Compose 版本更新、managed PostgreSQL override、发布脚本与 CI artifact 基建，让当前部署基线能支撑可重复的版本升级与后续自动化流水线。

## Confirmed Facts

- 当前分支已经提供 `agentdash-cloud:<version>` cloud image、`agentdash-server serve/migrate/doctor`、Compose 基准、release metadata、backup/restore runbook 和 `/api/version`。
- 当前 Compose 使用 `agentdash-cloud:${AGENTDASH_VERSION}`，适合本地镜像验证，但没有 image repository 参数，远端部署时无法直接表达 registry image。
- 当前 Compose 内置 `postgres` 服务，并让 `migrate` 依赖 `postgres` 健康；真实 managed PostgreSQL 部署需要不启动内置 Postgres 且不等待该服务。
- 当前 runbook 记录了升级顺序，但没有可执行 update 脚本把 pull、backup、migrate、up、health/version/doctor 串成固定流程。
- 仓库当前没有 `.github/workflows`，发布 CI 还没有 image build / metadata artifact skeleton。

## Requirements

- Compose image 必须支持 registry-aware 配置，保留本地默认 `agentdash-cloud:<version>`。
- Compose 必须保留单机内置 PostgreSQL 基准，并新增 managed PostgreSQL override，用于真实部署只消费外部 `DATABASE_URL`。
- 新增 Compose update 脚本，固定执行 `pull -> backup -> migrate -> up -> health/version/doctor`，支持指定 env file 与跳过备份。
- runbook 必须同步说明本地 PostgreSQL 与 managed PostgreSQL 两种 Compose 形态，以及版本更新命令。
- CI skeleton 必须能在 GitHub Actions 中构建 cloud image、生成 release metadata、产出 artifact；暂不做远端自动部署。
- 保持 `serve` 不执行 migration；开发/部署脚本需要显式 `migrate then serve` 或 `migrate then up`。
- 不引入兼容旧字段或旧部署路径；按当前预研期正确目标收敛。

## Acceptance Criteria

- [ ] `docker compose config` 可在默认本地 PostgreSQL 配置下通过。
- [ ] `docker compose config` 可在 managed PostgreSQL override 下通过，不要求 `postgres` 服务存在。
- [ ] update 脚本支持目标版本、env file、compose file 组合和跳过备份参数，并按固定顺序调用 Compose。
- [ ] release/runbook 文档明确 registry image、managed PostgreSQL、更新、回滚与 CI artifact 边界。
- [ ] GitHub Actions workflow skeleton 存在，能够表达 check/build image/release metadata/artifact upload 的流水线入口。
- [ ] 相关 package scripts 或文档入口能让维护者发现这些命令。
- [ ] 基础验证命令通过，至少包括 Compose config、脚本 dry-run/help、release metadata 和相关 lint/check。

## Out of Scope

- 不实现远端 SSH/CD 自动部署。
- 不实现 Helm chart 或完整 Kubernetes manifests。
- 不拆 `DATABASE_MIGRATION_URL` / `DATABASE_URL` 双账号模型，本任务只保留后续扩展空间。
- 不实现云厂商 registry 权限、GitHub Environment 审批或 production secrets 管理。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
