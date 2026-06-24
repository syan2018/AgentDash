# Canvas 个人与共用分发权限系统实施计划

## Preconditions

- 确认 Canvas identity 收束任务的最终字段命名：`canvas_id`、`canvas_mount_id`、`vfs_mount_id`、`canvas://...`、`canvas:{...}`。
- 确认 AgentFrame Canvas projection 收束任务已提供统一 runtime surface 更新路径；若尚未完成，本任务不得新增第二条 Canvas expose/adopt 路径。
- 确认 MVP 范围只覆盖 Project 内发布/复制，Shared Library `canvas_template` 另开后续任务。
- 当前任务分支：`codex/canvas-personal-shared-distribution-permission`。

## Optimized Parallelization Strategy

本任务最适合采用“串行打底、并行推进、串行收口”的节奏。数据库/domain/contract 是并行工作的地基，必须先稳定；之后 VFS/WorkspaceModule 和前端可以在明确 DTO 与 access projection 后并行；最终由主会话统一做 contract drift、权限矩阵和端到端验证。

### Phase A: Serial Foundation

必须由主会话或单一后端 worker 先完成，避免多个 worker 同时改同一批核心事实：

- Canvas domain 字段、value object、access projection。
- Migration 与 `PostgresCanvasRepository` row mapping。
- Canvas DTO contract 的最终字段名。
- Application use case 函数签名：list/publish/copy/unpublish/load-with-access。

Phase A 输出稳定后，其他 worker 才能安全并行。输出物必须包括：

- `CanvasScope` / access projection 类型。
- `CanvasResponse` 新字段。
- publish/copy/unpublish use case 的输入输出。
- 只读 runtime write flag 的数据来源。

### Phase B: Parallel Tracks

Phase A 稳定后并行拆四条线：

| Track | Worker | Scope | Depends On | Output |
| --- | --- | --- | --- | --- |
| B1 | Runtime surface worker | Canvas mount builder、provider write guard、session exposure access | Phase A access projection | read-only VFS tests |
| B2 | WorkspaceModule worker | descriptor operation裁切、tool mutation guard、present/create语义 | Phase A use cases + B1 mount access shape | workspace_module tests |
| B3 | API/contract worker | routes、contract generation、API tests | Phase A DTO/use cases | generated TS + API tests |
| B4 | Frontend worker | Canvas service、Mine/Shared UI、read-only editor state | Phase A DTO + B3 generated TS shape | app-web tests |

B1 与 B2 都会碰 Canvas runtime surface，分工边界要清楚：B1 只负责 VFS mount/provider 能力，B2 只负责 WorkspaceModule operation 和 agent tool admission。

B3 与 B4 可以并行，但 B4 在生成 TS contract 前只能做结构准备，不能围绕临时字段写死 mapper。

### Phase C: Serial Integration

最后由主会话统一收口：

- 跑 contract check，确认 Rust DTO 与 TS generated types 一致。
- 检查三条写路径权限一致：
  - HTTP `PUT/DELETE`
  - VFS `write/delete/rename`
  - WorkspaceModule `canvas.bind_data`
- 检查只读 Canvas 仍可 preview/present/read binding。
- 更新 specs 和 `canvas-system` skill。
- 执行最终验证命令，整理提交。

## Suggested Commit Slices

提交要按“可独立 review、可局部回滚”的边界切，不按文件类型机械切。推荐提交序列：

1. `feat(canvas): 建立个人与项目共用Canvas领域模型`
   - Canvas scope/owner/lineage value object。
   - access projection。
   - migration 与 repository mapping。
   - domain/repository tests。

2. `feat(canvas): 增加发布与复制应用服务`
   - list/load-with-access。
   - publish-to-project / copy-to-personal / unpublish use cases。
   - mount id 唯一生成。
   - application tests。

3. `feat(api): 暴露Canvas分发权限接口`
   - Canvas DTO 新字段。
   - list scope filter。
   - publish/copy/unpublish routes。
   - update/delete 权限改为 Canvas access。
   - generated TS contracts。

