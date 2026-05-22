# Project Filespace 前端能力修复

## Goal

修复 `b362100c` 迁移 Project Filespace / Agent VFS 能力时引入的前端落地缺陷：让 Filespace 类目与同 Assets 页其它类目（Skill / MCP / Workflow）行为一致、状态正确、契约对齐，并补齐迁移设计中要求但实际未实现的 mount binding 管理与 API 路由。

## User Value

- Assets / Filespace 类目能正确渲染反馈、显示来源（用户创建 / Marketplace 安装 / 已发布），与 Skill 类目同款交互。
- Project Agent VFS 能力分配在 Project 内 Filespace 变化后能及时刷新，加载状态可见。
- 用户可独立管理 Project VFS Mount Binding（解绑、改 capabilities、绑 external_service），不用绕到 Filespace 删除来强制解绑。
- typecheck / lint / 关键行为测试恢复绿色，未来再次回归同类问题时不会被静默合并。

## Confirmed Facts

- `b362100c` 完成 Project Filespace 域模型、迁移、API、Marketplace publish/install 后端能力，以及前端 Assets 类目壳。
- `packages/app-web/src/features/assets-panel/_shared/Notice.tsx` 的 `NoticeProps` prop 名为 `notice`；其它类目（Skill / MCP / Workflow）按此调用。
- `FilespaceCategoryPanel.tsx` 误用 `data` prop，导致 `pnpm typecheck` 在 `app-web` 包失败。
- `crates/agentdash-api/src/routes/project_filespaces.rs` 的 `ProjectFilespaceResponse` 未透出 `installed_source`，前端 `packages/app-web/src/types/index.ts` 的 `ProjectFilespace` 也无此字段。
- `crates/agentdash-api/src/routes.rs` 仅暴露 `GET /vfs-mount-bindings`、`PUT /{binding_id}`，没有 POST / DELETE。
- `VfsAccessPicker` 加载状态从未被置为 `true`；其数据获取仅依赖 `projectId`，不响应 Filespace / Binding 变化。
- 域模型 `ProjectFilespace.installed_source` 已经存在 (`crates/agentdash-domain/src/project_filespace/entity.rs`)；后端只是在 DTO 转换时丢字段。
- Skill / MCP / Workflow 类目均使用 `CardMenu` / `OriginBadge` / `PublishedBadge` / `Notice` / Editor Dialog / `ConfirmDeleteDialog` 等共享 UI atoms，定义在 `packages/app-web/src/features/assets-panel/_shared` 与 `@agentdash/ui`。
- `AssetPickerDrawer` 的 `FilespaceList` 当前未按 `installed_source` 过滤，存在让安装资产被重复发布的风险。

## Requirements

### P0 — 阻塞编译 / 数据完整性

- 修正 `FilespaceCategoryPanel` 的 `<Notice>` 调用，使 `pnpm typecheck` 在 `app-web` 通过。
- 后端 `ProjectFilespaceResponse` 暴露 `installed_source`；前端 `ProjectFilespace` 类型同步补字段，并在 service / category panel / picker 等所有消费点保留该字段。
- 修正 `VfsAccessPicker` 的加载状态：进入 effect 立即 `setIsLoading(true)`，错误态下也要 finally 关闭。

### P1 — 与其它 Assets 类目保持一致

- `FilespaceCategoryPanel` 改造为与 `SkillCategoryPanel` 同骨架：
  - 顶部 `CreateButton` + 简短统计；
  - Card Grid（响应式 `sm:grid-cols-2 xl:grid-cols-3`）；
  - 每张卡片用 `OriginBadge` 标识 user / installed / builtin（builtin 暂时不存在 → 不展示），用 `PublishedBadge` 标识 user_authored 已发布版本；
  - 通过 `CardMenu` 暴露「编辑 / 发布到资源市场 / 删除」；
  - 删除走 `ConfirmDeleteDialog` 而不是 `window.confirm`；
  - 创建 / 编辑走 Editor Dialog（不是 inline VfsBrowser），dialog 内仍复用 `VfsBrowser` 浏览 `project_filespace` 文件；
  - 复用 `Notice` 的 `notice` / `clearNotice` / `showSuccess` / `showError` 模式。
