# Canvas VFS 与 runtime binding 语义收束

## Goal

收束 Canvas 在 VFS、workspace module、runtime snapshot、runtime bridge、API/前端之间的身份命名与动态资源读取语义，让 Canvas authoring、preview、browser runtime bridge 和 Agent VFS 看到同一套清晰的事实源。

## User Value

- Agent 和前端都不再猜 `canvas_id`、`mount_id`、`cvs-...`、`canvas://...` 各自代表什么。
- Canvas binding 文件、runtime preview、runtime bridge 调用共享同一套动态绑定读取逻辑，避免一边是快照、一边是实时桥接的隐性分叉。
- 未上线阶段可以一次性做破坏式整理，不保留旧字段/旧路径兼容层。

## Confirmed Facts

- Canvas presentation URI 是 `canvas://{mount_id}`，用于 WorkspacePanel 打开 Canvas tab。
- Canvas Agent/VFS 编辑 URI 是 `cvs-<mount_id>://...`，当前由 `build_canvas_mount_id(canvas)` 生成。
- Canvas domain 里 `Canvas.mount_id` 当前是不带 `cvs-` 的稳定业务 id；VFS mount id 当前是 `cvs-{mount_id}`。
- `workspace_module` 目前使用 `canvas:{mount_id}` 作为 module id。
- 近期已将 `canvas-system` 收束为 lifecycle-projected skill，并移除了 Canvas 文件树隐藏 skill。
- 近期已将 binding 文件路径改为 `bindings/<alias>.<ext>`，并在 Canvas mount metadata 里暴露一次解析出的只读生成文件。
- 当前 binding 解析仍是 exposure-time snapshot：source 文件变化后不会自动反映到 `cvs-...://bindings/...`，除非重新 expose/present 或 bind。
- 用户倾向于把 Canvas VFS mount 语义统一成强制 `cvs` 前缀规则，并考虑让 mount id 本身同名为 `cvs...`，减少 `mount_id` 与 `vfs_mount_id` 的二次派生。
- 用户希望 binding 使用“动态绑定”的统一底层逻辑，runtime bridge 与 binding 完全走同一套实现。
- 后端研究确认 `Canvas.mount_id`、runtime VFS mount id、`visible_canvas_mount_ids_json`、workspace module ref 当前分属未加前缀/加前缀/`canvas:` 三种字符串空间，恢复和投影时需要反复记住派生规则。
- Contract/frontend 研究确认 `canvas_id` 当前同时表示 UUID 与 Canvas mount slug，`mount_id` 同时表示 Canvas slug 与 VFS mount id；未上线阶段适合一次性破坏式重命名。
- Runtime bridge/binding 研究确认当前最接近的共享底层是 `VfsService.read_text/read_binary`，还不存在同时负责 runtime context、binding text、asset blob 的 Canvas runtime resource service。

## Requirements

- 统一 Canvas identity vocabulary：
  - `canvas_id` 只表示 Canvas 数据库 UUID。
  - `canvas_mount_id` 表示 Canvas 的项目内稳定 mount key，并采用强制 `cvs-` 前缀规则。
  - `vfs_mount_id` 对 Canvas 来说与 `canvas_mount_id` 同名，均为 `cvs-...`，不再存在 `dashboard-a -> cvs-dashboard-a` 的隐式派生。
  - `canvas:{canvas_mount_id}` 表示 workspace module id。
  - `canvas://{canvas_mount_id}` 表示 WorkspacePanel presentation URI。
  - `{canvas_mount_id}://...` 表示 VFS authoring URI。
  - 工具/API/前端字段名必须表达真实语义，不再用 `canvas_id` 返回非 UUID 的 mount key。
- 统一动态 binding 底层：
  - binding 文件、runtime snapshot、runtime bridge context、runtime bridge asset/data 读取共享一个 application 层 Canvas runtime resource service。
  - 该服务以 session active VFS + VfsService 为事实源，按 URI/source binding 解析文本数据与 VFS asset URL。
  - binding 文件应采用 read-time dynamic view：读取 `bindings/<alias>.<ext>` 时反映当前 source 内容；snapshot 可以 materialize 当次内容，但 resolver 仍是同一服务。
- 收紧 binding 类型：
  - Canvas data binding 只支持文本兼容内容类型。
  - 二进制资产不通过 data binding 内联，继续走 runtime bridge asset URL。
- VFS listing 语义完整：
  - Canvas 生成 binding 文件在 list/read/search 中都应带 generated/read-only/alias/source/content_type/resolved 等元数据。
  - 写、删、重命名生成文件必须被一致拒绝。
- Frontend/contract 同步：
  - 生成 TS contract、手写类型、Canvas editor/preview/workspace module 调用需同步字段命名。
  - WorkspacePanel 仍只从 `presentation_uri` 打开 Canvas tab，不从 VFS URI 反推。
- Provider root_ref 与 presentation URI 解耦：
  - `canvas://...` 只作为 presentation URI。
  - Canvas VFS provider root_ref 改为不会与 browser-facing presentation 混淆的内部 scheme 或结构化 root。
- Specs and skills：
  - 更新 Canvas skill、workspace module skill、cross-layer/frontend/backend spec，使新的身份和动态 binding 语义成为后续开发准则。

## Acceptance Criteria

- [ ] 代码中 Canvas 身份相关字段命名清晰，`canvas_id` 只承载 UUID，Canvas mount key 使用 `canvas_mount_id`。
- [ ] Canvas mount id 的 `cvs-` 前缀规则由 domain/application helper 强制生成和校验，没有散落字符串拼接，也不会产生 `cvs-cvs-*`。
- [ ] `canvas://...` presentation URI、`cvs...://...` VFS URI、`canvas:{...}` module id 的生成与解析有单一 helper 或清晰边界。
- [ ] runtime snapshot binding 文件、Canvas mount generated binding 文件、runtime bridge asset/data 读取同一套 Canvas runtime resource resolver。
- [ ] 修改 binding source 文件后，读取 `bindings/<alias>.<ext>` 能反映当前 source 内容，并有自动化测试覆盖。
- [ ] 非文本兼容 `content_type` 的 data binding 被拒绝，并有测试覆盖。
- [ ] Canvas mount list/read/search 对生成 binding 文件返回一致元数据，并有测试覆盖。
- [ ] API contracts、generated TS、frontend Canvas/workspace module 调用与测试全部同步，旧字段别名被移除。
- [ ] 数据库 migration 将既有 Canvas mount key 规范化为 `cvs-...`，并补齐 `(project_id, mount_id)` 或重命名后等价字段的唯一约束。
- [ ] Canvas/system skills 与 Trellis specs 记录新的正向架构语义。
- [ ] 聚焦 Rust 与前端测试通过；不引入兼容旧字段/旧路径的回退逻辑。

## Out Of Scope

- 不重做 Canvas UI 视觉体验。
- 不引入二进制 data binding；图片/文件资产继续通过 runtime bridge asset URL 或现有 VFS asset 机制处理。
- 不保留旧 `canvas_id` 混用字段或旧 binding 固定 `.json` 路径兼容层。

## Open Questions

- 用户最终确认是否接受推荐方案：Canvas 持久化 mount key 也改为 `cvs-...` 同名形态，binding 文件采用 read-time dynamic view，浏览器公开 API 仍保持 `invoke` / `assets.url` / imported binding files 三种分工但共享服务端底层。