4. `feat(runtime): 按Canvas访问权限裁切VFS与WorkspaceModule`
   - Canvas mount capabilities read-only 裁切。
   - provider write/delete/rename guard。
   - workspace module operation 裁切。
   - runtime/workspace_module tests。

5. `feat(frontend): 支持Canvas个人与项目共用资产视图`
   - Canvas services/types。
   - Mine/Shared UI。
   - 只读 detail/editor 状态。
   - copy/publish/unpublish 操作。
   - frontend tests。

6. `docs(canvas): 记录Canvas分发权限与只读运行面语义`
   - Trellis spec updates。
   - Canvas system skill update。
   - Shared Library future path note。

如实现过程中某个提交过大，优先把测试随对应功能提交，不把“所有测试”堆到最后。migration 与 repository mapping 必须同提交，避免中间状态无法启动。

## Pre-Start Preparation Checklist

- [x] 创建任务分支 `codex/canvas-personal-shared-distribution-permission`。
- [x] 将 Trellis task branch 设置为 `codex/canvas-personal-shared-distribution-permission`。
- [x] 用户确认 PRD / design / implement 的 MVP 范围。
- [x] 在进入实现前检查并记录两个依赖任务状态：
  - `.trellis/tasks/06-23-canvas-vfs-runtime-binding-convergence`
  - `.trellis/tasks/06-23-agentframe-canvas-projection-convergence`
- [x] 启动前执行一次 `git status --short --branch`，确认只处理本任务文件和后续实现文件，不碰并行会话改动。
- [x] 若使用子代理，分发 prompt 必须包含本文件的 Phase A/B/C 顺序，避免前端或 runtime worker 抢先定义临时契约。

Dependency status checked on 2026-06-24:

- `.trellis/tasks/06-23-canvas-vfs-runtime-binding-convergence`: `in_progress`。
- `.trellis/tasks/06-23-agentframe-canvas-projection-convergence`: `in_progress`。
- 本任务以 design 中记录的最终 `canvas_id` / `canvas_mount_id` / `vfs_mount_id` 和统一 projection 语义继续推进，不新增第二条 Canvas expose/adopt 路径。

## Implementation Checklist

### 1. Domain And Migration

- [ ] 在 Canvas domain 增加 scope、owner、publish/copy lineage 字段与 value object。
- [ ] 增加 Canvas access projection 类型，集中表达 view/edit/publish/manage/copy/runtime write 能力。
- [ ] 更新 `Canvas::new` 或 builder，使新建 Canvas 默认是 current user 的 personal Canvas。
- [ ] 增加 migration，补齐 `canvases` owner/scope/lineage/publish 字段和索引。
- [ ] 迁移既有 Canvas 到最终模型，优先保证现有项目 Canvas 可读可用。
- [ ] 更新 `PostgresCanvasRepository` row mapping、insert、update、list 查询。
- [ ] 更新 repository trait 和测试 fake repo。

### 2. Application Use Cases

- [ ] 新增 `list_canvases_for_user`，支持 mine/shared/all 或等价 filter。
- [ ] 新增 `load_canvas_with_access`，所有 Canvas API 和 runtime mutation 入口复用它。
- [ ] 新增 `publish_canvas_to_project`，从 personal Canvas deep copy 出 project shared Canvas。
- [ ] 新增 `update_project_canvas_publication` 或等价覆盖发布路径。
- [ ] 新增 `copy_canvas_to_personal`，从 project shared Canvas deep copy 出 current user personal Canvas。
- [ ] 新增 `unpublish_project_canvas`，管理项目共用 Canvas 可见性或删除共用记录。
- [ ] 抽出 Canvas payload copy helper，统一复制 files、bindings、sandbox、entry 和 title/description。
- [ ] 抽出 unique `canvas_mount_id` generator，避免复制/发布时 mount 冲突。

### 3. API Routes And Contracts

