# Research: workspace_module canvas implementation touchpoints

- Query: 基于当前代码和任务 prd/design/implement，梳理实现 `workspace_module_create(kind="canvas")` 与 Canvas instance-first 收束的关键后端、前端、skill/spec 触点。
- Scope: internal
- Date: 2026-06-10

## Findings

### 任务结论

本任务应把 Canvas 的 Agent-facing 创建、绑定、展示入口从独立 `canvas` capability 收束到 `workspace_module` capability。现有代码已经有 `canvas:{mount_id}` module 投影、`workspace_module_list` / `describe` / `invoke` / `present` 工具和前端 `workspace_module_presented` 事件处理，但 Canvas module 当前无 invokable operation，`workspace_module_present` 只发 UI 事件，不复用 `present_canvas` 的 session exposure 语义。

实现时最关键的收束点是把 `canvas/tools.rs` 中“创建/接入 + 暴露 session 编辑面”“绑定数据”“展示前刷新 session runtime surface”的业务逻辑抽成 application use case，然后由 `workspace_module_create`、`workspace_module_invoke(canvas:{mount_id}, canvas.bind_data)`、`workspace_module_present(canvas:{mount_id}, ...)` 调用。

### Files Found

- `crates/agentdash-application/src/canvas/tools.rs` - 旧 Canvas Agent tools；包含 create/attach、bind、present 与 session exposure 逻辑。
- `crates/agentdash-application/src/workspace_module/mod.rs` - workspace module 聚合投影；当前把 Canvas 投影为 `canvas:{mount_id}`，但 operations 为空。
- `crates/agentdash-application/src/workspace_module/tools.rs` - workspace module Agent tools；已有 list/describe/invoke/present，缺 create，present 当前只发事件。
- `crates/agentdash-contracts/src/workspace_module.rs` - Rust/TS 共享 contract；需要新增 create request/result 与 host-owned Canvas dispatch/presentation 字段。
- `crates/agentdash-application/src/vfs/tools/provider.rs` - runtime tool provider；当前同时注入 Canvas cluster 与 WorkspaceModule cluster。
- `crates/agentdash-spi/src/platform/tool_capability.rs` - well-known capability、tool cluster、tool descriptors、visibility rules 的权威定义。
- `crates/agentdash-spi/src/connector/mod.rs` - `ToolCluster` / `CapabilityState`；`CapabilityState::all()` 当前仍包含 `ToolCluster::Canvas`。
- `crates/agentdash-application/src/session/plan.rs` - 默认 session plan 的 conditional flow tools；当前仍硬编码 Canvas 专用工具。
- `crates/agentdash-application/src/session/capability_state.rs` - `visible_workspace_module_refs` 到 `WorkspaceModuleDimension` 的投影入口。
- `crates/agentdash-application/src/canvas/visibility.rs` - session construction 时根据 visible canvas mount ids 追加 Canvas VFS mount。
- `crates/agentdash-application/src/vfs/mount.rs` / `crates/agentdash-application/src/vfs/path.rs` - Canvas VFS mount id/root_ref 约定。
- `crates/agentdash-api/src/routes/workspace_module.rs` - Project Settings HTTP projection；复用 `build_workspace_modules`。
- `packages/app-web/src/pages/SessionPage.tsx` - 前端 session event 处理；当前同时处理 `canvas_presented` 与 `workspace_module_presented`。
- `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx` - 仍通过 `activeCanvasId` 自动打开 Canvas tab。
- `packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx` - Canvas tab `canvas://{id}` URI 解析/构造入口。
- `packages/app-web/src/types/index.ts` / `packages/app-web/src/types/workflow.ts` - 前端 capability picker / well-known capability key 列表仍包含 `canvas`。
- `packages/app-web/src/features/project/agent-preset-editor/form-state.ts` - ProjectAgent preset capability_directives 前端 roundtrip。
- `crates/agentdash-domain/src/canvas/value_objects.rs` - `canvas-system` embedded bundle 声明。
- `crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md` - 当前仍指导 Agent 直接调用旧 Canvas 工具。
- `crates/agentdash-domain/src/companion/value_objects.rs` / `crates/agentdash-application/src/companion/skill_projection.rs` - `companion-system` 项目级 builtin SkillAsset + lifecycle projection 可作为 `workspace-module-system` 模板。
- `crates/agentdash-application/src/skill_asset/definition.rs` / `service.rs` - builtin skill template 注册与 `bootstrap_builtins(project_id, Some(key))` 入口。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` / `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs` - `project_agents.config` 存储形态与迁移目标字段。

### Code Patterns

#### 1. Canvas create / present / session exposure 如何抽出复用

- `StartCanvasTool` 的 `execute` 已完整表达 create-or-attach：先按 `canvas_id` 通过 `get_by_mount_id` 接入已有 Canvas，找不到则调用 `build_canvas` 并 `canvas_repo.create` 新建；未传 `canvas_id` 时从 `title` 派生并做重复检查（`crates/agentdash-application/src/canvas/tools.rs:265`, `crates/agentdash-application/src/canvas/tools.rs:288`, `crates/agentdash-application/src/canvas/tools.rs:314`, `crates/agentdash-application/src/canvas/tools.rs:322`）。
- create/attach 成功后，旧工具统一调用 `expose_canvas_to_session(...)`，再返回 `canvas_id`、`mount_id=cvs-*`、`entry_file`、`skill_path=cvs-*://skills/canvas-system/SKILL.md`（`crates/agentdash-application/src/canvas/tools.rs:341`, `crates/agentdash-application/src/canvas/tools.rs:349`, `crates/agentdash-application/src/canvas/tools.rs:356`）。这段应抽成 `create_or_attach_canvas_for_session`，供 `workspace_module_create(kind="canvas")` 复用并返回 `WorkspaceModuleDescriptor`。
- `BindCanvasDataTool` 当前用 `load_canvas_by_ref` 解析 mount id，构造 `CanvasDataBinding`，调用 `upsert_canvas_binding` 后 `canvas_repo.update`（`crates/agentdash-application/src/canvas/tools.rs:404`, `crates/agentdash-application/src/canvas/tools.rs:406`, `crates/agentdash-application/src/canvas/tools.rs:413`, `crates/agentdash-application/src/canvas/tools.rs:420`, `crates/agentdash-application/src/canvas/tools.rs:422`）。这段应抽成 `bind_canvas_data(project_id, mount_id, input)`，由 `workspace_module_invoke` 的 host Canvas branch 调用。
- `PresentCanvasTool` 先 `load_canvas_by_ref`，再调用同一个 `expose_canvas_to_session(...)`，然后发 `canvas_presented` 事件（`crates/agentdash-application/src/canvas/tools.rs:470`, `crates/agentdash-application/src/canvas/tools.rs:477`, `crates/agentdash-application/src/canvas/tools.rs:485`）。`workspace_module_present` 的 Canvas renderer 分支必须复用这段 exposure，再发 `workspace_module_presented`。
- `expose_canvas_to_session` 是最关键复用函数：先 `vfs.append_canvas_mount(canvas)`，再把 `canvas.mount_id` 写入 frame visible canvas mounts，最后调用 live VFS capability state 同步（`crates/agentdash-application/src/canvas/tools.rs:550`, `crates/agentdash-application/src/canvas/tools.rs:556`, `crates/agentdash-application/src/canvas/tools.rs:561`, `crates/agentdash-application/src/canvas/tools.rs:573`）。
- capability state 同步最终调用 `apply_live_vfs_capability_state(..., "canvas", "canvas_visible")`，这会让 runtime VFS 和 skill discovery 先刷新，再展示 Canvas（`crates/agentdash-application/src/canvas/tools.rs:627`, `crates/agentdash-application/src/canvas/tools.rs:634`, `crates/agentdash-application/src/canvas/tools.rs:635`, `crates/agentdash-application/src/canvas/tools.rs:642`）。
- 现有测试已覆盖 create 后能立刻通过 shared VFS 写 `cvs-{canvas_id}://...`，并能发现 `canvas-system` skill（`crates/agentdash-application/src/canvas/tools.rs:963`, `crates/agentdash-application/src/canvas/tools.rs:1009`, `crates/agentdash-application/src/canvas/tools.rs:1015`, `crates/agentdash-application/src/canvas/tools.rs:1028`）。这些测试可迁移或复制到 `workspace_module_create`。
- 现有测试还覆盖 `present_canvas` 更新 frame visible canvas、active VFS、skill baseline 的顺序事实（`crates/agentdash-application/src/canvas/tools.rs:1119`, `crates/agentdash-application/src/canvas/tools.rs:1220`, `crates/agentdash-application/src/canvas/tools.rs:1230`, `crates/agentdash-application/src/canvas/tools.rs:1236`）。实现 `workspace_module_present` Canvas 分支时应保留等价断言，并把事件 key 改成 `workspace_module_presented`。

