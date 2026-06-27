# 桌面端服务器指向与发布策略执行计划

## Suggested Steps

1. 阅读父任务材料和 `docs/deployment/deployment-baseline.md`。
2. 检查 `scripts/desktop-build.js` 和 Tauri 启动配置。
3. 检查当前 API origin resolve 逻辑。
4. 设计运行时 server origin 持久化位置。
5. 设计 discovery fetch 和 compatibility check。
6. 更新桌面端发布说明。
7. 如果进入实现，补最小 UI / 状态处理。

## Validation

按改动范围选择：

```bash
pnpm run desktop:check
pnpm --filter app-tauri build
pnpm run desktop:bundle
git diff --check
```

## Handoff

完成后同步父任务：

- 服务器配置存储位置。
- 通用包与预配置包命令。
- 依赖的 discovery 字段。
- 未实现的自动更新或版本提示缺口。
