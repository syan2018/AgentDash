# 统一 Assets 管理页（Workflow / Canvas / MCP Preset）

## Goal

把 Workflow/Canvas 从顶级 Tab 降级为"项目级可复用资产"，连同新增的 **MCP Preset**（单个 MCP Server 条目粒度）统一收入 Assets 页。本任务只做"资产视图 + 资产 CRUD（含 builtin）"，**agent 组装侧对 MCP Preset 的活引用接入拆到子任务**。

## Requirements

### 前端 Assets 页
- 新增 `/dashboard/assets` 路由，在 `frontend/src/App.tsx` 嵌套路由注册
- `workspace-layout.tsx` 顶级导航：新增 `Assets`；Workflow/Canvas 顶级入口移除（`/dashboard/workflow`、`/dashboard/canvas` 保留重定向到 Assets 对应子视图，避免旧深链失效）
- 布局：左类目列表（Workflow / Canvas / MCP Preset）+ 右资产列表 + 详情/预览面板
- 每条资产行：名称、简介、更新时间、**来源 chip（builtin / user）**
- 只读预览卡：
  - Workflow：DAG 缩略（静态图）+ step/edge 计数
  - Canvas：files 计数、bindings 计数、最近编辑时间
  - MCP Preset：transport 类型（http/sse/stdio）+ target/command 摘要 + env/header 数量
- 行动作：`编辑`（Workflow/Canvas 跳原 editor；MCP Preset 在 Assets 页内就地表单）、`复制`、`删除`（builtin 仅允许"复制为 user"）

### 后端 MCP Preset（新增）
- Domain 实体 `McpPreset`：`id`、`project_id`、`name`、`description`、`server_decl`（复用 `McpServerDecl`，http/sse/stdio 三种 transport）、`source`（`builtin` | `user`）、`builtin_key`（仅 builtin）、`created_at`、`updated_at`
- Postgres 迁移：`mcp_presets` 表；`(project_id, name)` 唯一约束
- Repository + Application service：CRUD + name 唯一性校验 + builtin seed 装载
- Builtin 机制对齐 Workflow：`crates/agentdash-application/src/mcp_preset/builtins/*.json` + `BuiltinSeed` 源标记；首批内置 2~3 个常用 server（如 filesystem / fetch / git），留 followup 扩充
- API routes：`/api/projects/:id/mcp-presets` CRUD + `GET` 支持按 source 过滤
- DTO：对齐 Canvas/Workflow 现有命名风格

### 后端 Workflow / Canvas（最小改动）
- 若现 list API 返回完整结构导致 Assets 列表加载过重，新增 summary DTO；否则复用
- **不改** Workflow/Canvas 的编辑路径与运行时

### 活引用语义铺垫（本任务只铺路，不接入 agent）
- MCP Preset 设计上支持活引用：主键稳定、name 可变、`source` 标记保留
- agent 侧接入在**子任务** `assets-mcp-preset-agent-binding` 完成

## Acceptance Criteria

- [ ] Assets Tab 可访问，三类资产列表分别渲染并响应 < 500ms
- [ ] Workflow 行"编辑"可正确拉起现 `workflow-editor.tsx` 并返回 Assets 不丢状态
- [ ] Canvas 行"编辑"可正确拉起现 Canvas panel 并返回 Assets
- [ ] MCP Preset CRUD 完整：创建 / 编辑 / 删除 / 复制，刷新后数据保留
- [ ] Builtin MCP Preset 显示为只读，"复制为 user" 可生成可编辑副本
- [ ] `/dashboard/workflow` / `/dashboard/canvas` 深链重定向到新 Assets 对应子视图
- [ ] 后端 `cargo test` + 前端 `tsc` / lint 全绿

## Definition of Done

- MCP Preset domain/persistence/api 单测 + 至少 1 条集成测试覆盖 CRUD 与 name 唯一性
- Postgres migration 干净 up/down
- `cargo clippy` / `tsc` / eslint 全绿
- Builtin MCP Preset 的 JSON schema 在 spec 里留一份定义（如有新约定触发 `/trellis:update-spec`）

## Technical Approach

