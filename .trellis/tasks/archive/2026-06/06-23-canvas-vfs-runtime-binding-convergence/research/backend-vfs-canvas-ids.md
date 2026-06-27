# Research: backend-vfs-canvas-ids

- Query: Canvas 后端/VFS 中 ID、mount id、URI、root_ref、presentation_uri 的命名和路径语义散落在哪里；是否适合统一为强制 `cvs-` 前缀和 mount id 同名规则。
- Scope: mixed, internal code + internal specs
- Date: 2026-06-23

## Findings

### Current Facts

Canvas 当前至少有五种相邻但不同的身份字符串：

| 名称 | 当前形态 | 主要语义 |
| --- | --- | --- |
| `Canvas.id` | UUID | 持久化实体主键，`canvas_fs` provider 通过 mount metadata 的 `canvas_id` 回查实体。 |
| `Canvas.mount_id` | 未加 `cvs-` 的字符串，如 `dashboard-a` | Project 内 Canvas 逻辑标识、workspace module id 尾部、presentation URI 尾部、visible canvas 记录值。 |
| Runtime VFS mount id | `cvs-{canvas.mount_id}` | Agent 编辑 URI 的 mount 前缀，如 `cvs-dashboard-a://src/main.tsx`。 |
| `root_ref` | `canvas://{canvas.id}` | VFS mount/provider 内部 root identity；不是用户打开的 Canvas URI。 |
| `presentation_uri` | `canvas://{canvas.mount_id}` | WorkspacePanel 打开 Canvas tab 的 UI URI；不是 VFS 编辑 URI。 |

关键规格已写明当前分工：runtime mount id 是 `cvs-<canvas.mount_id>`，`visible_canvas_mount_ids` 存未加前缀的 `canvas.mount_id`，`presentation_uri` 是 `canvas://{mount_id}`，Agent 编辑用 `cvs-<mount_id>://...`；见 `.trellis/spec/backend/vfs/vfs-access.md:65`、`.trellis/spec/backend/vfs/vfs-access.md:66`、`.trellis/spec/backend/vfs/vfs-access.md:70`、`.trellis/spec/backend/vfs/vfs-access.md:71`。

Domain 层 `Canvas` 实体同时持有 `id` 与 `mount_id`，没有进一步类型区分；见 `crates/agentdash-domain/src/canvas/entity.rs:9`、`crates/agentdash-domain/src/canvas/entity.rs:24`。`CanvasRepository` 同时提供 Project-scoped `get_by_mount_id(project_id, mount_id)` 与全局 `find_by_mount_id(mount_id)`；见 `crates/agentdash-domain/src/canvas/repository.rs:7`、`crates/agentdash-domain/src/canvas/repository.rs:10`、`crates/agentdash-domain/src/canvas/repository.rs:16`。

Application Canvas 管理层把 `mount_id` 当作用户/Agent 提供的 Canvas 逻辑 id。未提供时由 title 派生；只禁止空白、`main`、空白字符、`/`、`\`、`:`，并未强制 `cvs-` 前缀；见 `crates/agentdash-application/src/canvas/management.rs:108`、`crates/agentdash-application/src/canvas/management.rs:116`、`crates/agentdash-application/src/canvas/management.rs:117`、`crates/agentdash-application/src/canvas/management.rs:288`、`crates/agentdash-application/src/canvas/management.rs:327`。

VFS Canvas mount 构造层集中把 logical `canvas.mount_id` 转成 runtime mount id：`build_canvas_mount_id(canvas) = format!("cvs-{}", canvas.mount_id)`；同时 `root_ref` 使用 `canvas://{canvas.id}`，metadata 同时写 `canvas_id`、未加前缀的 `mount_id`、`project_id`、`entry_file`；见 `crates/agentdash-application/src/vfs/mount_canvas.rs:8`、`crates/agentdash-application/src/vfs/mount_canvas.rs:12`、`crates/agentdash-application/src/vfs/mount_canvas.rs:17`、`crates/agentdash-application/src/vfs/mount_canvas.rs:30`。`append_canvas_mounts` 用 mount id 去重替换，binding file refresh 也重新派生 `cvs-...`；见 `crates/agentdash-application/src/vfs/mount_canvas.rs:39`、`crates/agentdash-application/src/vfs/mount_canvas.rs:47`。

