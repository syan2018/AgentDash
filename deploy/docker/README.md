# Cloud Image 构建入口

`deploy/docker/` 负责维护 `agentdash-cloud:<version>` 镜像的构建入口。该镜像是 Compose 和 Kubernetes 共用的云端运行产物。

## 目标镜像内容

| 内容 | 说明 |
| --- | --- |
| `agentdash-server` | Rust release binary |
| Web Dashboard static assets | `packages/app-web/dist` 构建产物 |
| migration resources | `agentdash-infrastructure` 运行 migration 所需资源 |
| release metadata | version、git SHA、build time、protocol version |

## 目标运行命令

```bash
agentdash-server serve
agentdash-server migrate
agentdash-server doctor
```

`serve` 是长驻服务入口；`migrate` 是部署升级时的一次性入口；`doctor` 是升级后和排障时的诊断入口。

## 镜像标签

推荐标签：

```text
agentdash-cloud:<version>
agentdash-cloud:<version>-<short_sha>
```

`latest` 只适合本地验证。部署 runbook 使用明确版本标签，原因是升级、回滚和排障需要可追踪的产物身份。

## 后续落地项

- 新增 cloud Dockerfile。
- 新增 build args：`AGENTDASH_VERSION`、`AGENTDASH_GIT_SHA`、`AGENTDASH_BUILD_TIME`。
- 确认 Web Dashboard static assets 由 server 托管还是 reverse proxy 托管。
- 确认 migration resources 在 release image 中的路径。
