# Implement · Workspace Module 集成 review + 管理 UI + 收尾

> 依据 `design.md`。有序执行，每步验证。

## 步骤

1. **后端路由**（`crates/agentdash-api/src/routes/workspace_module.rs` 新）
   - `GET /projects/{project_id}/workspace-modules` → enabled installations + canvases → `build_workspace_modules` → `Json(Vec<WorkspaceModuleDescriptor>)`，无 mapper。
   - 注册路由（样板 extension_runtime.rs）。
   - 验证：`cargo build -p agentdash-api`

2. **DB 列**（migration + repository）
   - 新建 `crates/agentdash-infrastructure/.../migrations/0008_agent_frame_visible_workspace_modules.sql`（ADD COLUMN JSONB 可空）。
   - `lifecycle_anchor_repository.rs`：FrameRow struct + 3 SELECT + INSERT $13 + TryFrom parse_opt_json + UPDATE 写入点，全部镜像 `visible_canvas_mount_ids`。grep 确认无漏。
   - 验证：`cargo build -p agentdash-infrastructure && node scripts/check-migration-history.js`

3. **前端 store + hook**（`packages/app-web/src/features/workspace-module/model/`）
   - `workspaceModuleStore`（zustand byProjectId + inflight）+ `useProjectWorkspaceModules`，用生成类型。
   - 验证：`pnpm --filter app-web typecheck`

4. **前端设置页区块**（`ProjectSettingsPage.tsx`）
   - SectionCard「Workspace Modules」合并列 kind/title/source/status/ops 数；unavailable 显 reason。
   - 验证：`pnpm --filter app-web typecheck && pnpm --filter app-web build`（若 build 慢可跳，typecheck 必过）

5. **集成 review（验证性）**
   - 确认 invoke/present provenance+trace 已记录（Child 2）；present 诊断事件前端已接（Child 2）；无需新增代码，列出确认点。

6. **改名 + docs**
   - parent 目录 rename + task.json id/name + 3 child parent 字段 + check.jsonl 引用。
   - `docs/extension-system.md` 补 Workspace Module 章节。
   - 验证：`python ./.trellis/scripts/task.py current`（确认任务树未断）

7. **全量验证**
   ```powershell
   cargo build --workspace
   cargo test -p agentdash-infrastructure
   cargo test -p agentdash-application workspace_module
   pnpm contracts:check
   pnpm --filter app-web typecheck
   ```

## 回滚点

- 步 2 DB 列：4 处 SQL 任一漏改 → FromRow panic，回退到逐处 grep 核对。
- 步 6 改名最后做、单独提交，便于回退。

## Review gate

- 步 1/2 后端通后确认，再做前端。