VFS path 解析层已经把 mount id 当 URI scheme 使用，`MountId` 只禁止 `://`、斜杠、反斜杠与空白；见 `crates/agentdash-application/src/vfs/path.rs:13`、`crates/agentdash-application/src/vfs/path.rs:82`、`crates/agentdash-application/src/vfs/path.rs:193`、`crates/agentdash-application/src/vfs/path.rs:238`。VFS hard validation 已有 reserved rule：任何以 `cvs-` 开头的 mount id 必须由 `canvas_fs` provider 使用；`canvas_fs` 的 `root_ref` scheme 必须是 `canvas://`；见 `crates/agentdash-application/src/vfs/path.rs:400`、`crates/agentdash-application/src/vfs/path.rs:406`、`crates/agentdash-application/src/vfs/path.rs:419`、`crates/agentdash-application/src/vfs/path.rs:439`。

`CanvasFsMountProvider` 不依赖 mount id 查仓库，而是从 mount metadata 取 `canvas_id`，再 `get_by_id`；这使得 runtime mount id 可以重命名，只要 metadata/root identity 稳定即可；见 `crates/agentdash-application/src/vfs/provider_canvas.rs:20`、`crates/agentdash-application/src/vfs/provider_canvas.rs:29`、`crates/agentdash-application/src/vfs/provider_canvas.rs:254`。读写/list/search 均基于 mount-relative path，binding 生成文件通过 metadata 中的 `binding_files` 覆盖未解析占位内容，并拒绝直接写入生成文件；见 `crates/agentdash-application/src/vfs/provider_canvas.rs:66`、`crates/agentdash-application/src/vfs/provider_canvas.rs:101`、`crates/agentdash-application/src/vfs/provider_canvas.rs:278`、`crates/agentdash-application/src/vfs/provider_canvas.rs:296`。

Agent-facing VFS tool path 仍是 `mount_id://path`，`resolve_uri_path` 对有 `://` 的输入直接走 `parse_mount_uri`，没有 Canvas 特例；见 `crates/agentdash-application/src/vfs/tools/common.rs:15`。`SharedRuntimeVfs::append_canvas_mount` 也直接使用 `build_canvas_mount`；见 `crates/agentdash-application/src/vfs/tools/common.rs:64`。

Canvas tool 层的命名有混用：`StartCanvasParams.canvas_id` 文档说是 stable canvas identifier，但实现用它查/建 `Canvas.mount_id`；`CanvasToolResult.canvas_id` 返回的也是 `canvas.mount_id`，`mount_id` 返回的是 `build_canvas_mount_id` 之后的 `cvs-...`；见 `crates/agentdash-application/src/canvas/tools.rs:17`、`crates/agentdash-application/src/canvas/tools.rs:34`、`crates/agentdash-application/src/canvas/tools.rs:55`、`crates/agentdash-application/src/canvas/tools.rs:72`、`crates/agentdash-application/src/canvas/tools.rs:146`、`crates/agentdash-application/src/canvas/tools.rs:148`、`crates/agentdash-application/src/canvas/tools.rs:149`。

Canvas data binding 的 `source_uri` 通过 current session VFS 解析，因此它可以引用任意当前可见 mount URI；见 `crates/agentdash-application/src/canvas/runtime.rs:35`、`crates/agentdash-application/src/canvas/runtime.rs:177`、`crates/agentdash-application/src/canvas/runtime.rs:184`。runtime snapshot 在有 session VFS 时写 `resource_surface_ref = session-runtime:{session_id}`；见 `crates/agentdash-application/src/canvas/runtime.rs:134`、`crates/agentdash-application/src/canvas/runtime.rs:145`。

