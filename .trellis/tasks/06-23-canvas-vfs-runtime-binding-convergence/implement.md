# Canvas VFS 与 runtime binding 语义收束执行计划

## Implementation Checklist

### 1. Canvas Identity 收束

- [ ] 新增或整理 Canvas identity helper，集中生成/解析：
  - `canvas_mount_id`
  - `vfs_mount_id`
  - `canvas:{canvas_mount_id}` workspace module id
  - `canvas://{canvas_mount_id}` presentation URI
  - `{canvas_mount_id}://...` VFS authoring URI
  - Canvas provider root ref
- [ ] 将 Canvas domain mount key 规范为强制 `cvs-...`，避免 `cvs-cvs-*`。
- [ ] `build_canvas_mount_id` 不再拼接 `cvs-`，要么删除，要么只校验并返回同名 id。
- [ ] `visible_canvas_mount_ids_json` 与 active VFS mount id 使用同一个 `cvs-...` 字符串。
- [ ] `workspace_module` 使用 `canvas:{canvas_mount_id}`，示例变为 `canvas:cvs-dashboard-a`。
- [ ] Canvas provider root ref 从 `canvas://{uuid}` 改为内部 scheme 或结构化 root，避免与 presentation URI 共用 scheme。

### 2. Database / Repository / API Contract

- [ ] 增加 migration，将既有 Canvas mount key 规范化为 `cvs-...`。
- [ ] 为 Canvas 增加 Project-scoped mount key 唯一约束。
- [ ] 收敛 `CanvasRepository::find_by_mount_id` 的生产路径，Project-scoped 查询显式使用 `project_id + canvas_mount_id`。
- [ ] Rust contracts 重命名：
  - `CanvasResponse.id` -> `canvas_id`
  - `CanvasResponse.mount_id` -> `canvas_mount_id`
  - `CreateCanvasRequest.mount_id` -> `canvas_mount_id`
  - Runtime snapshot 增加或明确 `canvas_mount_id` / `vfs_mount_id`
- [ ] Canvas route path 不再接受 UUID-or-mount 双义 ref；拆成 UUID route 与显式 mount lookup。
- [ ] Agent-facing tool params/results 重命名：
  - `canvas_id` 只表示 UUID
  - `canvas_mount_id` 表示 `cvs-...`
  - `vfs_mount_id` 表示 VFS mount id；Canvas 场景与 `canvas_mount_id` 同名

### 3. Canvas Runtime Resource Service

- [ ] 新增 `canvas/runtime_resource.rs`。
- [ ] 抽出 `CanvasRuntimeContext`，统一解析：
  - Canvas record
  - session active VFS
  - `resource_surface_ref`
  - RuntimeGateway actor/context
  - runtime bridge surface
- [ ] 把 `resolve_canvas_binding_files` 从 `canvas/runtime.rs` 迁入 resource service。
- [ ] Runtime snapshot route 和 session exposure/adopt 都消费同一个 resource service。
- [ ] runtime bridge asset/blob 读取共享同一 context 和 VFS URI parser；如新增 Canvas-specific asset route，需要服务端校验 image MIME。
- [ ] runtime invoke route 共享 `CanvasRuntimeContext` / admission，但 action execution 继续进入 `RuntimeGateway.invoke`。

### 4. Dynamic Binding View

- [ ] 将 generated binding files 从 exposure-time metadata snapshot 改为 read-time dynamic view，或通过 session overlay/provider projection 达到同等语义。
- [ ] 修改 `bindings/<alias>.<ext>` source 文件后，读取 Canvas mount 生成文件立即反映当前内容。
- [ ] 保留 unresolved 作为值状态：
  - JSON binding: `null`
  - 非 JSON text binding: empty content
  - metadata `resolved=false`
- [ ] `list/read/search` 返回 generated/read-only/alias/source_uri/content_type/resolved metadata。
- [ ] `write/delete/rename` 继续一致拒绝 generated binding 文件。
- [ ] 非文本兼容 `content_type` 在 binding mutation 阶段被拒绝。

### 5. Frontend Sync

- [ ] 重新生成 TS contracts。
- [ ] 更新 `packages/app-web/src/types/canvas.ts` facade。
- [ ] 更新 `packages/app-web/src/services/canvas.ts`，区分 UUID route 与 mount lookup。
- [ ] 更新 Canvas runtime panel/tab：
  - `canvas://...` 解析结果命名为 `canvasMountId`
  - 调 UUID API 前必须显式 resolve，不再把 mount id 当 `canvasId`
  - 文件浏览入口使用返回的 `vfs_mount_id`，不在前端拼 `cvs-`
- [ ] 更新 workspace module open logic，继续只从 `presentation_uri` 打开 Canvas tab。
- [ ] 更新 ProjectCanvasManager 字段名和展示。
- [ ] 更新前端 tests 和 fixtures。

### 6. Specs and Skills

- [ ] 更新 `canvas-system` skill。
- [ ] 更新 `workspace-module-system` skill。
- [ ] 更新 backend VFS/capability specs。
- [ ] 更新 cross-layer frontend-backend contracts。
- [ ] 更新 frontend architecture/type-safety specs。
- [ ] 清理误导性 `canvas://src/...` VFS 示例，改为 `cvs-demo://src/...`。

## Suggested Sub-Agent Split

- 后端 identity/migration worker：domain Canvas、repository、migration、VFS mount construction、session visibility、workspace module backend tests。
- Runtime resource worker：`canvas/runtime_resource.rs`、snapshot route、session exposure、runtime invoke/asset admission、binding dynamic read tests。
- Contract/frontend worker：Rust contracts、TS generation、frontend Canvas services/panels/workspace module tests。
- Check worker：跨层 review、spec compliance、lint/type/test。

## Validation Commands

- `cargo fmt`
- `git diff --check`
- `cargo test -p agentdash-domain canvas`
- `cargo test -p agentdash-application canvas`
- `cargo test -p agentdash-application workspace_module`
- `cargo test -p agentdash-api canvases`
- `pnpm run contracts:check`
- 前端类型/测试命令待实现前根据 `package.json` 确认。

## Risky Areas

- `Canvas.mount_id` 语义改为 `cvs-...` 会影响 workspace module id、visible canvas mount ids、frontend tab tests、stored AgentFrame visible refs。
- read-time dynamic binding 可能需要扩展 provider context 或引入 session overlay/provider projection，这是本任务最大的实现风险。
- runtime bridge 与 binding 合并底层时要避免把 browser API 合并成不清晰的大一统接口。
- generated TS contract 更新会牵出手写类型、fixtures、服务函数命名。
- 数据库唯一约束和 mount key migration 需要考虑现有重复数据处理；预研阶段可选择直接失败并修数据，不做兼容回退。

## Review Gate

进入实现前需要用户确认：

- 接受推荐方案：Canvas 持久化 mount key 改为 `cvs-...` 同名形态。
- 接受推荐方案：binding 文件采用 read-time dynamic view。
- 接受推荐方案：浏览器公开 API 保持 `invoke` / `assets.url` / imported binding files 三种分工，只共享服务端底层实现。