- 接入已有但被遗忘的 `updateProjectFilespace`：编辑 dialog 提供 display_name / key / description 编辑入口（key 改名要求后端同时维护对应 mount binding，由后端实现）。
- `AssetPickerDrawer.FilespaceList` 过滤 `installed_source`，避免重复发布安装来的 Filespace。
- `AssetsTabView` 顶部描述补 Filespace。
- Filespace 类目页应展示 `已发布 → marketplace key/版本` 信息，使 Skill 那套 `fetchLibraryAssets({ asset_type: "filespace_template", owner_id })` 流程对 Filespace 同样可用。

### P2 — Project VFS Mount Binding 管理闭环

- 后端补齐路由：
  - `POST /api/projects/{project_id}/vfs-mount-bindings`：创建 binding，支持 `Filespace` / `ExternalService` 两种 source。后端校验 mount_id 唯一、不占用保留字、capabilities 在 provider 支持范围内。
  - `DELETE /api/projects/{project_id}/vfs-mount-bindings/{binding_id}`：单独解绑，不删除底层 Filespace。
  - 现有 `PUT` 仍允许全字段覆盖；不引入 PATCH。
- 前端服务层补 `createProjectVfsMountBinding` / `deleteProjectVfsMountBinding`。
- 在 ProjectSettings Context Tab 与 / 或 Filespace 类目下提供 Mount Binding 管理入口：
  - 列表展示 mount_id / display_name / source 摘要 / capabilities / default_write；
  - 支持新建（选择 Filespace 或 external_service provider）；
  - 支持解绑（带确认）；
  - 支持改 capabilities / default_write；
  - 解绑或修改后通知 `VfsAccessPicker` 刷新。
- Filespace 创建后自动生成同名 binding 的逻辑保留；当用户多挂一份 binding 时不重复创建。

### P3 — 状态联动与一致性

- 新建 / 删除 Filespace 或 mount binding 后，`VfsAccessPicker` 在 Agent 编辑器打开期间能感知刷新（推荐用 zustand store 暴露 `bindingsByProjectId` + 显式 `invalidate(projectId)`，或基于 mount binding query 的 reload tick；具体方式由 design 决定）。
- 整理 `FilespaceCategoryPanel` 的 selectedId fallback：删除项后 detail 区直接收起，不再落到 `items[0]`，避免闪烁。
- 修复 typecheck 之外的 lint warning（如有），保持仓库现有 lint pass 状态。

## Acceptance Criteria

- [ ] `pnpm --filter app-web typecheck` 通过。
- [ ] `pnpm --filter app-web lint` 通过（不引入新 warning）。
- [ ] `cargo check -p agentdash-api` 通过。
- [ ] Filespace 类目 UI 风格与 Skill 类目一致：Card Grid + CardMenu + OriginBadge + PublishedBadge + Editor Dialog + ConfirmDeleteDialog + Notice。
- [ ] Filespace 创建 / 编辑 / 删除 / 发布 / 安装来源识别 5 种用户路径的成功 / 失败反馈正确显示。
- [ ] Marketplace 安装来的 Filespace 不能再次被「发布」；`AssetPickerDrawer` 不展示带 `installed_source` 的 Filespace。
- [ ] Mount Binding：用户可在不删除 Filespace 的前提下创建第二个 binding、解绑、修改 capabilities / default_write；UI 在 binding 变化后及时反映。
- [ ] Project Agent 编辑器中的 `VfsAccessPicker`：加载文案在加载期间显示；新建 / 删除 Filespace 或 binding 后能在不重新挂载组件的情况下刷新列表。
- [ ] `ProjectFilespaceResponse` 在 user_authored / installed / 未安装三种场景下都能正确展示来源信息。
- [ ] 后端：`POST /vfs-mount-bindings`、`DELETE /vfs-mount-bindings/{binding_id}` 行为有单元 / 集成测试覆盖；mount_id 冲突、删除关联 Filespace 时的 binding 级联仍然正确。
- [ ] 不重新引入旧 `project_container_ids` 字段；不破坏既有 `vfs_access_grants` 持久化形态。

## Out Of Scope

- 不重做 Project Filespace 域模型、不调整 `inline_fs_files` 存储。
- 不引入 binding 排序 / 优先级 / 标签系统。
- 不做 Marketplace 端的 UI 改造（除 AssetPickerDrawer 过滤外）。
- 不优化 Filespace 文件浏览器的内核交互（VfsBrowser 自身行为）。
- 不引入跨 session 实时刷新（仍走当前 fetch + invalidate 模型）。

## Open Questions

- Mount Binding 管理 UI 放置位置：放在 ProjectSettings Context Tab、Assets / Filespace detail 子面板，还是单独类目？设计阶段决定。
