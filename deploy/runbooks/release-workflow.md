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
| 桌面更新目录 | `pnpm run release:metadata -- --desktop-release-dir dist/release/desktop` | `release.json`、`channels/stable/latest.json`、`upload-plan.json`、sha256 与 updater signature 引用 |

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

## 桌面发布 Manifest 契约

桌面自动更新发布目录由主仓生成，默认根目录为 `dist/release/desktop`。该目录只表达对象存储无关的 release 契约，私有发布流程负责把这些文件同步到具体 S3-compatible 服务并提供公开或内网 HTTP 访问地址。

标准目录结构：

```text
dist/release/desktop/
  releases/agentdash/<version>/release.json
  releases/agentdash/<version>/release.json.sha256
  releases/agentdash/<version>/windows/x86_64/<installer>.exe
  releases/agentdash/<version>/windows/x86_64/<installer>.exe.sha256
  releases/agentdash/<version>/windows/x86_64/<updater>.nsis.zip
  releases/agentdash/<version>/windows/x86_64/<updater>.nsis.zip.sha256
  releases/agentdash/<version>/windows/x86_64/<updater>.nsis.zip.sig
  channels/stable/latest.json
  upload-plan.json
```

`release.json` 是不可变版本 manifest，包含：

```text
schema_version
product
version
git_sha
build_time
channel
platforms.windows-x86_64.installer.file/object_key/sha256/sha256_file/public_url
platforms.windows-x86_64.updater.file/object_key/sha256/sha256_file/signature/signature_file/signature_object_key/public_url
```

`channels/stable/latest.json` 是 stable channel 指针，包含当前版本、`release_manifest` 引用和同一平台矩阵。客户端不依赖对象存储 listing；云端服务端后续通过运行期配置读取 stable latest manifest，再向桌面端提供 update endpoint。

`upload-plan.json` 是私有发布流程的输入，记录 `local_path -> object_key`、`content_type` 和 `immutable`。默认 object key 前缀为 `desktop/`，其中 versioned artifacts 与 `release.json` 为不可变对象，`desktop/channels/stable/latest.json` 是可覆盖的 channel 指针。`public_base_url_env` 使用 `AGENTDASH_DESKTOP_RELEASE_PUBLIC_BASE_URL` 占位，由私有部署流程在生成最终可访问 URL 时注入。

主仓不记录具体对象存储实例、访问凭据或私有访问域名。签名私钥只属于 release 环境；主仓 manifest 只保存 Tauri updater 产出的 signature 字符串和 `.sig` 文件引用。

## Baseline Flow

```text
1. 运行检查
2. 构建后端 release binary
3. 构建 Web Dashboard static assets
4. 生成 artifact manifest
5. 构建 cloud image
6. 构建 desktop installer 与 Tauri updater artifact
7. 生成桌面 release 目录、stable latest manifest 和 upload plan
8. 发布 artifacts
9. 更新 release notes 和 compatibility matrix
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

跨平台更新入口：

```bash
pnpm run deploy:compose:update -- --env-file deploy/compose/.env --version 0.2.0
```

连接 managed PostgreSQL 时：

```bash
pnpm run deploy:compose:update -- --env-file deploy/compose/.env --version 0.2.0 --managed-postgres --skip-backup
```

`--managed-postgres` 会追加 `deploy/compose/docker-compose.managed-postgres.yml`，让 migration job 直接连接 `DATABASE_URL` 指向的外部 PostgreSQL。`--skip-backup` 表示备份由 managed database snapshot 或部署方数据库备份流程承担。

## CI Artifact Flow

`.github/workflows/cloud-image.yml` 提供 cloud image 构建骨架，由 release tag 或手动 dispatch 触发。日常 `main` push 由 quick check 覆盖，cloud image 只在需要可交付镜像和 release metadata 时产出：

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

## 桌面自动更新发布流程

Windows stable 发布顺序：

```text
1. 运行 `pnpm run desktop:check`
2. 在 release 环境配置 Tauri updater signing key
3. 运行桌面 bundle，确认 NSIS installer、updater zip 和 `.sig` 已产出
4. 运行 `pnpm run release:metadata -- --desktop-release-dir dist/release/desktop`
5. 检查 `dist/release/desktop/upload-plan.json`
6. 私有发布流程按 upload plan 先同步不可变 versioned artifacts
7. 私有发布流程最后覆盖 `desktop/channels/stable/latest.json`
8. 配置云端服务端运行环境读取 stable latest manifest 的 HTTP URL
9. 验证云端 update endpoint 返回新版本，桌面端可下载并通过签名校验
```

排查入口：

- `release-metadata` 如果无法唯一定位 updater artifact，会明确报出期望的 `*.nsis.zip` 或 `*.msi.zip` fixture/产物模式。
- 缺少 updater signature 时，检查 release 环境是否启用了 Tauri updater signing key。
- sha256 不一致时，重新生成 release 目录并确认私有上传流程没有改写 versioned artifacts。
- stable latest 指针异常时，先确认 versioned artifacts 已存在，再覆盖 channel latest manifest。

备份、恢复和回滚边界见 [备份与恢复 Runbook](./backup-restore.md)。涉及 schema 变更的回退以升级前数据库备份恢复为准；同 schema version 内可以回滚 cloud image。