- [ ] 扩展 Rust `agentdash-contracts` Canvas DTO：scope、owner、access、lineage、publish metadata。
- [ ] 修改 `GET /projects/{project_id}/canvases` 支持 scope filter。
- [ ] 修改 `POST /projects/{project_id}/canvases` 创建 personal Canvas。
- [ ] 修改 `PUT /canvases/{id}`：只允许 `can_edit_source` 的 Canvas。
- [ ] 修改 `DELETE /canvases/{id}`：按 personal owner / project shared manager 规则执行。
- [ ] 新增 publish-to-project route。
- [ ] 新增 copy-to-personal route。
- [ ] 新增 unpublish route。
- [ ] 确保 `promote-extension` 文案和 route 仍只表示发布为插件，不混入项目共用发布。
- [ ] 重新生成 TypeScript contracts。

### 4. VFS And Runtime Surface

- [ ] 修改 Canvas mount builder，接收 effective access 或 runtime write flag。
- [ ] 只读 Canvas mount capabilities 只包含 read/list/search。
- [ ] 修改 `CanvasFsMountProvider::edit_capabilities`，按 mount capability 返回 create/delete/rename。
- [ ] 修改 `write_text`、`delete_text`、`rename_text`，缺少 write capability 时返回一致的 forbidden/not supported 用户语义。
- [ ] 修改 session Canvas visibility append/expose 路径，按 current user/session identity 计算 Canvas access。
- [ ] 增加 tests 覆盖 project shared Canvas 进入 runtime surface 时没有 write capability。
- [ ] 增加 tests 覆盖只读 mount 的 write/delete/rename 被拒绝。

### 5. WorkspaceModule And Agent Tools

- [ ] 修改 Canvas workspace module builder，使其接收 access projection。
- [ ] editable personal Canvas 暴露 `canvas.bind_data`。
- [ ] read-only project shared Canvas 不暴露 `canvas.bind_data`。
- [ ] 修改 `workspace_module_create(kind="canvas")` 默认创建 personal Canvas。
- [ ] 修改 `workspace_module_present` 对 shared Canvas 只做 presentation/exposure，不授予写能力。
- [ ] 修改 `workspace_module_invoke` 的 host Canvas mutation 分支，调用前检查 `can_edit_source`。
- [ ] 更新 workspace module tests，覆盖 shared Canvas descriptor operations 裁切。

### 6. Frontend Canvas Assets UI

- [ ] 更新 `packages/app-web/src/types/canvas.ts` facade。
- [ ] 更新 `packages/app-web/src/services/canvas.ts`，新增 publish/copy/unpublish service。
- [ ] 重构 `ProjectCanvasManager`，区分 Mine 与 Shared 视图或分组。
- [ ] Mine 列表显示创建、编辑、发布、删除。
- [ ] Shared 列表显示打开/预览、复制为我的 Canvas；管理者显示取消发布/删除共用源。
- [ ] Canvas detail/runtime panel 根据 access 隐藏或禁用文件编辑与 binding 编辑入口。
- [ ] 复制成功后刷新列表并选中新 personal Canvas。
- [ ] 更新 Canvas 资产页文案，区分“发布到项目共用”和“发布为插件”。
- [ ] 更新前端 tests 和 fixtures。

### 7. Specs And Skills

- [ ] 更新 `.trellis/spec/backend/vfs/vfs-access.md`：Canvas read-only runtime mount 语义。
- [ ] 更新 `.trellis/spec/cross-layer/shared-library-contract.md`：记录 Canvas project-shared MVP 与未来 `canvas_template` 路径。
- [ ] 更新 `.trellis/spec/cross-layer/frontend-backend-contracts.md`：Canvas DTO access/scope/lineage 字段。
- [ ] 更新 `.trellis/spec/frontend/type-safety.md` 或前端相关 appendix：Canvas access 驱动 UI 状态。
- [ ] 更新 `crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md`：只读共用 Canvas 复制后编辑。
- [ ] 如 WorkspaceModule skill 存在 Canvas 描述，也同步更新只读 operation 裁切语义。

### 8. Validation And Review

