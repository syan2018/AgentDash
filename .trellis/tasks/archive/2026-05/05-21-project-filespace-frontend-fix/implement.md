# Project Filespace 前端能力修复执行计划

## 总体节奏

按 P0 → P1 → P2 → P3 顺序推进。每个 P 段结束跑一次 typecheck + lint + cargo check 收口；P3 结束做一次完整 UI 走查。

## P0 — 阻塞编译 / 数据契约

### Step P0.1 — Notice prop 修正

- 文件：`packages/app-web/src/features/assets-panel/categories/FilespaceCategoryPanel.tsx`
- 改动：`<Notice data={notice} ... />` → `<Notice notice={notice} ... />`
- 验证：`pnpm --filter app-web typecheck`

### Step P0.2 — `installed_source` 透出

- 后端文件：`crates/agentdash-api/src/routes/project_filespaces.rs`
  - `ProjectFilespaceResponse` 新增 `installed_source: Option<InstalledAssetSource>` 字段；
  - `From<ProjectFilespace>` 实现里搬运字段。
- 前端文件：`packages/app-web/src/types/index.ts`
  - `ProjectFilespace` 接口新增 `installed_source?: InstalledAssetSource | null`；
  - 若 `InstalledAssetSource` 未在该文件内可见，从 `types/shared-library.ts` 引入并 re-export。
- 验证：`cargo check -p agentdash-api && pnpm --filter app-web typecheck`。

### Step P0.3 — VfsAccessPicker 加载状态修复

- 文件：`packages/app-web/src/features/project/agent-preset-editor/vfs-access-picker.tsx`
- 改动：进入 effect 立即 `setIsLoading(true)`；reset 时 `setItems([])`；保留 finally 关闭。
- 验证：手动 mount Agent 编辑器看「正在加载 VFS Mount...」是否能短暂出现。

### P0 收口

```bash
pnpm --filter app-web typecheck
pnpm --filter app-web lint
cargo check -p agentdash-api
```

## P1 — Filespace 类目对齐 SkillCategoryPanel

### Step P1.1 — Service 层补充

- 文件：`packages/app-web/src/services/projectFilespaces.ts`
- 不动既有 `update`；新增 `createProjectVfsMountBinding` / `deleteProjectVfsMountBinding`（占位，等 P2 后端落地后才能调用，接口先按设计签名开好）。

### Step P1.2 — `FilespaceCategoryPanel` 重写

- 文件：`packages/app-web/src/features/assets-panel/categories/FilespaceCategoryPanel.tsx`
- 直接以 `SkillCategoryPanel.tsx` 为模板复制并重命名实体：
  - 状态：`filespaces` / `isLoading` / `busyId` / `detail` / `confirmDelete` / `publishTarget` / `publishedAssets` / `notice` 等同款。
  - 数据获取：`listProjectFilespaces` + `fetchLibraryAssets({ asset_type: "filespace_template", owner_id: currentUserId })`。
  - `OriginBadge`：`resolveOriginBadge(filespace.installed_source ? "marketplace" : "user_authored", Boolean(filespace.installed_source))`。
  - `PublishedBadge`：按 `publishedByKey.get(filespace.key)` 渲染。
  - `CardMenu`：`编辑` / `发布到资源市场`（installed_source 或匿名用户隐藏） / `删除`。
  - `ConfirmDeleteDialog` 复用 Skill 已有版本（如不能直接复用则原地新建一份）。
  - `FilespaceEditorDialog`：
    - 顶部 metadata：display_name / description / key（key 改名仅当 binding 不存在冲突时允许，否则 toast 阻止）；
    - 主体 `VfsBrowser source={{source_type: "project_filespace", project_id, filespace_id}}`；
    - 保存调用 `updateProjectFilespace`。
  - 创建流程：先 form 创建（key + display_name + description），创建后切换到 edit 模式继续编辑文件。
- 注意：删除当前 detail 项时直接关闭 detail，不再走 `items[0]` fallback。

### Step P1.3 — `AssetPickerDrawer.FilespaceList` 过滤

- 文件：`packages/app-web/src/features/assets-panel/publish/AssetPickerDrawer.tsx`
- 改动：`visible = items.filter((f) => !f.installed_source)`；空态文案沿 Skill 模板。

### Step P1.4 — `AssetsTabView` 描述补 Filespace

- 文件：`packages/app-web/src/features/assets-panel/AssetsTabView.tsx`
- 改动：header 子标题加上 Filespace 字样。

### P1 收口

```bash
pnpm --filter app-web typecheck
pnpm --filter app-web lint
```

## P2 — Mount Binding 管理闭环

### Step P2.1 — 后端新增路由

