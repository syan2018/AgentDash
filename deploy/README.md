# AgentDash 部署入口

`deploy/` 维护 AgentDash 从仓库构建产物到可部署运行环境的交付链路。当前基准是先用 Docker Compose 固定单机交付模型，再把同一套产物和运行语义映射到 Kubernetes。

## 目录结构

| 目录 | 职责 |
| --- | --- |
| `docker/` | 云端镜像构建入口、镜像标签和构建参数说明 |
| `compose/` | Docker Compose 基准部署文件、`.env.example` 和本机验证入口 |
| `k8s/` | Kubernetes 映射草案，保持与 Compose 角色一致 |
| `runbooks/` | 发布、升级、备份、恢复和排障流程 |

## 发布产物

| 产物 | 当前构建入口 | 目标用途 |
| --- | --- | --- |
| `agentdash-server` release binary | `pnpm run backend:build` | 云端 API、Relay endpoint、migration / doctor 命令承载体 |
| Web Dashboard static assets | `pnpm run frontend:build` | 云端 Web 入口，后续由 cloud image 或 reverse proxy 托管 |
| `agentdash-cloud:<version>` image | 待新增 `deploy/docker` 构建入口 | Compose / Kubernetes 共用云端镜像 |
| Windows desktop installer | `pnpm run desktop:bundle` | 桌面端通用包或预配置包 |

## 版本信息来源

发布链路统一收敛以下版本字段：

| 字段 | 来源 | 消费方 |
| --- | --- | --- |
| `version` | 根 `package.json` 与 Cargo workspace version 对齐 | cloud image tag、server version、desktop release |
| `git_sha` | 发布时的 Git commit | `/api/version`、release notes、排障 |
| `build_time` | 发布流水线生成 | `/api/version`、artifact manifest |
| `relay_protocol_version` | 云端/本机 relay 契约版本 | discovery endpoint、desktop compatibility check |
| `desktop_version` | 桌面构建产物版本 | installer、compatibility matrix |

第一版通过以下命令生成发布元数据：

```bash
pnpm run release:metadata
pnpm run release:metadata -- --out dist/release/agentdash-release.json
```

该命令从根 `package.json`、Cargo workspace metadata 和当前 Git commit 生成 artifact manifest。后续由 cloud primitives 子任务把其中的字段落到 `/api/version` 与 `/.well-known/agentdash`。

云端 release build 需要把 metadata 注入 Rust 编译环境：

```env
AGENTDASH_GIT_SHA=<git-sha>
AGENTDASH_BUILD_TIME=<iso-time>
```

`schema_version` 由 `agentdash-api` build script 从 `agentdash-infrastructure/migrations` 自动注入。

## 基准发布顺序

```text
check
build backend release binary
build Web Dashboard static assets
build cloud image
build desktop installer
write artifact manifest
publish artifacts
write release notes and compatibility matrix
```

Compose 与 Kubernetes 共享 `agentdash-cloud:<version>`。桌面端发布包通过 discovery endpoint 与服务器确认版本兼容性。
