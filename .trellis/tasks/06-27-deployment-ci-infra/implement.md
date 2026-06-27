# 实施计划

## Checklist

- [x] 调整 Compose image 为 registry-aware 形式。
- [x] 新增 managed PostgreSQL override，并确保 Compose config 可通过。
- [x] 更新 `.env.example`，加入 image repository 与 managed PostgreSQL 注释。
- [x] 新增 `deploy/compose/update.ps1`，实现 config/pull/backup/migrate/up/check/doctor 与 dry-run。
- [x] 在 `package.json` 增加可发现的 deploy/metadata 命令入口。
- [x] 新增 `.github/workflows/cloud-image.yml` skeleton。
- [x] 更新 deploy README、compose README、release runbook、backup runbook 和 cross-layer deployment spec。
- [x] 运行验证命令并修正发现的问题。

## Validation Commands

```powershell
docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env.example config
docker compose -f deploy/compose/docker-compose.yml -f deploy/compose/docker-compose.managed-postgres.yml --env-file deploy/compose/.env.example config
pwsh -NoProfile -File deploy/compose/update.ps1 -EnvFile deploy/compose/.env.example -DryRun
pnpm run release:metadata
pnpm run docker:cloud:build -- --dry-run
node -e "JSON.parse(require('fs').readFileSync('package.json','utf8')); console.log('package.json ok')"
```

If `pwsh` is unavailable, validate script syntax with Windows PowerShell-compatible parsing where possible and report the limitation.

## Risky Files

- `deploy/compose/docker-compose.yml`
- `deploy/compose/docker-compose.managed-postgres.yml`
- `deploy/compose/update.ps1`
- `.github/workflows/cloud-image.yml`
- `deploy/runbooks/release-workflow.md`
- `deploy/runbooks/backup-restore.md`

## Review Gates

- `serve` must remain readiness-only; no implicit migration should be reintroduced.
- Managed PostgreSQL override must remove dependency on Compose `postgres`.
- CI workflow must not require production secrets for PR validation.
- Update script dry-run must show commands without executing destructive operations.
