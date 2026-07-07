# 用户偏好工作状态建模实现计划

## Checklist

- [x] 新增前端用户工作状态 service/model，封装 `ui.workspace_state` 的读取、写入、解析和 recent Project 维护。
- [x] 改造 `projectStore.fetchProjects()`：加载 Project 后按用户工作状态选择当前 Project，并在状态不可用时修正写回。
- [x] 改造 `selectProject()`、`createProject()`、`cloneProject()`、`deleteProject()`：用户显式 Project 变化时写回工作状态。
- [x] 补充 `projectStore` 或 service 单元测试，覆盖恢复上次 Project、新建 Project 不被列表顺序覆盖、不可用偏好修正、显式切换写回。
- [x] 运行前端定向测试与 typecheck。

## Validation

```powershell
pnpm --filter app-web test -- projectStore userWorkspaceState
pnpm --filter app-web exec eslint src/services/userWorkspaceState.ts src/services/userWorkspaceState.test.ts src/stores/projectStore.ts src/stores/projectStore.test.ts
pnpm run frontend:check
```

## Risk Points

- `fetchProjects()` 同时负责服务端数据和默认选择，改动时需要避免异步 settings 读取导致短暂选错 Project 并连接错误 Project event stream。
- settings 写入失败不能阻断用户本地切换 Project，但要让 store error 可见。
- Project 删除后如果当前 Project 被移除，需要用同一选择规则落到剩余 Project 并修正工作状态。

## Rollback

回滚前端 service 与 `projectStore` 改动即可。没有数据库 migration 或后端 schema 变更。
