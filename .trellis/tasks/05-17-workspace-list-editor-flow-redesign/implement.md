# Workspace 列表与编辑流程重整 Implement Plan

## Phase 1. Planning Gate

- [x] 更新 PRD，明确 Workspace / Binding / Candidate / 本机目录识别的边界。
- [x] 创建 design.md，记录 UI 架构、数据流和任务边界。
- [x] 创建 implement.md，给出可执行步骤。
- [ ] 用户确认“本机目录识别作为可见二级主入口，仅在已授权 backend 内工作”的产品取舍。
- [ ] 确认后执行 `task.py start` 进入实现。

## Phase 2. Code Orientation

- [ ] 使用 `trellis-before-dev` 读取前端和相关后端 spec。
- [ ] 复核 `workspace-list.tsx`、`ProjectSettingsPage.tsx`、`workspaceStore.ts`、`backendAccess.ts`、workspace API 后端路由。
- [ ] 判断后端是否允许创建无 binding 的 logical Workspace；如果不允许，定位校验点。

## Phase 3. Frontend Model Helpers

- [ ] 在 Workspace feature 内提取派生函数，避免 drawer/list 继续膨胀。
- [ ] 实现 identity 摘要、binding availability、resolution summary、candidate 转 binding input、默认名称推导。
- [ ] 为 helper 增加 focused unit tests，覆盖无 binding、默认 binding、ready/offline/error、candidate create shape。

## Phase 4. Workspace List Card

- [ ] 重构 card 信息结构：Identity、Project default、binding availability、resolution diagnostic。
- [ ] 空状态改为引导“从发现项创建 / 创建逻辑 Workspace / 本机目录识别”，不再只提示 backend + 目录。
- [ ] 保留卡片主操作：设 Project 默认、打开详情、删除/归档入口。

## Phase 5. Create Drawer Modes

- [ ] 将新建 drawer 顶部改为模式切换。
- [ ] `从发现项创建`：加载 candidates，选择后预填 identity 和 binding，确认创建。
- [ ] `创建逻辑 Workspace`：填写 logical identity，可等待 inventory sync；必要时调整后端创建校验。
- [ ] `本机目录识别`：筛选本机/在线/已授权 backend，支持 browse + detect + 确认创建。
- [ ] detect warnings 和 matched workspace ids 显示为确认前诊断。

## Phase 6. Detail Drawer Sections

- [ ] 拆分 Identity / Resolution / Bindings / Candidates / Advanced Maintenance。
- [ ] Binding 列表突出 Workspace default binding、priority、status、last verified。
- [ ] Candidate 区支持对当前 Workspace 确认新增 binding。
- [ ] 手工 backend/root/status/detected_facts 编辑放入 Advanced Maintenance。

## Phase 7. Backend/API Adjustment If Needed

- [ ] 若创建空 logical Workspace 被后端拒绝，调整 request/domain 校验允许无 bindings 的 pending Workspace。
- [ ] 若新增 binding 只能通过全量 updateWorkspace 完成，先复用全量更新，不额外加 endpoint。
- [ ] 若需要新增字段或 enum，补数据库 migration；否则不引入 migration。

## Phase 8. Validation

- [ ] `pnpm typecheck`
- [ ] `pnpm lint`
- [ ] `pnpm test`
- [ ] 必要时 `pnpm dev` 手工验证 Project Settings Workspace tab：
  - Inventory 展开可点击并显示。
  - Candidate 可创建 Workspace。
  - 本机目录识别可 detect 并创建。
  - 无授权 backend、无 binding、offline/error binding 有明确诊断。

## Risky Files

- `packages/app-web/src/features/workspace/workspace-list.tsx`
- `packages/app-web/src/pages/ProjectSettingsPage.tsx`
- `packages/app-web/src/stores/workspaceStore.ts`
- `packages/app-web/src/services/backendAccess.ts`
- `crates/agentdash-api/src/routes/workspaces.rs`
- `crates/agentdash-domain/src/workspace/*`

## Rollback Points

- 先提交文档规划。
- 前端 helper 与 UI 重构分开提交，便于回滚 UI 而保留模型函数。
- 如需后端校验调整，单独提交并带后端测试。