#### 2. workspace_module contracts / tools / provider / capability 必改文件

- `crates/agentdash-contracts/src/workspace_module.rs` 当前只有 descriptor、ui entry、operation dispatch，没有 create tool contract；需要增加 `WorkspaceModuleCreateKind`、create params/result，或等价 contract 类型，并进入 TS generation（`crates/agentdash-contracts/src/workspace_module.rs:56`, `crates/agentdash-contracts/src/workspace_module.rs:76`, `crates/agentdash-contracts/src/workspace_module.rs:87`, `crates/agentdash-contracts/src/workspace_module.rs:126`）。
- `WorkspaceModuleUiEntry` 当前字段叫 `uri_scheme`，语义是 URI scheme；Canvas 现投影为 `cvs-*`，但目标展示 URI 是 `canvas://{mount_id}`，因此 contract/DTO 应新增或改用 `presentation_uri`，并保留 VFS URI 作为 `vfs_mount_uri` 或 runtime backing 诊断字段（`crates/agentdash-contracts/src/workspace_module.rs:78`, `crates/agentdash-contracts/src/workspace_module.rs:83`）。
- `WorkspaceModuleOperationDispatch::Canvas { canvas_action }` 当前注释和实现都偏 runtime action；目标需要 host-owned Canvas operation，例如 `HostCanvas { action }`，用于 `canvas.bind_data` 这类 application operation（`crates/agentdash-contracts/src/workspace_module.rs:95`, `crates/agentdash-contracts/src/workspace_module.rs:103`）。
- `build_canvas_module` 当前明确把 Canvas binding 排除为 operation，`operations` 为空，Canvas UI entry 的 `view_key` 是 `entry_file`，`uri_scheme` 是 `cvs-*`（`crates/agentdash-application/src/workspace_module/mod.rs:291`, `crates/agentdash-application/src/workspace_module/mod.rs:298`, `crates/agentdash-application/src/workspace_module/mod.rs:301`, `crates/agentdash-application/src/workspace_module/mod.rs:304`）。需要改为 instance-first：`module_id=canvas:{mount_id}`，operation 至少有 `canvas.bind_data`，UI entry 用稳定 `view_key`（如 `preview`）和 `presentation_uri=canvas://{mount_id}`。
- list/describe/invoke/present 都通过 `resolve_visible_modules` 现取现算，并按 `WorkspaceModuleDimension::allows(module_id)` 过滤；新增 create 后，如果当前 session 是 allowlist，需要在 runtime state 中动态授予新 `canvas:{mount_id}`，否则 create 后 describe/invoke 会被裁掉（`crates/agentdash-application/src/workspace_module/tools.rs:39`, `crates/agentdash-application/src/workspace_module/tools.rs:57`, `crates/agentdash-application/src/workspace_module/tools.rs:59`）。
- `workspace_module_invoke` 当前按 `dispatch` 分支走 runtime action、protocol channel、Canvas runtime action、builtin；Canvas branch 仍走 `RuntimeActor::UserCanvas`，不适合绑定数据这种 host-owned application operation（`crates/agentdash-application/src/workspace_module/tools.rs:504`, `crates/agentdash-application/src/workspace_module/tools.rs:594`, `crates/agentdash-application/src/workspace_module/tools.rs:609`）。要新增 host Canvas 分支并调用抽出的 canvas bind use case。
- `workspace_module_present` 当前只从 UI entry 构造 `{module_id, view_key, renderer_kind, uri, title, payload}` 并注入 `workspace_module_presented`，没有执行 Canvas exposure（`crates/agentdash-application/src/workspace_module/tools.rs:775`, `crates/agentdash-application/src/workspace_module/tools.rs:784`, `crates/agentdash-application/src/workspace_module/tools.rs:793`）。Canvas renderer 分支需要先执行 exposure，再发 payload；非 Canvas renderer 保持轻量。
- `crates/agentdash-application/src/vfs/tools/provider.rs` 当前同时注入 Canvas cluster 和 WorkspaceModule cluster；hard cut 后应停止正常注入 `canvases_list` / `canvas_start` / `bind_canvas_data` / `present_canvas`，并在 WorkspaceModule cluster 中新增 `workspace_module_create`（`crates/agentdash-application/src/vfs/tools/provider.rs:309`, `crates/agentdash-application/src/vfs/tools/provider.rs:327`, `crates/agentdash-application/src/vfs/tools/provider.rs:344`, `crates/agentdash-application/src/vfs/tools/provider.rs:361`, `crates/agentdash-application/src/vfs/tools/provider.rs:376`, `crates/agentdash-application/src/vfs/tools/provider.rs:456`）。
- `crates/agentdash-spi/src/platform/tool_capability.rs` 当前 `CAP_CANVAS` 仍是 well-known key，`CLUSTER_CANVAS_TOOLS` 仍列四个旧工具，`CLUSTER_WORKSPACE_MODULE_TOOLS` 缺 `workspace_module_create`（`crates/agentdash-spi/src/platform/tool_capability.rs:81`, `crates/agentdash-spi/src/platform/tool_capability.rs:98`, `crates/agentdash-spi/src/platform/tool_capability.rs:124`, `crates/agentdash-spi/src/platform/tool_capability.rs:130`）。
- 同文件 `platform_tool_descriptors()` 仍把四个 Canvas tools 注册在 `CAP_CANVAS`，WorkspaceModule descriptors 仅四个工具；hard cut 后应删除/隐藏普通 Canvas descriptors，并加入 `workspace_module_create`（`crates/agentdash-spi/src/platform/tool_capability.rs:340`, `crates/agentdash-spi/src/platform/tool_capability.rs:348`, `crates/agentdash-spi/src/platform/tool_capability.rs:355`, `crates/agentdash-spi/src/platform/tool_capability.rs:362`, `crates/agentdash-spi/src/platform/tool_capability.rs:369`）。
- `capability_to_tool_clusters_by_key` 当前 `CAP_CANVAS => ToolCluster::Canvas`，`CAP_WORKSPACE_MODULE => ToolCluster::WorkspaceModule`；若删除 Canvas 普通能力，需要同步移除或仅保留迁移读取逻辑（`crates/agentdash-spi/src/platform/tool_capability.rs:598`, `crates/agentdash-spi/src/platform/tool_capability.rs:603`）。
- `default_visibility_rules` 当前 `CAP_CANVAS` 仍 Project auto grant，`CAP_WORKSPACE_MODULE` 是 Project/Story/Task auto grant；收束后 Canvas 资产管理应只通过 `workspace_module` 表达（`crates/agentdash-spi/src/platform/tool_capability.rs:770`, `crates/agentdash-spi/src/platform/tool_capability.rs:778`）。
- `ToolCluster` enum 仍有 `Canvas`，注释列旧四工具；`CapabilityState::all()` 仍启用 `ToolCluster::Canvas`，但没有启用 `WorkspaceModule`（`crates/agentdash-spi/src/connector/mod.rs:196`, `crates/agentdash-spi/src/connector/mod.rs:207`, `crates/agentdash-spi/src/connector/mod.rs:337`, `crates/agentdash-spi/src/connector/mod.rs:346`）。收束时要检查 `all()`、tests、tool schema delta 的 cluster key。
- `session/plan.rs` 的 `conditional_flow_tools` 对 Project/Story/Task/None 都追加旧 Canvas tools；应改成 workspace module 工具集，至少包含 create/list/describe/invoke/present（`crates/agentdash-application/src/session/plan.rs:281`, `crates/agentdash-application/src/session/plan.rs:287`, `crates/agentdash-application/src/session/plan.rs:304`, `crates/agentdash-application/src/session/plan.rs:313`）。
- 前端 capability picker 仍把 `CapabilityKey` 限定到 `"canvas"`，`CAPABILITY_OPTIONS` 仍展示 Canvas，workflow well-known key list 仍包含 `canvas`，应改为 `workspace_module`（`packages/app-web/src/types/index.ts:104`, `packages/app-web/src/types/index.ts:132`, `packages/app-web/src/types/workflow.ts:187`, `packages/app-web/src/types/workflow.ts:191`）。
- ProjectAgent preset 前端 roundtrip 从 `capability_directives` 中抽 `{ add: CapabilityKey }`，保存时重新写 `{ add: key }`，因此前端类型改为 `workspace_module` 后能自然写回新 key（`packages/app-web/src/features/project/agent-preset-editor/form-state.ts:44`, `packages/app-web/src/features/project/agent-preset-editor/form-state.ts:87`）。
- forward migration 目标是 `project_agents.config`，DB 中为 `text DEFAULT '{}'`，Rust repository 读写时序列化/反序列化 JSON；新增 migration 文件应改写 text JSON 中 `capability_directives` 的 `"canvas"` 为 `"workspace_module"`（`crates/agentdash-infrastructure/migrations/0001_init.sql:388`, `crates/agentdash-infrastructure/migrations/0001_init.sql:393`, `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs:54`, `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs:77`, `crates/agentdash-infrastructure/src/persistence/postgres/agent_repository.rs:160`）。

