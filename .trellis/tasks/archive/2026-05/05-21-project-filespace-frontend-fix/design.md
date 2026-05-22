# Project Filespace 前端能力修复设计

## Architecture

修复围绕三层：

1. **后端 DTO / 路由补齐**：`ProjectFilespaceResponse` 透出 `installed_source`；新增 mount binding 的 POST / DELETE。
2. **前端类目重写**：`FilespaceCategoryPanel` 与 SkillCategoryPanel 等价骨架；新增可选 `MountBindingsPanel` 子区块。
3. **状态联动**：通过 `useProjectStore` 暴露 `filespacesByProjectId` / `vfsMountBindingsByProjectId` 与 `invalidate*` 动作，让 Filespace / Binding 变更能跨组件刷新。

## Backend

### `ProjectFilespaceResponse`

```rust
pub struct ProjectFilespaceResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub surface_ref: String,
    pub installed_source: Option<InstalledAssetSource>, // 新增
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

`InstalledAssetSource` 已经在 Skill / MCP 等响应里使用，直接复用既有 serde shape，避免给前端引入新枚举。

### 新增路由

```text
POST   /api/projects/{project_id}/vfs-mount-bindings
DELETE /api/projects/{project_id}/vfs-mount-bindings/{binding_id}
```

#### POST 创建

请求体：

```rust
pub struct CreateProjectVfsMountBindingRequest {
    pub mount_id: String,
    pub display_name: String,
    pub source: ProjectVfsMountSource,        // 复用现有 enum：filespace / external_service
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    #[serde(default)]
    pub default_write: bool,
}
```

校验：

- `normalize_identifier` 复用现有逻辑（不允许空白 / `/` / `\` / `:` / `main` 保留字）。
- 若 `source = Filespace { filespace_id }`：必须存在且 `project_id` 一致；不要求 mount_id 与 Filespace.key 相同（允许多挂）。
- `mount_id` 在 Project 内唯一（含 default Filespace 自动 binding）；冲突返回 `409 Conflict`。
- `capabilities` 走 `normalize_capabilities`（剔除 Exec / 去重 / 默认补 read+list+search）。
- `default_write` 仅在 capabilities 包含 `Write` 时为 true。

#### DELETE 解绑

- 仅删除 binding 行，不级联 Filespace；返回 `{ ok: true }`。
- 删除后若该 Filespace 不再被任何 binding 引用，前端在 UI 上提示「未挂载」状态，但后端不删除 Filespace。

#### 与现有 `delete_filespace` 的关系

`delete_filespace` 仍然级联删除该 Filespace 的所有 bindings；新 DELETE binding 是更细粒度的入口。两者互不冲突。

### Repository

`ProjectVfsMountBindingRepository` 已有 `create` / `update` / `delete` 方法，新增路由直接复用，不需要 schema 变更。

### 测试

- `routes/project_filespaces.rs` 单元 / route-level 测试覆盖：
  - POST 成功 / 重复 mount_id 冲突 / 引用不存在 Filespace / 跨 Project Filespace；
  - DELETE 成功 / 删除不存在 binding / 跨 Project；
  - GET response 含 `installed_source`（user_authored / installed 两种 fixture）。

## Frontend

### `types/index.ts`

```ts
export interface ProjectFilespace {
  id: string;
  project_id: string;
  key: string;
  display_name: string;
  description?: string | null;
  surface_ref: string;
  installed_source?: InstalledAssetSource | null; // 新增
  created_at: string;
  updated_at: string;
}
```

`InstalledAssetSource` 复用 `types/shared-library.ts` 现有定义，确保字段命名与 Skill / MCP 一致。

### Service 层

`packages/app-web/src/services/projectFilespaces.ts` 新增：

```ts
export interface CreateProjectVfsMountBindingPayload {
  mount_id: string;
  display_name: string;
  source: ProjectVfsMountBinding["source"];
  capabilities: ProjectVfsMountBinding["capabilities"];
  default_write: boolean;
}

export async function createProjectVfsMountBinding(
  projectId: string,
  payload: CreateProjectVfsMountBindingPayload,
): Promise<ProjectVfsMountBinding>;

export async function deleteProjectVfsMountBinding(
  projectId: string,
  bindingId: string,
): Promise<{ ok: boolean }>;
```

### Store 联动（`projectStore`）

新增字段与动作：

```ts
filespacesByProjectId: Record<string, ProjectFilespace[]>;
vfsMountBindingsByProjectId: Record<string, ProjectVfsMountBinding[]>;

fetchProjectFilespaces(projectId): Promise<ProjectFilespace[]>;
fetchProjectVfsMountBindings(projectId): Promise<ProjectVfsMountBinding[]>;
invalidateProjectFilespaces(projectId): void;
invalidateProjectVfsMountBindings(projectId): void;
```

UI 组件改为读 store；`FilespaceCategoryPanel` 在创建 / 编辑 / 删除后调 `invalidate*`，`VfsAccessPicker` 在订阅 `vfsMountBindingsByProjectId[projectId]` 时自动响应变更。

> 备选方案：若不想动 store，可在 `VfsAccessPicker` 引入一个 reload tick prop，但当前 store 已经承载 `projectAgentConfigsByProjectId` 等同类数据，扩展更自然。

### `FilespaceCategoryPanel` 重写骨架

完全对齐 `SkillCategoryPanel` 的结构：

```text
<div className="flex h-full flex-col gap-4 p-6">
  <header>                                          // CreateButton
  <Notice notice={notice} onDismiss={clearNotice}/>
  <FilespaceGrid                                    // sm:2 / xl:3 cols
    items={filespaces}
    publishedByKey={publishedByKey}
    onEdit={openEdit}
    onPublish={setPublishTarget}
    onDelete={setConfirmDelete}
  />
  <FilespaceEditorDialog .../>                     // VfsBrowser inside
  <ConfirmDeleteDialog .../>
  <PublishLibraryAssetDialog .../>