### 分期 / PR 拆分

- **PR1 — 后端 MCP Preset 骨架**：domain + migration + repository + application service + builtin 装载 + 单测
- **PR2 — 后端 API + DTO**：routes + DTO + 集成测试
- **PR3 — 前端 Assets 壳 + 路由 + 导航调整 + 旧路径重定向**
- **PR4 — 前端三类资产列表 + 预览 + 跳转编辑接线**
- **PR5 — 前端 MCP Preset 就地表单 CRUD + builtin 只读态**

### 关键设计点

- **MCP Preset schema 复用 `McpServerDecl`**（`frontend/src/types/index.ts:164-177`），不另造一套 —— 对齐 agent-preset-editor 的 MCP server 编辑器字段，子任务接入时零映射成本
- **Builtin 机制对齐 Workflow**：`BuiltinSeed` 源标记 + `builtins/*.json` 装载；删除 builtin = 变为 "已覆盖"（预留，首版可不做 override）
- **活引用铺路**：主键稳定；子任务只需在 `AgentPreset.config.mcp_servers` 旁新增 `mcp_preset_refs: Vec<McpPresetRef>`，运行时展开
- **顶级导航调整**：Workflow/Canvas Tab 移除，路由保留重定向；Routine 不动

## Decision (ADR-lite)

**Context**：Workflow/Canvas 当前以独立顶级 Tab 存在，Workflow editor 尤其重（~3866 行）；同时缺失"项目级 MCP 共享模板"概念，每个 agent 各自配置 MCP server 导致重复。

**Decision**：
1. 新建 Assets Tab 作为项目级可复用资产的统一入口，Workflow/Canvas 降级
2. 资产管理与运行态分离：Assets 页只做 CRUD + 只读预览，重编辑跳转原 editor
3. 新增 MCP Preset（单 server 粒度，project 级），对齐 Workflow 的 builtin/user 来源模型
4. 本任务只做资产视图 + MCP Preset CRUD；agent 侧活引用接入拆子任务

**Consequences**：
- 顶级导航层级收敛，agent 配置面板后续能一键复用 MCP 模板
- 旧的 `/dashboard/workflow` 深链需要重定向维护，有小兼容成本
- Builtin MCP Preset 首批内置需定义好 schema，避免后续 schema 漂移

## Out of Scope

- Routine 纳入 Assets（另任务）
- MCP Preset Bundle（多 server 组合包）
- **Agent 组装面板接入 MCP Preset 活引用**（拆到子任务 `assets-mcp-preset-agent-binding`）
- 跨项目 Preset 共享 / 全局作用域
- Assets 页内承载 DAG 编辑 / Canvas runtime preview
- 与 `04-20-builtin-workflow-admin` 的合并

## Technical Notes

### 参考文件
- `frontend/src/App.tsx:220-232` — 路由注册
- `frontend/src/components/layout/workspace-layout.tsx` — 顶级导航
- `frontend/src/features/workflow/workflow-tab-view.tsx`、`workflow-editor.tsx` — 现 Workflow Tab（不动，被跳转拉起）
- `frontend/src/features/canvas-panel/CanvasTabView.tsx` — 现 Canvas Tab（不动，被跳转拉起）
- `frontend/src/features/project/agent-preset-editor.tsx:287-326` — MCP servers 现有编辑器（子任务用）
- `frontend/src/types/index.ts:164-177` — `McpServerDecl` schema（复用）
- `frontend/src/types/canvas.ts:23-35` — Canvas DTO
- `crates/agentdash-domain/src/common/agent_config.rs:35-56` — `AgentConfig` / `tool_clusters`
- `crates/agentdash-application/src/workflow/{catalog,definition,builtins}` — Workflow 后端（builtin 装载模式参考）
- `crates/agentdash-api/src/dto/canvas.rs`、`crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs` — Canvas 后端（repository 模式参考）

### 关联任务
- 并行：`04-20-builtin-workflow-admin`（不合并）
- 子任务：`assets-mcp-preset-agent-binding`（Agent 组装面板接入活引用）
- 相关：`04-20-dynamic-capability-followup`
