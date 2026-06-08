# Design · Workspace Module 集成 review + 项目层管理 UI + 收尾

> Parent design §9-10。研究依据：本任务 `research/01..06`。Child 1/2 已落地契约/聚合/四工具/capability 维度/AgentFrame 预留字段（DB 列缺）。

## 1. 范围

1. 后端：新增 `GET /projects/{id}/workspace-modules` 供 UI 复用同一 projection（D5）。
2. DB：补 `visible_workspace_module_refs_json` 列，闭合 AgentFrame 预留字段持久化（Child 2 check 标记的缺口）。
3. 前端：项目设置页新增 WorkspaceModule 合并认知区块（Canvas + Extension 统一列出）。
4. 集成 review：确认 trace/provenance/ContextFrame 一致（研究/05：无遗漏，验证性）。
5. 收尾：parent slug 改名 + docs 补 Workspace Module 章节。

## 2. 后端路由（研究/01）

`crates/agentdash-api/src/routes/workspace_module.rs`（新）：

- `GET /projects/{project_id}/workspace-modules` → handler 取 enabled installations（`list_enabled_by_project`）+ canvases（`list_project_canvases`）→ `extension_runtime_projection_from_installations` + `build_workspace_modules` → `Json(Vec<WorkspaceModuleDescriptor>)`。
- **无需 mapper**：`WorkspaceModuleDescriptor` 本身就是 contract 类型，直接序列化。
- 路由注册进现有 router（与 extension_runtime 同注入 repos）。
- 这是 Child 1 §8 预留的 UI 数据出口；与 Agent 工具复用同一聚合函数（单一 canonical）。

## 3. DB 列（研究/04，关键：4 处 SQL + TryFrom + struct 全同步）

- 新建 migration `0008_agent_frame_visible_workspace_modules.sql`（guard 仅放行 status `A`）：`ALTER TABLE lifecycle_anchor ADD COLUMN visible_workspace_module_refs_json JSONB`（可空，与 canvas 列同构）。
- `lifecycle_anchor_repository.rs` 同步：
  - `FrameRow` struct 加字段。
  - 3 个 SELECT 列清单加列。
  - INSERT `$12` → 加 `$13`。
  - `TryFrom<FrameRow>` 行 212 把硬编 `None` 改为 `parse_opt_json`（镜像 canvas）。
  - frame 持久化/更新写入点镜像 `visible_canvas_mount_ids` 的 UPDATE 写法。
- 漏一处 SELECT 会 `FromRow` panic —— 实现时 grep 全列出确保 4 处一致。

> 闭合后：AgentFrame allowlist → `WorkspaceModuleDimension`（capability 链路 frame_construction:363 已通）→ 工具过滤，端到端可持久化。

## 4. 前端（研究/02-03）

- `features/workspace-module/model/`（新）：`workspaceModuleStore`（zustand，`byProjectId` + inflight 去重，镜像 extensionRuntimeStore）+ `useProjectWorkspaceModules(projectId)` hook，`api.get<WorkspaceModuleDescriptor[]>("/projects/{id}/workspace-modules")`，用生成类型 `workspace-module-contracts.ts`。
- `ProjectSettingsPage.tsx` 新增 `SectionCard`「Workspace Modules」：合并列出 extension + canvas module，每行示 kind / title / source / status（unavailable 显 reason）/ operations 数 / ui_entries 数。复用现有 `SectionCard`/`ContentGroup`。
- **管理动作**：enable/disable 复用现有 extension installation 管理（不重建）；本区块聚焦"合并认知"（统一可见 + 状态 + 诊断）。

## 5. 不做 / 边界（明确）

- **不做 per-frame 裁切编辑 UI**：`visible_workspace_module_refs` 是 AgentFrame 级（非项目级），编辑入口属 agent frame 编辑面，与项目设置页不同 surface。本 child 只补**持久化 + 项目层合并认知视图**；裁切默认 all，allowlist 持久化通路已就绪，编辑 UI 列为后续。这与 D5"项目层合并认知与管理"一致——用户诉求是合并视图，不是 per-frame allowlist 编辑器。
- 不改 Agent 工具行为（Child 1/2 已定）。

## 6. 改名收尾（研究/06，纯元数据）

- parent 任务目录 `06-08-agent-runtime-surface-registry` → `06-08-workspace-module-registry`：fs rename（未被 git 跟踪 → 无历史包袱）+ parent task.json `id`/`name` + 3 child 的 `parent` 字段 + 各 check.jsonl 中引用。
- 代码里**无** `surface-registry` slug；代码中的 `Surface`/`surface` 是底层 runtime projection 命名，按决策**保留不动**。
- `docs/extension-system.md` 补 Workspace Module 章节：定义、与 Runtime Surface 边界、四工具、与 extension/canvas 关系。

## 7. 验收对应

| 验收 | 落点 |
|---|---|
| 设置页列出并管理 ext+canvas module，复用 Child 1 projection 无第二份 DTO | §2 route + §4 UI |
| 可见性裁切经 capability 在 agent 侧生效（list/describe 反映） | §3 DB 闭合 + 既有 capability 链路 |
| trace/权限/ContextFrame 一致，present/unavailable 有诊断 | §1.4 验证 + Child 2 已实现 |
| slug 改名 + 文档术语边界 | §6 |
| 前后端类型生成 + e2e | 全量验证 |