Workspace module 聚合层把 Canvas 投影为 module id `canvas:{canvas.mount_id}`，UI entry 的 `presentation_uri` 是 `canvas://{canvas.mount_id}`，`runtime_backing` 是 `canvas:{build_canvas_mount_id(canvas)}`；见 `crates/agentdash-application/src/workspace_module/mod.rs:170`、`crates/agentdash-application/src/workspace_module/mod.rs:386`、`crates/agentdash-application/src/workspace_module/mod.rs:429`、`crates/agentdash-application/src/workspace_module/mod.rs:440`、`crates/agentdash-application/src/workspace_module/mod.rs:455`。`build_workspace_module_presentation` 对 UI entry 的 `presentation_uri` 做 canonical 输出；见 `crates/agentdash-application/src/workspace_module/mod.rs:233`、`crates/agentdash-application/src/workspace_module/mod.rs:255`、`crates/agentdash-application/src/workspace_module/mod.rs:273`。

Workspace module visibility 的动态可见性读取 `AgentRunEffectiveCapabilityView.visible_workspace_module_refs`，允许 `canvas:{mount_id}` 动态扩展 allowlist；缺失 ref 只产出 diagnostic；见 `crates/agentdash-application/src/workspace_module/visibility.rs:28`、`crates/agentdash-application/src/workspace_module/visibility.rs:48`、`crates/agentdash-application/src/workspace_module/visibility.rs:55`、`crates/agentdash-application/src/workspace_module/visibility.rs:67`。

`workspace_module_create` 调 `create_or_attach_canvas_for_session`，返回 descriptor 前暴露 Canvas VFS；`workspace_module_present` 调 `build_workspace_module_presentation` 后，如果 renderer 是 canvas，会 `expose_existing_canvas_for_session`，随后写 `workspace_module_presented` 事件；见 `crates/agentdash-application/src/workspace_module/tools.rs:398`、`crates/agentdash-application/src/workspace_module/tools.rs:436`、`crates/agentdash-application/src/workspace_module/tools.rs:923`、`crates/agentdash-application/src/workspace_module/tools.rs:1019`、`crates/agentdash-application/src/workspace_module/tools.rs:1033`、`crates/agentdash-application/src/workspace_module/tools.rs:1053`。

Session capability exposure 是当前最权威的 live binding 路径：`expose_canvas_mount_revision_and_adopt(session_id, canvas)` 反查 runtime session target，读取 current AgentFrame，向 active VFS 追加 canvas mount，刷新 binding files，写新的 AgentFrame revision，追加未加前缀的 visible canvas mount id 与 `canvas:{mount_id}` workspace module ref，然后 adopt 到 runtime；见 `crates/agentdash-application/src/session/capability_service.rs:103`、`crates/agentdash-application/src/session/capability_service.rs:126`、`crates/agentdash-application/src/session/capability_service.rs:130`、`crates/agentdash-application/src/session/capability_service.rs:143`、`crates/agentdash-application/src/session/capability_service.rs:144`。

AgentFrame 持久化列也体现双轨：`visible_canvas_mount_ids_json` 存未加前缀的 Canvas mount ids，`visible_workspace_module_refs_json` 存 `canvas:{mount_id}` 等 module refs；见 `crates/agentdash-domain/src/workflow/agent_frame.rs:26`、`crates/agentdash-domain/src/workflow/agent_frame.rs:33`、`crates/agentdash-domain/src/workflow/agent_frame.rs:77`、`crates/agentdash-domain/src/workflow/agent_frame.rs:86`、`crates/agentdash-domain/src/workflow/agent_frame.rs:104`、`crates/agentdash-domain/src/workflow/agent_frame.rs:113`。AgentFrame builder 会 carry forward 两列；见 `crates/agentdash-application/src/agent_run/frame/builder.rs:270`、`crates/agentdash-application/src/agent_run/frame/builder.rs:273`。

Frame construction 的 owner bootstrap 会从 spec 中接收 `visible_canvas_mount_ids` 并追加到 VFS，也会把 project agent config 的 `visible_workspace_module_refs` 投影进 `CapabilityState.workspace_module`；见 `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:135`、`crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:139`、`crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:261`、`crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:468`。SessionAssemblyBuilder 也有 `append_canvas_mounts(canvas_repo, project_id, mount_ids)`，输入同样是未加 `cvs-` 的 ids；见 `crates/agentdash-application/src/session/assembly_builder.rs:166`。

