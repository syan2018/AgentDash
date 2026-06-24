# Canvas 个人与共用分发权限系统设计

## Architecture Summary

本任务把 Canvas 从“Project 下所有 editor 共同可写的资源”调整为“Project 内带 ownership 与 sharing state 的资产”。核心变化不是新增一层 UI 标记，而是把 Canvas 的可见性、可写性、发布来源和复制来源作为后端事实，再由 API、WorkspaceModule、VFS runtime surface、前端资产页共同消费。

MVP 采用 Project 内分发模型：

```text
个人 Canvas(source, editable by owner)
  -> publish/update publish
项目共用 Canvas(shared source, read-only for project members)
  -> copy
个人 Canvas clone(editable by new owner)
```

Shared Library / Marketplace 暂不进入第一期。第一期完成后再引入 `canvas_template`，可以复用本任务沉淀的 Canvas payload mapper 和 lineage 字段。

## Domain Model

### Canvas Scope

新增 Canvas scope 概念：

```rust
enum CanvasScope {
    Personal,
    Project,
}
```

- `Personal` 表示用户个人 Canvas。owner 可以编辑，其他用户默认不可见。
- `Project` 表示项目共用 Canvas。具备 Project view 的用户可见和可使用，默认只读。

### Ownership And Lineage

Canvas 需要新增或等价表达以下字段：

```text
owner_user_id: Option<String>
scope: personal | project
published_from_canvas_id: Option<Uuid>
shared_canvas_id: Option<Uuid>            # 可选，用于个人源快速定位当前发布
cloned_from_canvas_id: Option<Uuid>
published_at: Option<DateTime<Utc>>
published_by_user_id: Option<String>
```

实现时可以用更简洁字段组合，但必须覆盖三类事实：

- 当前 Canvas 属于谁。
- 当前 Canvas 是个人源还是项目共用源。
- 当前 Canvas 是从哪个 Canvas 发布或复制而来。

`Project` scope 的 Canvas 仍保留 `project_id`，原因是项目共用区可见性由 Project view 管理。个人 Canvas 也保留 `project_id`，原因是 Canvas runtime、workspace module、VFS surface 仍是 Project 内资产和 session surface 的组成部分。

### Effective Access

Application 层提供单一 access projection：

```rust
struct CanvasAccessProjection {
    can_view: bool,
    can_edit_source: bool,
    can_publish: bool,
    can_manage_shared: bool,
    can_copy: bool,
    runtime_write_allowed: bool,
}
```

推荐规则：

| Canvas scope | 身份 | view | edit source | publish/manage | copy | runtime write |
| --- | --- | --- | --- | --- | --- | --- |
| Personal | owner | yes | yes | yes | yes | yes |
| Personal | project owner/admin | optional/manage only | no direct source edit | manage only | optional | no |
| Personal | other user | no | no | no | no | no |
| Project | project viewer/editor | yes | no | no | yes | no |
| Project | publisher | yes | through update-publish only | yes | yes | no direct write |
| Project | project owner/admin | yes | through management/update-publish only | yes | yes | no direct write |

`runtime_write_allowed` 应只对 editable personal Canvas 为 true。项目共用 Canvas 即使由发布者打开，也推荐不直接写源；发布者修改个人源后重新发布，源与发布记录边界更清晰。

## Repository And Persistence

### Migration

新增 migration：

- 给 `canvases` 增加 owner/scope/lineage/published 字段。
- 给 `canvases` 增加必要索引：
  - `(project_id, scope)`
  - `(project_id, owner_user_id, scope)`
  - `(project_id, mount_id)` 继续保持唯一。
  - `published_from_canvas_id` 或 `cloned_from_canvas_id` 可按查询需要加普通索引。
- 既有数据迁移策略：
  - 如果 personal 模式有稳定 current user/system user，可迁为该用户个人 Canvas。
  - 如果无法可靠指定 owner，则迁为 `scope = project` 的项目共用 Canvas，保证现有项目成员仍可读取和使用。

预研阶段不做旧字段兼容，migration 应直接产出最终字段。

### Repository Methods

在 `CanvasRepository` 上补充围绕 scope 和 lineage 的查询：

```rust
list_by_project(project_id, filter)
list_personal_by_owner(project_id, owner_user_id)
list_project_shared(project_id)
find_published_from(source_canvas_id)
```

现有 `get_by_id` 和 `get_by_mount_id(project_id, canvas_mount_id)` 保留，但 application 层必须在返回前执行 Canvas access projection。

## Application Services

新增或扩展 `canvas::management` use cases：

- `create_personal_canvas(current_user, project_id, input)`
- `list_canvases_for_user(current_user, project_id, scope_filter)`
- `load_canvas_with_access(current_user, canvas_id, required_action)`
- `publish_canvas_to_project(current_user, canvas_id, input)`
- `update_project_canvas_publication(current_user, source_canvas_id, shared_canvas_id, input)`
- `copy_canvas_to_personal(current_user, source_canvas_id, input)`
- `unpublish_project_canvas(current_user, shared_canvas_id)`

发布和复制均为 deep copy：