- [ ] Rust format/check。
- [ ] Migration guard。
- [ ] Contract generation/check。
- [ ] Backend Canvas domain/application/API tests。
- [ ] VFS provider read-only Canvas tests。
- [ ] WorkspaceModule operation裁切 tests。
- [ ] Frontend Canvas assets UI tests。
- [ ] Cross-layer review：HTTP、VFS、WorkspaceModule 三条写路径权限一致。

## Suggested Sub-Agent Split

- Backend domain/migration worker：Canvas entity/value objects/repository/migration/API DTO。
- Runtime surface worker：VFS mount builder/provider、session exposure、WorkspaceModule operation裁切。
- Frontend worker：Canvas services/types、ProjectCanvasManager Mine/Shared UI、runtime panel read-only state。
- Check worker：contract drift、权限一致性、spec compliance、测试缺口。

每个子代理 prompt 必须以 `Active task: .trellis/tasks/06-24-canvas-personal-shared-distribution-permission` 开头。

推荐调度：

- 第一批只派 Backend domain/migration worker，完成 Phase A。
- 第二批并行派 Runtime surface worker、WorkspaceModule worker、API/contract worker。
- 第三批在 generated TS 稳定后派 Frontend worker。
- 最后派 Check worker 做跨层一致性审查。

## Validation Commands

- `cargo fmt`
- `git diff --check`
- `cargo check -p agentdash-domain -p agentdash-application -p agentdash-api -p agentdash-infrastructure -p agentdash-contracts`
- `cargo test -p agentdash-domain canvas`
- `cargo test -p agentdash-application canvas`
- `cargo test -p agentdash-application workspace_module`
- `cargo test -p agentdash-api canvases`
- `pnpm run contracts:check`
- `pnpm --filter app-web run check`
- 具体前端单测命令在实现前按 `packages/app-web/package.json` 确认。

## Risky Files

- `crates/agentdash-domain/src/canvas/entity.rs`
- `crates/agentdash-domain/src/canvas/value_objects.rs`
- `crates/agentdash-domain/src/canvas/repository.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs`
- `crates/agentdash-infrastructure/migrations/*.sql`
- `crates/agentdash-application/src/canvas/management.rs`
- `crates/agentdash-application/src/canvas/tools.rs`
- `crates/agentdash-application/src/canvas/visibility.rs`
- `crates/agentdash-application/src/vfs/mount_canvas.rs`
- `crates/agentdash-application/src/vfs/provider_canvas.rs`
- `crates/agentdash-application/src/workspace_module/mod.rs`
- `crates/agentdash-application/src/workspace_module/tools.rs`
- `crates/agentdash-api/src/routes/canvases.rs`
- `crates/agentdash-contracts/src/surface/canvas.rs`
- `packages/app-web/src/services/canvas.ts`
- `packages/app-web/src/types/canvas.ts`
- `packages/app-web/src/features/canvas-panel/ProjectCanvasManager.tsx`
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePanel.tsx`
- `packages/app-web/src/features/canvas-panel/CanvasFilesEditor.tsx`
- `packages/app-web/src/features/canvas-panel/CanvasBindingsEditor.tsx`
- `packages/app-web/src/features/assets-panel/categories/CanvasCategoryPanel.tsx`

## Review Gates Before `task.py start`

- PRD/design/implement 已由用户确认。
- MVP 范围未包含 Shared Library `canvas_template`。
- 当前工作区其他未提交修改与本任务无冲突。
- 相关 Canvas identity/projection 任务状态已检查，实施计划按其最终契约执行。
- 提交切片已确认按上方 6 段推进；如实现时需要拆分，仍保持 migration/repository、API/contract、runtime、frontend、docs 的 review 边界。

## Rollback Points

- Migration 完成前：仅修改 domain/application/API planning code，可整体回退本任务改动。
- Migration 完成后：若 access projection 或 VFS 裁切失败，优先暂停实现并修正 migration/model，不引入兼容双轨。
- Frontend UI 改造前：确保后端 API contract 和 generated TS 已稳定，避免前端围绕临时字段返工。