Contracts/embedded skills 也暴露同样语义：WorkspaceModule contract 说 module id 是 `canvas:{mount_id}`，presentation URI 示例为 `canvas://dashboard`，且 `uri_scheme` 不是 Canvas VFS 编辑 mount；见 `crates/agentdash-contracts/src/surface/workspace_module.rs:59`、`crates/agentdash-contracts/src/surface/workspace_module.rs:82`、`crates/agentdash-contracts/src/surface/workspace_module.rs:86`。Canvas system skill 也告知 Agent 用 `cvs-<mount_id>://...` 编辑、`canvas://{mount_id}` 展示；见 `crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md:14`、`crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md:21`、`crates/agentdash-domain/src/canvas/skills/canvas-system/SKILL.md:24`。

Persistence 层没有为 `canvases(project_id, mount_id)` 建唯一约束；`0001_init.sql` 只给 `canvases` 添加主键，而 `project_vfs_mounts` 有 `(project_id, mount_id)` 唯一约束；见 `crates/agentdash-infrastructure/migrations/0001_init.sql:170`、`crates/agentdash-infrastructure/migrations/0001_init.sql:173`、`crates/agentdash-infrastructure/migrations/0001_init.sql:855`、`crates/agentdash-infrastructure/migrations/0001_init.sql:943`。Postgres repository 的 `find_by_mount_id` 是全局 `WHERE mount_id = $1 LIMIT 1`，有跨 Project 同名歧义；见 `crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs:220`、`crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs:248`、`crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs:253`。

### Files Found

- `.trellis/spec/backend/vfs/vfs-access.md`: 当前 Canvas session visibility 规范，明确 `cvs-<canvas.mount_id>` / `canvas:{mount_id}` / `canvas://{mount_id}` 分工。
- `.trellis/spec/backend/vfs/architecture.md`: VFS hard validation 与 runtime tool composition baseline。
- `.trellis/spec/backend/runtime-gateway.md`: Canvas iframe 通过 RuntimeGateway 调 Session Action 的边界。
- `crates/agentdash-domain/src/canvas/entity.rs`: Canvas 聚合实体，保存 UUID id 与 `mount_id`。
- `crates/agentdash-domain/src/canvas/repository.rs`: Canvas repository 查找接口，含 project-scoped 与 global mount_id lookup。
- `crates/agentdash-application/src/canvas/management.rs`: Canvas 创建、更新、路径和 mount_id normalize 规则。
- `crates/agentdash-application/src/canvas/tools.rs`: workspace module create/invoke/present 复用的 Canvas tool use cases，返回 `canvas_id`/`mount_id`。
- `crates/agentdash-application/src/canvas/runtime.rs`: Canvas runtime snapshot、binding `source_uri` 解析、session runtime surface ref。
- `crates/agentdash-application/src/canvas/visibility.rs`: 根据 AgentFrame 中记录的 visible canvas ids 向 VFS 追加 Canvas mounts。
- `crates/agentdash-application/src/vfs/mount_canvas.rs`: Canvas runtime mount id、root_ref、metadata 构造集中点。
- `crates/agentdash-application/src/vfs/provider_canvas.rs`: `canvas_fs` provider 通过 metadata `canvas_id` 回查实体并实现 read/write/list/search。
- `crates/agentdash-application/src/vfs/path.rs`: VFS URI、root_ref、reserved mount id 与 provider/root_ref validation。
- `crates/agentdash-application/src/vfs/tools/common.rs`: Agent-facing VFS path resolution 与 runtime VFS handle。
- `crates/agentdash-application/src/workspace_module/mod.rs`: workspace module 聚合、Canvas descriptor、presentation 构造。
- `crates/agentdash-application/src/workspace_module/visibility.rs`: `canvas:{mount_id}` 动态可见性过滤。
- `crates/agentdash-application/src/workspace_module/tools.rs`: `workspace_module_create/invoke/present` 工具路径。
- `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs`: workspace module tools 装配入口。
- `crates/agentdash-application/src/session/capability_service.rs`: Canvas exposure 写 AgentFrame revision 并 adopt 到 runtime 的核心路径。
- `crates/agentdash-domain/src/workflow/agent_frame.rs`: visible canvas ids 与 visible workspace module refs 持久化字段和 append helper。
- `crates/agentdash-application/src/agent_run/effective_capability.rs`: AgentRun effective capability view 输出 `visible_workspace_module_refs`。
- `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs`: owner bootstrap 将 visible canvas ids 加回 VFS 并投影 workspace module allowlist。
- `crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs`: Canvas PostgreSQL 持久化与 mount_id lookup。
- `crates/agentdash-infrastructure/migrations/0001_init.sql`: `canvases` schema、缺少 `(project_id, mount_id)` unique 的事实源。