#### 3. 前端 workspace_module_presented payload 与 canvas:// URI 触点

- 后端 `workspace_module_present` 当前 payload 字段是 `"uri": ui_entry.uri_scheme`，不是 `presentation_uri`；Canvas 当前又把 `ui_entry.uri_scheme` 投影成 `cvs-*`，导致前端不得不猜 `canvas://{view_key}`（`crates/agentdash-application/src/workspace_module/tools.rs:775`, `crates/agentdash-application/src/workspace_module/tools.rs:779`, `crates/agentdash-application/src/workspace_module/mod.rs:301`, `crates/agentdash-application/src/workspace_module/mod.rs:304`）。
- 前端 `SessionPage` 旧 `canvas_presented` 分支读取 `canvas_id`，刷新 runtime state，再打开 `canvas://${nextCanvasId}`；hard cut 后这条路径不应是正常事实源（`packages/app-web/src/pages/SessionPage.tsx:585`, `packages/app-web/src/pages/SessionPage.tsx:592`, `packages/app-web/src/pages/SessionPage.tsx:594`）。
- `workspace_module_presented` 分支当前读取 `renderer_kind`、`view_key`、`uri`；Canvas 时使用 `uri || canvas://{viewKey}`，这会把 `view_key=src/main.tsx` 错当成 Canvas id（`packages/app-web/src/pages/SessionPage.tsx:598`, `packages/app-web/src/pages/SessionPage.tsx:603`, `packages/app-web/src/pages/SessionPage.tsx:605`, `packages/app-web/src/pages/SessionPage.tsx:607`）。目标应改为读取 `presentation_uri`，Canvas 必须是 `canvas://{mount_id}`。
- `WorkspacePanel` 仍监听 `activeCanvasId` 变化自动打开 Canvas tab，`activeCanvasId` 来自旧事件；workspace-module-driven 打开应直接通过 tab uri，不依赖这个旁路事实源（`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:28`, `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:75`, `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:79`）。
- Canvas tab 本身已经以 `canvas://` 为展示 URI 协议，并能 parse/build `canvas://{canvasId}`；这里可复用，但传入 id 应统一是 mount id，不是 DB id、entry file 或 `cvs-*` mount id（`packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:9`, `packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:11`, `packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:51`, `packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:75`）。
- VFS 编辑 URI 与展示 URI 要分开：`build_canvas_mount_id` 当前生成 `cvs-{mount_id}`，Canvas mount `root_ref` 是内部 `canvas://{canvas.id}`，VFS path validator 要求 `canvas_fs` root_ref 使用 `canvas://`（`crates/agentdash-application/src/vfs/mount.rs:980`, `crates/agentdash-application/src/vfs/mount.rs:984`, `crates/agentdash-application/src/vfs/mount.rs:989`, `crates/agentdash-application/src/vfs/path.rs:406`, `crates/agentdash-application/src/vfs/path.rs:439`）。Agent-facing 编辑 URI 应继续是 `cvs-{mount_id}://...`；frontend presentation URI 应是 `canvas://{mount_id}`。

