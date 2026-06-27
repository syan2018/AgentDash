# 发布链路 Runbook

本文记录 AgentDash 基准发布链路。当前阶段先固定产物顺序和交接点，后续再把步骤自动化到 CI / release workflow。

## Artifact Inventory

| 阶段 | 命令 | 产物 |
| --- | --- | --- |
| 后端检查 | `pnpm run backend:check` | Rust 类型与依赖检查 |
| 前端检查 | `pnpm run frontend:check` | Web Dashboard 类型检查 |
| 桌面检查 | `pnpm run desktop:check` | Tauri renderer 与 shell 检查 |
| 后端构建 | `pnpm run backend:build` | `target/release/agentdash-server` |
| 前端构建 | `pnpm run frontend:build` | `packages/app-web/dist` |
| 发布元数据 | `pnpm run release:metadata` | artifact manifest JSON |
| cloud image | `pnpm run docker:cloud:build` | `agentdash-cloud:<version>` |
| 桌面安装包 | `pnpm run desktop:bundle` | `target/release/bundle/nsis/AgentDash_<version>_x64-setup.exe` |

## Release Metadata

每次发布需要记录：

```text
version
git_sha
build_time
schema_version
relay_protocol_version
compatible_desktop_range
compatible_local_runtime_range
```

这些字段后续进入：

- `/api/version`。
- `/.well-known/agentdash`。
- cloud image labels。
- release notes。
- compatibility matrix。

## Baseline Flow

```text
1. 运行检查
2. 构建后端 release binary
3. 构建 Web Dashboard static assets
4. 生成 artifact manifest
5. 构建 cloud image
6. 构建 desktop installer
7. 发布 artifacts
8. 更新 release notes 和 compatibility matrix
```

## Upgrade Flow

Compose 基准升级流程：

```text
1. 解析目标 AGENTDASH_IMAGE_REPOSITORY 和 AGENTDASH_VERSION
2. 校验 Compose 配置
3. 拉取目标版本 cloud image
4. 备份 PostgreSQL 或确认 managed PostgreSQL 快照已完成
5. 执行 migrate one-shot service
6. 启动 agentdash-cloud 和 reverse-proxy
7. 检查 /api/health
8. 检查 /api/version
9. 执行 `docker compose run --rm agentdash-cloud doctor`
10. 记录升级结果
```

PowerShell 更新入口：

```powershell
pnpm run deploy:compose:update -- -EnvFile deploy/compose/.env -Version 0.2.0
```

连接 managed PostgreSQL 时：

```powershell
pnpm run deploy:compose:update -- -EnvFile deploy/compose/.env -Version 0.2.0 -ManagedPostgres -SkipBackup
```

`-ManagedPostgres` 会追加 `deploy/compose/docker-compose.managed-postgres.yml`，让 migration job 直接连接 `DATABASE_URL` 指向的外部 PostgreSQL。`-SkipBackup` 表示备份由 managed database snapshot 或部署方数据库备份流程承担。

## CI Artifact Flow

`.github/workflows/cloud-image.yml` 提供 cloud image 构建骨架：

```text
checkout
install pnpm / Rust
backend check
frontend check
release metadata
docker build cloud image
optional push to GHCR
upload release metadata artifact
```

该 workflow 不执行远端部署。后续 CD 应消费 registry image、release metadata 和 Compose update script，并通过环境审批管理生产发布。

备份、恢复和回滚边界见 [备份与恢复 Runbook](./backup-restore.md)。涉及 schema 变更的回退以升级前数据库备份恢复为准；同 schema version 内可以回滚 cloud image。