### Code Patterns

- Canvas logical id 与 VFS authoring mount id 由 helper 派生：`Canvas.mount_id = "dashboard-a"` -> `build_canvas_mount_id = "cvs-dashboard-a"`；见 `crates/agentdash-application/src/vfs/mount_canvas.rs:8`。
- Canvas VFS mount 内部定位实体使用 `root_ref = canvas://{uuid}` + metadata `canvas_id`；provider 实际读取 metadata `canvas_id` 而不是解析 root_ref；见 `crates/agentdash-application/src/vfs/mount_canvas.rs:17`、`crates/agentdash-application/src/vfs/provider_canvas.rs:254`。
- Workspace module 使用 `canvas:{canvas.mount_id}`，presentation 使用 `canvas://{canvas.mount_id}`，runtime backing 使用 `canvas:{cvs-mount-id}`；见 `crates/agentdash-application/src/workspace_module/mod.rs:429`、`crates/agentdash-application/src/workspace_module/mod.rs:440`、`crates/agentdash-application/src/workspace_module/mod.rs:455`。
- Runtime exposure 写入两个不同 ref：`visible_canvas_mount_ids_json` 为未加前缀 mount id，`visible_workspace_module_refs_json` 为 `canvas:{mount_id}`；见 `crates/agentdash-application/src/session/capability_service.rs:143`、`crates/agentdash-application/src/session/capability_service.rs:144`。
- VFS validation 已支持 `cvs-` reserved prefix，但只约束 provider，不强制所有 canvas mount id 一定来自 prefixed persisted id；见 `crates/agentdash-application/src/vfs/path.rs:406`。
- `find_by_mount_id` 全局 lookup 与 Project-scoped lookup 并存，且数据库当前没有唯一约束保证 Project 内同名不重复；见 `crates/agentdash-infrastructure/src/persistence/postgres/canvas_repository.rs:248`、`crates/agentdash-infrastructure/migrations/0001_init.sql:855`。

### Risks

1. `canvas://` scheme 同时承载两种意义：VFS `root_ref = canvas://{uuid}` 与 UI `presentation_uri = canvas://{mount_id}`。这两个值在字符串上无法靠 scheme 区分，后续新增 parser、router、materialization 或 frontend bridge 时容易误把 presentation URI 当 provider root，或反过来。
2. `mount_id` 这个字段名在不同层含义不同：domain/API 上是 Canvas logical id，VFS 上是 `cvs-...` authoring mount id，tool result 同时返回 `canvas_id = logical mount_id` 与 `mount_id = cvs mount id`。Agent-facing 文档虽解释了规则，但代码边界没有类型保护。
3. `visible_canvas_mount_ids_json` 存未加前缀值，`active_vfs.mounts[].id` 存加前缀值，`visible_workspace_module_refs_json` 又存 `canvas:{unprefixed}`。每次恢复/构造/过滤都要知道该从哪种形式派生哪种形式。
4. 数据库未约束 `(project_id, mount_id)` 唯一，应用创建路径有存在性检查但不防并发竞态；`find_by_mount_id` 全局 `LIMIT 1` 与“Canvas 是 Project 级资产”的语义冲突。
5. `Canvas.mount_id` 当前只禁止 `main`，没有禁止与 `lifecycle`、`routine`、`skill-assets`、`cvs-*` 等 VFS reserved namespace 的逻辑冲突。由于 runtime 会自动加 `cvs-`，logical id `cvs-demo` 会形成 `cvs-cvs-demo`。
6. `root_ref` 当前使用 UUID，metadata 也使用 UUID；如果 future provider 解析 root_ref 与 metadata 不一致，可能出现双事实源。现在 provider 只读 metadata，这让 `root_ref` 更像 validation placeholder。
7. Embedded skill/contract/test 中硬编码 `canvas:{mount_id}`、`canvas://{mount_id}`、`cvs-<mount_id>` 很多；任何统一改名必须同步 contracts、generated TS、frontend tab open、skills 和 tests。