#### 4. embedded skill bundle 新增 workspace-module-system 与更新 canvas-system 入口

- `canvas-system` 的 bundle 声明在 `crates/agentdash-domain/src/canvas/value_objects.rs`，通过 `CANVAS_SYSTEM_BUNDLE_FILES`、`CANVAS_SYSTEM_BUNDLE` 和 `ensure_canvas_system_skill` 物化到 Canvas 文件内（`crates/agentdash-domain/src/canvas/value_objects.rs:10`, `crates/agentdash-domain/src/canvas/value_objects.rs:17`, `crates/agentdash-domain/src/canvas/value_objects.rs:30`, `crates/agentdash-domain/src/canvas/value_objects.rs:118`）。
- `canvas-system` skill 内容当前明确要求 `canvases_list -> canvas_start -> bind_canvas_data -> present_canvas`，必须更新为 workspace module 流程：`workspace_module_create(kind="canvas")` 或 `list/describe` 找到 `canvas:{mount_id}`，通过 `workspace_module_invoke` 绑定，通过 `workspace_module_present` 展示，再编辑 `cvs-*://` 文件（`crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md:10`, `crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md:12`, `crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md:15`, `crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md:16`）。
- 新 `workspace-module-system` 最适合做成项目级 builtin SkillAsset，而不是 Canvas mount 内文件；模板入口是 `crates/agentdash-application/src/skill_asset/definition.rs` 的 `BUILTIN_SKILL_TEMPLATES`，当前注册了 `canvas-system`、`companion-system`、`routine-memory`（`crates/agentdash-application/src/skill_asset/definition.rs:1`, `crates/agentdash-application/src/skill_asset/definition.rs:13`, `crates/agentdash-application/src/skill_asset/definition.rs:15`, `crates/agentdash-application/src/skill_asset/definition.rs:20`, `crates/agentdash-application/src/skill_asset/definition.rs:25`）。
- bundle 定义模式可照 `companion-system`：domain 侧创建 `value_objects.rs` 常量、`include_str!` 指向 `skills/<name>/SKILL.md` 和 references，声明 `EmbeddedSkillBundle` 并加 validation test（`crates/agentdash-domain/src/companion/value_objects.rs:3`, `crates/agentdash-domain/src/companion/value_objects.rs:18`, `crates/agentdash-domain/src/companion/value_objects.rs:51`, `crates/agentdash-domain/src/companion/value_objects.rs:63`）。
- project-level bootstrap/projection 可照 `companion/skill_projection.rs`：`ensure_companion_system_skill_asset` 调 `SkillAssetService::bootstrap_builtins(project_id, Some(COMPANION_SYSTEM_SKILL_NAME))`，`append_companion_system_skill_key` 把 key 加入 lifecycle skill projection 列表（`crates/agentdash-application/src/companion/skill_projection.rs:11`, `crates/agentdash-application/src/companion/skill_projection.rs:15`, `crates/agentdash-application/src/companion/skill_projection.rs:28`, `crates/agentdash-application/src/companion/skill_projection.rs:38`）。
- session assembler 当前只在 lifecycle mount 存在时确保并投影 `companion-system`，然后追加 visible Canvas mounts，最后 `append_lifecycle_skill_asset_projection`；`workspace-module-system` 应在 session 具备 `workspace_module` capability 时加入 `skill_asset_keys`，并经同一 lifecycle projection 暴露（`crates/agentdash-application/src/session/assembler.rs:599`, `crates/agentdash-application/src/session/assembler.rs:603`, `crates/agentdash-application/src/session/assembler.rs:606`, `crates/agentdash-application/src/session/assembler.rs:608`, `crates/agentdash-application/src/session/assembler.rs:616`）。
- `SkillAssetService::bootstrap_builtins(project_id, Some(key))` 已支持按 builtin key 同步单个内嵌 skill；缺 key 会返回 NotFound，新增模板后可直接复用（`crates/agentdash-application/src/skill_asset/service.rs:206`, `crates/agentdash-application/src/skill_asset/service.rs:211`, `crates/agentdash-application/src/skill_asset/service.rs:228`）。

