# 备份与恢复 Runbook

本文记录 Docker Compose 基准部署下 PostgreSQL 数据的备份、恢复和回滚边界。升级前备份是发布流程的固定步骤，因为 schema migration 与应用镜像版本共同决定运行状态。

## 备份

在部署目录准备本地备份目录：

```bash
mkdir -p backups
```

执行逻辑备份。为了保留 custom dump 的二进制格式，先在 `postgres` 容器内写入 dump，再通过 `docker compose cp` 复制到宿主机：

```bash
timestamp=$(date +%Y%m%d-%H%M%S)
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env exec -T postgres sh -c "pg_dump -U \"\$POSTGRES_USER\" -d \"\$POSTGRES_DB\" --format=custom --no-owner --file=/tmp/agentdash-$timestamp.dump"
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env cp postgres:/tmp/agentdash-$timestamp.dump deploy/compose/backups/agentdash-$timestamp.dump
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env exec -T postgres rm -f /tmp/agentdash-$timestamp.dump
```

备份文件需要和本次发布的 cloud image tag、git SHA、schema version 一起记录。这样恢复时能明确数据库快照对应的应用版本，而不是只依赖文件名判断。

managed PostgreSQL 部署使用数据库平台 snapshot 或外部备份工具。Compose update 脚本在 `--managed-postgres` 模式下要求显式 `--skip-backup`，原因是外部数据库备份入口应由部署方数据库平台负责。

## 升级前检查

升级前确认目标镜像和当前备份可用：

```bash
pnpm run deploy:compose:update:dry-run -- --env-file deploy/compose/.env --version 0.2.0
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env run --rm agentdash-cloud doctor
```

随后更新 `.env` 中的 `AGENTDASH_VERSION`，并按发布链路执行 migration 和服务重启。

## 恢复

恢复前停止会写入数据库的服务：

```bash
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env stop agentdash-cloud reverse-proxy
```

重建目标数据库并导入备份：

```bash
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env exec -T postgres sh -c 'dropdb --if-exists -U "$POSTGRES_USER" "$POSTGRES_DB" && createdb -U "$POSTGRES_USER" "$POSTGRES_DB"'
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env exec -T postgres sh -c 'pg_restore -U "$POSTGRES_USER" -d "$POSTGRES_DB" --no-owner' < backups/agentdash-<timestamp>.dump
```

恢复后使用与该备份匹配的 `AGENTDASH_VERSION` 启动服务：

```bash
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env up -d agentdash-cloud reverse-proxy
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env run --rm agentdash-cloud doctor
curl -fsS http://127.0.0.1:8080/api/health
curl -fsS http://127.0.0.1:8080/api/version
```

## 回滚边界

同一个 schema version 内的应用回滚可以只切换 `AGENTDASH_VERSION` 并重启服务。涉及 schema migration 的升级回滚以升级前备份恢复为准，恢复完成后再启动与备份匹配的 cloud image。

恢复操作完成后需要记录：

- 恢复使用的备份文件。
- 恢复后的 `AGENTDASH_VERSION`。
- `/api/version` 返回的 `schema_version`。
- `agentdash-server doctor` 的结果。