### Can It Converge To Mandatory `cvs-` Prefix And Same Mount Id?

建议可以统一，但要明确统一对象是“VFS authoring mount id”，不是所有 Canvas 业务引用都无条件叫 mount id。

推荐目标语义：

- `Canvas.mount_id` 改为真正的 runtime VFS mount id，强制以 `cvs-` 开头，例如 `cvs-dashboard-a`。
- 所有字段名为 `mount_id` 的后端/VFS/Agent-facing 数据都指同一个字符串：`cvs-dashboard-a`。这样 `visible_canvas_mount_ids_json`、`active_vfs.mounts[].id`、tool result `mount_id`、VFS URI scheme 保持同名。
- 如果 UI 或模块仍需要非 VFS 的短名，应新增/改名为 `canvas_key`、`canvas_slug` 或 `display_slug`，不要继续叫 `mount_id`。
- `build_canvas_mount_id` 不再拼接前缀，只做 validate/identity return，或被删除。
- Workspace module ref 可以随同统一为 `canvas:{mount_id}`，即 `canvas:cvs-dashboard-a`。这虽然更长，但具备“module 指向哪个 authoring mount”这个直觉，且无需再知道隐藏派生规则。
- `presentation_uri` 可以继续是 `canvas://{mount_id}`，即 `canvas://cvs-dashboard-a`，但必须与 VFS `root_ref` scheme 拆开。
- `root_ref` 不建议继续使用 `canvas://{uuid}`。推荐改为更内部化且可验证的 provider root scheme，例如 `canvas-root://{canvas_uuid}` 或 `canvas://id/{canvas_uuid}`。若保留 `canvas://`，至少要在 `RootRef` validation/provider parse 中区分 `canvas://id/{uuid}` 与 `canvas://{mount_id}`。

如果产品更希望 `canvas:{dashboard-a}` 和 `canvas://dashboard-a` 短一些，也可以保留短 Canvas key，但那就不应称为 `mount_id`。这种方案的边界是：`Canvas.key = dashboard-a`，`Canvas.vfs_mount_id = cvs-dashboard-a`，所有 VFS-facing 字段只接受 `vfs_mount_id`。它比“所有 mount id 同名”多一个字段，但对 UI 更短。基于用户问题中的“mount id 同名”，本研究更推荐前一方案：`Canvas.mount_id` 本身即 `cvs-...`。

### Recommended Refactor Boundaries

1. Domain and persistence boundary:
   - 将 `Canvas.mount_id` 的 invariant 改为强制 `cvs-` 前缀。
   - 增加数据库 migration：回填现有 `canvases.mount_id = 'cvs-' || mount_id`（已带前缀的跳过或先规范化），并添加 `UNIQUE(project_id, mount_id)`。
   - 移除或替换 `CanvasRepository::find_by_mount_id`，优先所有入口都携带 `project_id`；若保留，只能用于明确全局唯一的新约束场景。

2. VFS boundary:
   - 删除 `build_canvas_mount_id` 的拼接语义；`build_canvas_mount` 使用 `canvas.mount_id` 作为 `Mount.id`。
   - `validate_reserved_mount_id` 保持 `cvs-` 只能属于 `canvas_fs`，同时 Canvas normalize 也强制 `cvs-`。
   - 拆分 provider `root_ref` scheme，避免 `canvas://{uuid}` 与 presentation URI 同 scheme。
   - provider 继续以 metadata `canvas_id` 作为实体回查权威；root_ref 只作为 provider root validation / materialization key。