- 新 UUID。
- 新 `canvas_mount_id`，由后端生成并校验唯一。
- 复制 `entry_file`、`sandbox_config`、`files`、`bindings`。
- 复制 title/description 时允许请求覆盖。
- 写入 lineage。

Mount id 冲突处理由 application helper 完成。推荐使用基础 slug + 短后缀，循环检查 `(project_id, mount_id)` 唯一。

## API Contract

### DTO

`CanvasResponse` 增加：

```text
owner_user_id: Option<String>
scope: "personal" | "project"
access: {
  can_view: bool
  can_edit_source: bool
  can_publish: bool
  can_manage_shared: bool
  can_copy: bool
  runtime_write_allowed: bool
}
published_from_canvas_id: Option<String>
cloned_from_canvas_id: Option<String>
published_at: Option<String>
published_by_user_id: Option<String>
```

字段命名应遵循 Canvas identity 收束任务的结果：`canvas_id` 只表示 UUID，`canvas_mount_id` 表示 `cvs-...` mount key，`vfs_mount_id` 对 Canvas 与 `canvas_mount_id` 同名。

### Routes

推荐路由：

```text
GET  /api/projects/{project_id}/canvases?scope=mine|shared|all
POST /api/projects/{project_id}/canvases
POST /api/canvases/{id}/publish-to-project
POST /api/canvases/{id}/copy-to-personal
POST /api/canvases/{id}/unpublish
```

现有 CRUD 路由继续存在，但 update/delete 必须基于 Canvas access，而不是只看 Project edit。

`promote-extension` 保持现有职责，但前端文案要和 publish-to-project 区分。

## VFS And Runtime Surface

Canvas runtime mount 构建必须接收 access projection：

```rust
build_canvas_mount(canvas, CanvasRuntimeAccess { writable: bool })
```

规则：

- editable personal Canvas：read/write/list/search。
- project shared Canvas：read/list/search。
- 非 owner personal Canvas：不应进入该用户 runtime surface。

`CanvasFsMountProvider` 当前 `edit_capabilities` 总是 create/delete/rename=true，写路径也不检查外部 access。实现时需要通过 mount metadata 或 mount capabilities 让 provider 判断：

- mount 缺少 write capability 时，`write_text`、`delete_text`、`rename_text` 返回 forbidden/not supported 语义。
- `edit_capabilities` 对只读 mount 返回 create/delete/rename=false。

WorkspaceModule visibility 和 AgentRun runtime surface 更新路径需要使用同一个 access projection，避免 HTTP 和 Agent VFS 行为不一致。

## WorkspaceModule Projection

`build_canvas_workspace_module` 需要根据 access 裁切 operations：

- editable personal Canvas：保留 `canvas.bind_data`。
- read-only project shared Canvas：只保留 UI entry / presentation，不暴露 `canvas.bind_data`。

`WorkspaceModuleSummary.permission_summary` 或 metadata 可追加只读说明，供 Agent/tool describe 理解为什么没有 mutation operation。

`workspace_module_create(kind="canvas")` 默认创建 personal Canvas。若请求 attach project shared Canvas，只允许 present/read，不允许 attach 后编辑。

## Frontend Design

`ProjectCanvasManager` 拆成更清楚的视图状态：

- Mine：个人 Canvas 列表，支持创建、编辑、发布、删除。
- Shared：项目共用 Canvas 列表，支持打开/预览、复制为我的 Canvas；管理者支持取消发布或删除共用源。

Canvas detail panel 接收 `access`：

- `can_edit_source=true` 时显示文件编辑、binding 编辑和保存入口。
- `runtime_write_allowed=false` 时保持 preview 可用，但编辑控件不可用或不出现。
- “发布到项目共用”和“发布为插件”作为两个不同按钮或菜单项，文案不能混用。

复制成功后默认选中新个人 Canvas，进入可编辑 detail。

## Shared Library Future Path

MVP 不新增 `LibraryAssetType::CanvasTemplate`。但发布/复制 mapper 应尽量抽出 `CanvasTemplatePayload` 形状，后续可以平滑进入 Shared Library：

```text
CanvasTemplatePayload {
  schema_version
  title
  description
  entry_file
  sandbox_config
  files
  bindings
}
```

后续 `canvas_template` install 只需要把 payload 安装成 personal Canvas 或 project shared Canvas。

## Trade-Offs

- 选择“共用源只读 + 复制编辑”避免多人直接共编辑带来的冲突、锁、版本历史和审计复杂度。
- 选择 Project 内 MVP 避免同时改 Shared Library asset type、install/source-status、Marketplace UI，降低第一期风险。
- 选择 application 层 effective access projection，避免 API、VFS、WorkspaceModule 各自手写权限规则。
- 选择发布为 deep copy，保证共用源在发布后稳定，不受个人源后续试验影响。

## Operational Notes

- 实现前需要检查当前 Canvas identity 收束任务状态，若其未完成，本任务实现必须以其 PRD 中的最终命名作为准绳。
- 实现前需要检查 AgentFrame Canvas projection 收束任务状态，runtime surface read-only 裁切应接入统一 update service，而不是新增旁路。
- 本任务改变数据库 schema 和 API contract，需要同步 migration、contract generation、frontend generated types。
