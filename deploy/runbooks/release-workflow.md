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
| cloud image | 待新增 | `agentdash-cloud:<version>` |
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
1. 拉取目标版本 cloud image
2. 备份 PostgreSQL
3. 执行 migrate one-shot service
4. 启动 agentdash-cloud 和 reverse-proxy
5. 检查 /api/health
6. 检查 /api/version
7. 记录升级结果
```

涉及 schema 变更的回退以升级前数据库备份恢复为准；同 schema 兼容范围内可以回滚 cloud image。
