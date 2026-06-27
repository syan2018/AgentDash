# 部署版本更新与 CI 基建设计

## Scope

本任务补齐部署基线的工程化入口，聚焦 Compose 版本更新与 CI artifact 产出。它不把 PR #47 变成完整生产 CD，而是让后续生产部署有稳定的命令、配置和流水线骨架可接。

## Architecture

### Compose Image Contract

Compose image 从固定本地名：

```yaml
image: agentdash-cloud:${AGENTDASH_VERSION}
```

调整为 registry-aware 形式：

```yaml
image: ${AGENTDASH_IMAGE_REPOSITORY:-agentdash-cloud}:${AGENTDASH_VERSION:?AGENTDASH_VERSION is required}
```

本地验证继续使用 `agentdash-cloud:0.1.0`；CI/远端部署可以把 `AGENTDASH_IMAGE_REPOSITORY` 设置为 `ghcr.io/<owner>/<image>`。

### PostgreSQL Compose Modes

默认 `docker-compose.yml` 继续包含 `postgres` 服务，作为单机基准部署和本机验证入口。

新增 managed PostgreSQL override：

```text
deploy/compose/docker-compose.managed-postgres.yml
```

该 override 让 `migrate` 不依赖 Compose 内置 `postgres`，让 `agentdash-cloud` 只依赖 `migrate` 完成。真实部署通过外部 `DATABASE_URL` 连接 managed PostgreSQL。

### Update Script

新增 `deploy/compose/update.ps1` 作为当前 Windows/PowerShell 工作环境下的可执行更新入口。脚本支持：

- `-EnvFile`
- `-ComposeFile`
- `-Version`
- `-ImageRepository`
- `-ManagedPostgres`
- `-SkipBackup`
- `-DryRun`

执行顺序：

```text
docker compose config
docker compose pull
backup PostgreSQL unless skipped
docker compose run --rm migrate
docker compose up -d agentdash-cloud reverse-proxy
curl /api/health
curl /api/version
docker compose run --rm agentdash-cloud doctor
```

默认内置 Postgres 模式用 `docker compose exec postgres pg_dump` 备份。Managed PostgreSQL 模式无法假设本地有 `pg_dump` 或可 exec 的 postgres 容器，因此脚本要求显式 `-SkipBackup`，备份由部署方 managed database snapshot/runbook 承担。

### CI Skeleton

新增 GitHub Actions workflow：

```text
.github/workflows/cloud-image.yml
```

目标是构建与发布 artifact 的骨架：

- checkout
- setup Node / pnpm
- setup Rust toolchain
- run focused checks
- generate release metadata
- build cloud image with version and SHA tags
- optionally push to GHCR on tags/manual input
- upload release metadata artifact

CI 不做远端部署；CD 由后续任务基于 update script 和环境审批接入。

## Contracts

- `AGENTDASH_VERSION` 继续是部署版本事实。
- `AGENTDASH_IMAGE_REPOSITORY` 是 image repository，可缺省为 `agentdash-cloud`。
- `DATABASE_URL` 是 app/migrate 共同消费的 PostgreSQL URL。
- `AGENTDASH_PUBLIC_ORIGIN` 是 health/version 检查的外部入口事实源。

## Trade-offs

- 先提供 PowerShell update 脚本，原因是当前项目工作说明和主力环境是 Windows/PowerShell；后续 Linux shell 脚本可按同一 contract 补齐。
- Managed PostgreSQL 备份先要求 `-SkipBackup`，原因是外部数据库备份能力差异大，脚本不应假装能统一处理云厂商 snapshot。
- CI skeleton 先不自动部署，原因是当前远端环境、registry 权限和审批策略尚未固定。

## Rollback

- 同 schema version 内通过切换 `AGENTDASH_VERSION` 并重新执行 update 流程回滚 image。
- 涉及 schema migration 的回滚仍以升级前备份或 managed database snapshot 恢复为准。
- update script 在 migration 或 health/version/doctor 失败时停止并保留错误输出，不执行隐式恢复。