### Related Specs

- `.trellis/spec/backend/embedded-skill-bundles.md` - 规定 embedded bundle 模型、`ensure_embedded_skill_bundle`、项目级 SkillAsset bootstrap/lifecycle projection 路径。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` - 规定 well-known capability、ToolCluster、tool descriptor、resolver、tool_policy 消费链路；当前 spec 仍列 `canvas`，实现后需要更新为 `workspace_module`。
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md` - 规定 Workspace module visibility 是 ProjectAgent preset `visible_workspace_module_refs` 到 `CapabilityState.workspace_module` 的 Replace projection。
- `.trellis/spec/backend/vfs/vfs-access.md` - 记录 Canvas visible mount、`cvs-*` mount id、`visible_canvas_mount_ids` 与 capability state 刷新顺序；实现后要补充 workspace_module_present 复用该语义的原因。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - 规定 Rust contract -> generated TS 的单一契约链路；新增/变更 workspace module DTO 后需要跑 contracts check。
- `.trellis/spec/frontend/type-safety.md` - 规定 `canvas_presented`、`capability_state_changed` 等事件只触发 runtime state refresh，不应创建长期旁路事实源；本任务应让 `workspace_module_presented.presentation_uri` 成为打开 tab 的输入。

### External References

- 未使用外部文档；本研究基于本地 Trellis 任务文档、spec 与源码。
- 当前日期：2026-06-10；未验证依赖包版本或官方 API 文档。

## Caveats / Not Found

- `task.py current --source` 返回 `Current task: (none)`；本研究按用户明确给出的 `.trellis/tasks/06-10-canvas-workspace-module-convergence` 作为任务目录写入，未修改 active task 状态。
- 未找到现成 `workspace_module_create` 类型、工具或 provider 注入；需要新建 contract、tool implementation、SPI descriptor 和 provider 分支。
- 未找到现成 `workspace-module-system` domain bundle；需要新增 domain 模块或选择合适归属，并注册到 builtin SkillAsset templates。
- 现有 `WorkspaceModuleOperationDispatch::Canvas` 表达的是 Canvas runtime action，不是 host-owned Canvas application operation；直接复用会把 `bind_data` 错建模。
- 当前 Project Settings HTTP route 暴露 project-level all modules，不按 session `WorkspaceModuleDimension` 裁切；Agent tools 已按 session visibility 裁切。实现 create 后的动态 visibility grant 需要落在 session/runtime capability 层，不应写回 ProjectAgent preset。
- 没有执行测试、migration guard 或 contract generation；本文件仅为实现触点研究。
