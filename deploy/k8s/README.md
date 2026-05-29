# Kubernetes 映射草案

`deploy/k8s/` 维护 Kubernetes 部署形态的资源映射。第一阶段只保持与 Compose 模型一致的资源边界，完整 Helm chart 后续再落地。

## 资源映射

| Compose 角色 | Kubernetes 资源 | 说明 |
| --- | --- | --- |
| `agentdash-cloud` | Deployment + Service | 运行 cloud image |
| `migrate` | Job / Helm hook | 升级前执行 migration |
| `reverse-proxy` | Ingress | 同源入口、HTTPS、WebSocket |
| `.env` | ConfigMap + Secret | 配置和敏感信息分离 |
| `postgres` | managed PostgreSQL / StatefulSet | 生产优先接外部数据库 |

## 设计原则

Kubernetes 方案继承 Compose 运行语义：同一 cloud image、同一 public origin 契约、同一 migration 入口、同一 health / version check。这样 Compose 验证通过的交付模型可以自然迁移到集群部署。

## 后续落地项

- 定义 base manifests 或 Helm chart 目录。
- 明确 Secret / ConfigMap 字段。
- 明确 migration Job 的失败处理。
- 明确 readiness / liveness probe 路径。
- 明确 Ingress path 和 WebSocket upgrade 配置。