- 文件：`crates/agentdash-api/src/routes/project_filespaces.rs`
  - 新增 `CreateProjectVfsMountBindingRequest`；
  - 新增 `create_mount_binding` handler：构造 `ProjectVfsMountBinding`，校验 mount_id / source / capabilities，调用 repo `create`；
  - 新增 `delete_mount_binding` handler：校验 binding 归属，调用 repo `delete`。
- 文件：`crates/agentdash-api/src/routes.rs`
  - 把 `/projects/{project_id}/vfs-mount-bindings` 的 `get(...)` 改为 `get(list).post(create)`；
  - 把 `/projects/{project_id}/vfs-mount-bindings/{binding_id}` 的 `put(...)` 改为 `put(update).delete(delete)`。
- 验证：`cargo check -p agentdash-api`。

### Step P2.2 — 后端测试

- 文件：在 `routes/project_filespaces.rs` 模块（或 `tests/` 同级）写 route-level 测试：
  - POST 成功路径（Filespace / ExternalService 各一）；
  - mount_id 冲突；
  - source 引用不存在 Filespace 时 404；
  - DELETE 成功 / 未找到 / 跨 Project 拒绝。
- 验证：`cargo test -p agentdash-api project_filespaces`。

### Step P2.3 — 前端 Service / Store

- 文件：`packages/app-web/src/services/projectFilespaces.ts`
  - 接入真实 `createProjectVfsMountBinding` / `deleteProjectVfsMountBinding`。
- 文件：`packages/app-web/src/stores/projectStore.ts`
  - 新增 `filespacesByProjectId` / `vfsMountBindingsByProjectId` 与 `fetch*` / `invalidate*` 动作；
  - 创建 / 删除 binding / Filespace 后更新对应 map。
- `FilespaceCategoryPanel` / `VfsAccessPicker` 切换为读取 store。

### Step P2.4 — Mount Binding 管理 UI

- 文件：在 `packages/app-web/src/features/project-settings/` 或 `pages/ProjectSettingsPage.tsx` 内新增 `MountBindingsPanel`：
  - 列表 + 行内 capabilities / default_write 编辑（PUT）；
  - 解绑按钮（DELETE，带 ConfirmDeleteDialog）；
  - 顶部新建按钮 → `MountBindingCreateDialog`：
    - source kind 单选（Filespace / ExternalService）；
    - Filespace 子表单：select Filespace + mount_id / display_name / capabilities / default_write；
    - ExternalService 子表单：service_id / root_ref / mount_id / display_name / capabilities / default_write；
  - 创建 / 解绑 / 修改后调 `invalidateProjectVfsMountBindings`。
- ContextTab 接入 `MountBindingsPanel`：放在 `Project Filespace` 跳转入口与 `解析后的 VFS Mount` 之间。

### P2 收口

```bash
pnpm --filter app-web typecheck
pnpm --filter app-web lint
cargo check -p agentdash-api
cargo test -p agentdash-api project_filespaces
```

## P3 — 状态联动与一致性

### Step P3.1 — Picker 订阅 store

- 文件：`packages/app-web/src/features/project/agent-preset-editor/vfs-access-picker.tsx`
- 改动：直接 `const items = useProjectStore((s) => s.vfsMountBindingsByProjectId[projectId] ?? []);`；mount 时若为空则 `fetchProjectVfsMountBindings(projectId)`。

### Step P3.2 — 删除项 fallback 整理

- `FilespaceCategoryPanel` 删除当前 detail 项后 `setDetail({ kind: "closed" })`，不再 useMemo 回退。

### Step P3.3 — Lint warning 清理（如有）

- 仅修迁移相关文件引入的 warning，不做大范围 lint pass。

### P3 收口

```bash
pnpm --filter app-web typecheck
pnpm --filter app-web lint
cargo check -p agentdash-api
```

## 最终验收

- 启动 dev server，按 PRD Acceptance Criteria 走一遍：
  1. 创建 Filespace → 编辑文件 → 删除；
  2. 发布 Filespace 到 Marketplace → 在另一 Project 安装 → 在 Assets / Filespace 看到 marketplace 标签；
  3. AssetPickerDrawer 不展示 installed Filespace；
  4. 在 ProjectSettings 创建第二个 binding（Filespace 类型）→ 在 Agent 编辑器 VfsAccessPicker 立即可见；
  5. 解绑 binding → Filespace 仍存在；
  6. 在 VfsAccessPicker 看到加载文案；
  7. typecheck / lint / cargo check / cargo test 全绿。

## Rollback 点

- P0 完成即可单独提交，作为热修。
- P1 完成是另一个独立提交（仅前端）。
- P2 拆为后端路由 + 前端 UI 两个提交，方便回滚。
- P3 是收尾 polish，单独提交。

按这个粒度提交，单个 P 段问题不影响其它段。

## Required Spec Updates

执行结束后按 design.md 列出的 spec 文件补内容，作为 step 3.3 输入。