3. Session/capability boundary:
   - `visible_canvas_mount_ids_json` 改为存 runtime mount id 同名值 `cvs-*`。
   - `append_visible_canvas_mounts` 不再从 unprefixed id 派生；直接按 `canvas.mount_id` 匹配。
   - `expose_canvas_mount_revision_and_adopt` 追加同名 mount id，并写 `visible_workspace_module_refs_json = ["canvas:cvs-*"]`。
   - owner bootstrap / composer_project_agent / SessionAssemblyBuilder 的输入字段如果继续叫 `visible_canvas_mount_ids`，必须明确它们是 VFS mount ids；否则重命名为 `visible_canvas_ids` 并另行派生。

4. Workspace module and tools boundary:
   - `WorkspaceModuleSummary.module_id` 统一为 `canvas:{canvas.mount_id}`，示例更新为 `canvas:cvs-dashboard-a`。
   - `WorkspaceModuleUiEntry.presentation_uri` 更新为 `canvas://{canvas.mount_id}` 或进一步拆成 `canvas-panel://{canvas.mount_id}`，后者能完全消除 `canvas://` 歧义。
   - `CanvasToolResult.canvas_id` 当前返回 logical mount id，应改为真正 UUID 或删除；推荐返回 `canvas_id = Canvas.id`、`mount_id = Canvas.mount_id`、`module_id = canvas:{mount_id}`、`presentation_uri = canvas://{mount_id}`。
   - `StartCanvasParams.canvas_id` 若实际表示 mount id，应改名为 `mount_id`；若表示 Canvas UUID，应用 UUID 查询。预研期无兼容要求，建议直接改名。

5. Contract/frontend/docs boundary:
   - 更新 `agentdash-contracts`、generated TS、workspace panel canvas tab opener、Canvas panel service。
   - 更新 embedded skills：`canvas:{mount_id}` 示例改为 `canvas:cvs-demo`，编辑 URI 仍为 `cvs-demo://...`。
   - 更新 `.trellis/spec/backend/vfs/vfs-access.md`：`visible_canvas_mount_ids` 不再存未加前缀值，`mount_id` 同名。

### Acceptance Points

- Creating Canvas without explicit id derives a `Canvas.mount_id` that starts with `cvs-` and is valid as a VFS URI scheme.
- Creating Canvas with explicit id rejects non-`cvs-` values, rejects `cvs-` with empty suffix, rejects `/`, `\`, `:`, whitespace, and reserved malformed forms.
- `workspace_module_create(kind="canvas")` returns one canonical `mount_id` such as `cvs-dashboard-a`; returned descriptor has `module_id = canvas:cvs-dashboard-a`, runtime VFS contains mount id `cvs-dashboard-a`, and no code path returns `cvs-cvs-dashboard-a`.
- `workspace_module_present(module_id="canvas:cvs-dashboard-a")` exposes the same VFS mount id, writes `workspace_module_presented.presentation_uri` using the canonical presentation URI, and updates AgentFrame visible refs before emitting the presentation event.
- `visible_canvas_mount_ids_json` and `active_vfs.mounts[].id` contain the same string for Canvas mounts.
- `canvas_fs` provider can read/write/list/search `cvs-dashboard-a://...`; generated binding files remain read-only and `source_uri` resolution still uses the session VFS.
- `root_ref` for `canvas_fs` no longer conflicts with presentation URI semantics; validation rejects stale/ambiguous root_ref shapes.
- DB has `UNIQUE(project_id, mount_id)` for canvases and repository create/update relies on the constraint instead of only pre-checks.
- No production code path uses global `find_by_mount_id` for a Project-scoped Canvas lookup.
- Specs, embedded skills, contract comments, and tests no longer describe `visible_canvas_mount_ids` as unprefixed values.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task, but the dispatch prompt explicitly provided `.trellis/tasks/06-23-canvas-vfs-runtime-binding-convergence` and this research file path, so output was written there.
- I did not modify business code, specs, contracts, migrations, or generated TS in this research pass.
- No external web references were needed; all references are project code and Trellis specs.
- I inspected infrastructure persistence/migration only to verify Canvas mount id uniqueness and repository lookup semantics.
- Frontend files were only surfaced by search; this research focused on requested backend/VFS/runtime/session/workspace module boundaries.