</div>
```

- `OriginBadge` 通过 `resolveOriginBadge(filespace.installed_source ? "installed" : "user_authored", Boolean(filespace.installed_source))` 调用。Filespace 当前没有 builtin / github 等 source，统一以 `user_authored` 作为默认 source 字符串处理（不为它新建 `source` 字段，依据是否 `installed_source` 判定即可）。
- `PublishedBadge` 使用 `fetchLibraryAssets({ asset_type: "filespace_template", owner_id: currentUserId })` 的 `publishedByKey` map。
- `CardMenu` 三项：`编辑` / `发布到资源市场`（installed / 未登录用户隐藏） / `删除`。
- `Editor Dialog`：
  - 创建模式：表单 (key / display_name / description) + 创建后再打开 dialog 内 `VfsBrowser`；或直接两段式（先创建占位再编辑），保持与 Skill 创建对话框一致即可。
  - 编辑模式：左侧 metadata 表单（display_name / description / key）+ 右侧 `VfsBrowser source={{source_type: "project_filespace", project_id, filespace_id}}`。

### `VfsAccessPicker` 修复

```ts
useEffect(() => {
  if (!projectId) return;
  let cancelled = false;
  setIsLoading(true);
  setError(null);
  listProjectVfsMountBindings(projectId)
    .then((next) => { if (!cancelled) { setItems(next); } })
    .catch((err) => { if (!cancelled) setError(err instanceof Error ? err.message : String(err)); })
    .finally(() => { if (!cancelled) setIsLoading(false); });
  return () => { cancelled = true; };
}, [projectId, bindingsRevision]);
```

- 引入 `bindingsRevision` 来自 `useProjectStore((s) => s.vfsMountBindingsRevisionByProjectId[projectId] ?? 0)`，每次 `invalidate` 时 revision++。
- 直接订阅 `vfsMountBindingsByProjectId[projectId]` 也是可行替代；优先选订阅，避免重复 fetch。

### Mount Binding 管理 UI

放在 ProjectSettings Context Tab，作为 `Project Filespace` 与 `解析后的 VFS Mount` 之间的新 SectionCard `Project VFS Mount`。理由：

- ContextTab 本来就是 Project 级 VFS 配置入口；
- Filespace 类目页保持「资产即文件」的纯粹语义，不和 mount 配置混；
- 已有 `打开 Filespace 资产` 跳转入口，互补而不重叠。

UI 形态：

```text
SectionCard: "Project VFS Mount"
  - List rows: mount_id · display_name · source 摘要 · capabilities chips · default_write toggle · 解绑按钮
  - 行内编辑：capabilities checkbox group / default_write toggle → 触发 PUT
  - 顶部新建按钮：弹出小 dialog
    - source kind 选择：Filespace / ExternalService
      - Filespace 子表单：选 Filespace（来自 store），填 mount_id / display_name / capabilities / default_write
      - ExternalService 子表单：填 service_id / root_ref / mount_id / display_name / capabilities / default_write
```

ExternalService provider 列表来自 `/api/mount-providers`（`vfs::list_configurable_mount_providers`），可后续扩展；本任务首版允许手填 service_id + root_ref，复用现有 mount-providers 选择器（若已存在）。

### `AssetPickerDrawer.FilespaceList`

```ts
const visible = useMemo(
  () => (items ?? []).filter((f) => !f.installed_source),
  [items],
);
```

并在空态文案沿用 Skill 相同提示语。

### `AssetsTabView`

顶部描述补 Filespace：

```text
{currentProject.name} · 统一管理 Workflow / MCP / Skill / Filespace / Canvas 等项目级可复用资产
```

## Migration / Compat

无 schema 变更。后端只增加路由；旧 PUT 行为不变。前端 store 字段为新增；不影响既有 `projectAgentConfigsByProjectId` 行为。

## Tradeoffs

- **Store 化**：让 `VfsAccessPicker` 通过 store 联动比传 prop drilling 干净，但增加了 `projectStore` 体积。当前 `projectStore` 已经承载多个 `*ByProjectId` 字段，扩展是渐进的。
- **Mount Binding UI 放在 Context Tab**：与 Filespace 类目分离，路径上多一步跳转；好处是语义正交。Open Question 在 PRD 已记录，设计阶段确认放 ContextTab。
- **不引入 PATCH 路由**：保持后端表层简单；UI 改少量字段也走 PUT 整体覆盖。代价是前端必须发送完整 binding 对象，但这是 SkillAsset / McpPreset 既有模式。

## Required Spec Updates

- `.trellis/spec/cross-layer/shared-library-contract.md`：补 `installed_source` 在 ProjectFilespace DTO 的暴露策略。
- `.trellis/spec/backend/vfs/`（如存在 vfs-access.md）：补 mount binding POST/DELETE 路径与 mount_id 唯一性约束。
- `.trellis/spec/frontend/`：若有 Assets Panel UI 公约，补 Filespace 类目走同一 OriginBadge / PublishedBadge / CardMenu 模板。
