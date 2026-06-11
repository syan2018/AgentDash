# Cloud Image 构建入口

`deploy/docker/` 负责维护 `agentdash-cloud:<version>` 镜像的构建入口。该镜像是 Compose 和 Kubernetes 共用的云端运行产物。

## 目标镜像内容

| 内容 | 说明 |
| --- | --- |
| `agentdash-server` | Rust release binary |
| Web Dashboard static assets | `packages/app-web/dist` 构建产物 |
| migration resources | `agentdash-infrastructure` 运行 migration 所需资源 |
| release metadata | version、git SHA、build time、protocol version |

## 构建命令

```bash
pnpm run docker:cloud:build
pnpm run docker:cloud:build -- --tag agentdash-cloud:0.1.0 --tag agentdash-cloud:0.1.0-<short_sha>
pnpm run docker:cloud:build -- --dry-run
```

构建脚本会读取根 `package.json` 版本和当前 Git commit，并通过 build args 注入：

```env
AGENTDASH_VERSION=<package-version>
AGENTDASH_GIT_SHA=<git-sha>
AGENTDASH_BUILD_TIME=<iso-time>
```

这些字段也会写入 OCI image labels，便于 registry、发布记录和运行时版本端点互相核对。

## 目标运行命令

```bash
agentdash-server serve
agentdash-server migrate
agentdash-server doctor
```

`serve` 是长驻服务入口；`migrate` 是部署升级时的一次性入口；`doctor` 是升级后和排障时的诊断入口。

镜像入口为 `agentdash-server`，因此 Compose `command` 使用 `serve`、`migrate`、`doctor` 这些子命令即可。

## 镜像标签

推荐标签：

```text
agentdash-cloud:<version>
agentdash-cloud:<version>-<short_sha>
```

`latest` 只适合本地验证。部署 runbook 使用明确版本标签，原因是升级、回滚和排障需要可追踪的产物身份。

## 后续落地项

- 补充 CI 中的镜像构建与推送步骤。
- 确认是否需要为私有 registry 增加 tag / push 包装。
- 根据 Compose 验证结果调整 runtime base image 依赖。
