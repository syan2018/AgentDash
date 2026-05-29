# Compose 交付基准与运维 Runbook 设计

## Boundaries

主要触达范围：

- `deploy/compose/*`
- `deploy/docker/*`
- `docs/deployment/*`
- 可选的 backup / restore 脚本

不负责：

- 后端 endpoint 实现。
- Tauri 桌面端配置体验。
- 完整 Kubernetes chart。

## Compose Model

目标模型：

```text
postgres
  -> migrate
  -> agentdash-cloud
  -> reverse-proxy
```

`migrate` 和 `agentdash-cloud` 使用同一镜像，只切换 command。

## Update Model

标准流程：

```text
docker compose pull
backup database
docker compose run --rm migrate
docker compose up -d app reverse-proxy
check /api/health
check /api/version
```

## Kubernetes Mapping

Compose 文件中的每个角色需要可以映射到 Kubernetes 资源：

- `migrate` -> Job / Helm hook。
- `app` -> Deployment。
- `reverse-proxy` -> Ingress。
- `.env` -> ConfigMap + Secret。
- `postgres` -> managed PostgreSQL / StatefulSet。
