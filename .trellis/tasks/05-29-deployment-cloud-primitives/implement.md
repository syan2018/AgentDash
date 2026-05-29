# 云端发布原语与发现端点执行计划

## Suggested Steps

1. 阅读父任务材料：
   - `.trellis/tasks/05-29-deployment-baseline/prd.md`
   - `.trellis/tasks/05-29-deployment-baseline/design.md`
   - `docs/deployment/deployment-baseline.md`
2. 检查当前 API 路由、health endpoint、migration 调用和配置解析。
3. 固定 `/api/version` 与 `/.well-known/agentdash` schema。
4. 实现 endpoint 或写明暂缓实现原因。
5. 梳理 `agentdash-server migrate` / `doctor` 子命令方案。
6. 更新部署基准文档。
7. 运行后端检查。

## Validation

优先使用：

```bash
pnpm run backend:check
pnpm run backend:test
```

如果只做文档和设计，可至少运行：

```bash
git diff --check
```

## Handoff

完成后在父任务文档中同步：

- endpoint 字段。
- CLI/subcommand 决策。
- 配置命名。
- Compose 和桌面端可依赖的契约。

## Current Slice Result

- 已实现 `/api/version`。
- 已实现 `/.well-known/agentdash`。
- 已加入 `agentdash-api` build script，从 migration 文件名注入 `schema_version`。
- 已让 server options 识别 `AGENTDASH_BIND_HOST` / `AGENTDASH_PORT`。
- 已验证 `cargo test -p agentdash-api release_info`。
