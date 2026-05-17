# Project Backend Workspace 自动路由实施计划

## Phase 0. 规划确认

- [ ] 用户 review `prd.md` / `design.md` / `implement.md`。
- [ ] 确认后再执行 `task.py start` 进入实现态。

## Phase 1. 数据模型与仓储

- [ ] 新增领域类型：`ProjectBackendAccess`、`BackendWorkspaceInventory`、状态枚举、capability/root policy 值对象。
- [ ] 新增 repository trait 和 PostgreSQL 实现。
- [ ] 新增 migration，创建 `project_backend_access` 与 `backend_workspace_inventory`。
- [ ] 更新初始化建表逻辑，保证新建库完整。
- [ ] 增加 repository 单元测试或集成测试，覆盖 create/list/update/revoke/upsert inventory。

验证命令：

- `cargo test -p agentdash-infrastructure project_backend_access`
- `cargo test -p agentdash-infrastructure backend_workspace_inventory`

## Phase 2. Backend inventory refresh

- [ ] 在 application/runtime_gateway setup action 中设计 `workspace.inventory_refresh`。
- [ ] local/backend transport 支持批量 inventory refresh；首版可先基于 accessible roots 扫描一级 workspace，并复用现有 Git/P4/LocalDir detection。
- [ ] API route：按 ProjectBackendAccess 触发 refresh，并写入 cloud inventory。
- [ ] 返回 refresh summary 与 warnings。
- [ ] 测试 offline backend、无 access、detect error、partial success。

验证命令：

- `cargo test -p agentdash-application workspace_inventory`
- `cargo test -p agentdash-api backend_access`
- `cargo test -p agentdash-local workspace_probe`

## Phase 3. ProjectBackendAccess API 与校验

- [ ] API：list/create/update/delete ProjectBackendAccess。
- [ ] 权限：Project edit 才能管理 access；Project view 可查看有效 access 和 runtime preview。
- [ ] 将 browse / detect / inventory refresh 路由收敛到 Project context，避免裸 backend browse 绕过 Project 权限。
- [ ] RuntimeResolution 增加 ProjectBackendAccess 校验。
- [ ] 测试 Project 无 access 时无法 browse/detect/execute。

验证命令：

- `cargo test -p agentdash-api project_backend_access`
- `cargo test -p agentdash-application workspace_resolution`

## Phase 4. Workspace binding sync

- [ ] 新增 application command：按 ProjectBackendAccess + inventory 同步 WorkspaceBinding。
- [ ] 匹配已有 Workspace 时自动 upsert binding。
- [ ] 未匹配 inventory 生成 candidate preview。
- [ ] 冲突/歧义/低 confidence 返回诊断，不静默落库。
- [ ] 更新 WorkspaceBinding priority 计算。
- [ ] 测试 Git/P4/LocalDir identity 匹配、多 backend 命中、离线/stale inventory。

验证命令：

- `cargo test -p agentdash-application workspace_binding_sync`
- `cargo test -p agentdash-api workspaces`

## Phase 5. 前端设置页

- [ ] Project Settings Workspace tab 增加 Backend Access 区块。
- [ ] 增加 Inventory / Discovered Candidates 区块。
- [ ] Workspace 主流程弱化手填 root_ref / JSON identity，移入高级区。
- [ ] Runtime Preview 展示 access、binding、resolution reason 和 warnings。
- [ ] 清理前端“默认 workspace 缺失时取第一个 workspace”的残留。

验证命令：

- `pnpm --filter app-web test`
- `pnpm --filter app-web build`

## Phase 6. 端到端验证

- [ ] 启动 `pnpm dev`。
- [ ] 创建/选择 Project。
- [ ] 授权一个在线 local backend。
- [ ] 刷新 inventory。
- [ ] 验证匹配已有 Workspace 自动生成 binding。
- [ ] 验证未匹配 candidate 需要用户确认创建 Workspace。
- [ ] 验证无 access 时不能 browse/detect/runtime execute。
- [ ] 验证 runtime preview 与 session 启动使用同一 binding。

## 风险文件

- `crates/agentdash-domain/src/backend/`
- `crates/agentdash-domain/src/workspace/`
- `crates/agentdash-domain/src/project/`
- `crates/agentdash-application/src/workspace/`
- `crates/agentdash-application/src/runtime_gateway/`
- `crates/agentdash-api/src/routes/backends.rs`
- `crates/agentdash-api/src/routes/workspaces.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/`
- `packages/app-web/src/pages/ProjectSettingsPage.tsx`
- `packages/app-web/src/features/workspace/`

## 回滚点

- 数据模型 Phase 1 完成后可单独回滚，不影响现有 WorkspaceBinding 手工流程。
- Inventory refresh Phase 2 可先隐藏 UI，仅保留 API 和 tests。
- RuntimeResolution access 校验上线前必须确保 ProjectBackendAccess 已有迁移/seed 或明确诊断，否则会阻断现有会话。

## 不做事项

- 不实现 server 侧 backend 能力扩展治理。
- 不实现 root/workspace 级精细 Project allowlist。
- 不引入旧 API 兼容层。
- 不以“最近使用 backend”作为默认路由依据。
