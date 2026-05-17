# Workspace 列表与编辑流程重整 Implement Plan

## Phase 1. Planning Gate

- [x] 更新 PRD，明确 Workspace / Binding / Candidate / 本机目录识别的边界。
- [x] 创建 design.md，记录 UI 架构、数据流和任务边界。
- [x] 创建 implement.md，给出可执行步骤。
- [x] 用户确认“本机目录识别作为可见二级主入口，仅在已授权 backend 内工作”的产品取舍。
- [x] 确认后执行 `task.py start` 进入实现。

## Phase 2. Code Orientation

- [x] 使用 `trellis-before-dev` 读取前端和相关后端 spec。
- [x] 复核 `workspace-list.tsx`、`ProjectSettingsPage.tsx`、`workspaceStore.ts`、`backendAccess.ts`、workspace API 后端路由。
- [x] 判断后端是否允许创建无 binding 的 logical Workspace；如果不允许，定位校验点。

## Phase 3. Frontend Model Helpers

- [x] 在 Workspace feature 内提取派生函数，避免 drawer/list 继续膨胀。
- [x] 实现 identity 摘要、binding availability、resolution summary、candidate 转 binding input、默认名称推导。
- [x] 为 helper 增加 focused unit tests，覆盖无 binding、默认 binding、ready/offline/error、candidate create shape。

## Phase 4. Workspace List Card

- [x] 重构 card 信息结构：Identity、Project default、binding availability、resolution diagnostic。
- [x] 空状态改为引导“从发现项创建 / 创建逻辑 Workspace / 本机目录识别”，不再只提示 backend + 目录。
- [x] 保留卡片主操作：设 Project 默认、打开详情、删除/归档入口。

## Phase 5. Create Drawer Modes

- [x] 将新建 drawer 顶部改为模式切换。
- [x] `从发现项创建`：加载 candidates，选择后预填 identity 和 binding，确认创建。
- [x] `创建逻辑 Workspace`：填写 logical identity，可等待 inventory sync；必要时调整后端创建校验。
- [x] `本机目录识别`：筛选本机/在线/已授权 backend，支持 browse + detect + 确认创建。
- [x] detect warnings 和 matched workspace ids 显示为确认前诊断。

## Phase 6. Detail Drawer Sections

- [x] 拆分 Identity / Resolution / Bindings / Candidates / Advanced Maintenance。
- [x] Binding 列表突出 Workspace default binding、priority、status。
- [x] Candidate 区支持对当前 Workspace 确认新增 binding。
- [x] 手工 backend/root/status/detected_facts 编辑放入 Advanced Maintenance。

## Phase 7. Backend/API Adjustment If Needed

- [x] 复核创建空 logical Workspace：后端已允许显式 identity + 空 bindings，无需调整校验。
- [x] 若新增 binding 只能通过全量 updateWorkspace 完成，先复用全量更新，不额外加 endpoint。
- [x] 若需要新增字段或 enum，补数据库 migration；否则不引入 migration。

## Phase 8. Validation

- [x] `pnpm typecheck`
- [x] `pnpm lint`
- [x] `pnpm test`
- [ ] 必要时 `pnpm dev` 手工验证 Project Settings Workspace tab：
  - Inventory 展开可点击并显示。
  - Candidate 可创建 Workspace。
  - 本机目录识别可 detect 并创建。
  - 无授权 backend、无 binding、offline/error binding 有明确诊断。

## Phase 9. Missed Inventory Registration Fix

- [x] 补 PRD / design / implement，明确本机目录识别必须能登记 backend inventory。
- [x] 新增 backend inventory register endpoint：已授权 access + root_ref -> detect -> upsert inventory。
- [x] 前端 service 增加 register API。
- [x] 本机目录识别区增加 `登记到 Backend Inventory` 动作，成功后刷新 candidates / inventory 输入。
- [x] 区分登记 inventory 与创建 Workspace，Advanced Maintenance 只维护 bindings。
- [x] 补齐 Workspace 抽屉登记成功后的 Backend Access 面板刷新，已展开 Inventory 会重新拉取。
- [x] 补回归验证并重新运行 frontend/backend 相关检查。

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
