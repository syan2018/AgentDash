# Compose 交付基准与运维 Runbook 执行计划

## Suggested Steps

1. 阅读父任务材料和 `docs/deployment/deployment-baseline.md`。
2. 检查当前 build 脚本、frontend build、backend release build。
3. 设计 `agentdash-cloud` Dockerfile。
4. 设计 Compose services：
   - postgres。
   - migrate。
   - app。
   - reverse-proxy。
5. 补 `.env.example`。
6. 写 upgrade / backup / restore runbook。
7. 更新 K8s 映射说明。
8. 用 `docker compose config` 做静态校验。

## Validation

至少运行：

```bash
docker compose config
git diff --check
```

如果实现 Dockerfile，可视环境尝试：

```bash
docker compose build
docker compose up -d
```

## Handoff

完成后同步父任务：

- Compose 文件路径。
- 当前是否可运行。
- 依赖哪些尚未实现的 server command 或 endpoint。
- K8s 映射剩余缺口。

## Current Slice Result

- 已落地 Compose scaffold：
  - `deploy/compose/docker-compose.yml`
  - `deploy/compose/.env.example`
  - `deploy/compose/reverse-proxy/Caddyfile`
- 已落地 cloud image 构建入口：
  - `deploy/docker/Dockerfile.cloud`
  - `pnpm run docker:cloud:build`
- 已补充备份与恢复 runbook：
  - `deploy/runbooks/backup-restore.md`
- 当前 scaffold 依赖 `agentdash-cloud:${AGENTDASH_VERSION}` 目标镜像。
- 已通过 `docker compose config` 静态校验。
- 下一步在环境允许时跑完整镜像构建与 Compose 启动验证。
