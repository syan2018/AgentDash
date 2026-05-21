# VFS Mount 与 Filespace 扁平化重构设计

## Architecture

把 b362100c 的双层 (`ProjectFilespace` 资产 + `ProjectVfsMountBinding` 挂载) 合并为单层 `ProjectVfsMount`，content 异构地承载 inline 文件 / external service。Marketplace 资产从 `filespace_template` 改名为 `vfs_mount_template`，覆盖两种 content 子类型。整个改动 hard cut，不保留兼容路径。

```
┌─────────────────────────────────────────────────────────────────┐
│ ProjectVfsMount                                                 │
│   id (db pk uuid, internal)                                     │
│   project_id, mount_id (api identifier), display_name           │
│   description?, installed_source?                               │
│   capabilities, default_write                                   │
│   content: Inline | ExternalService { service_id, root_ref }    │
│                                                                 │
│   inline_fs_files(owner_kind="project_vfs_mount",               │
│                   owner_id=mount.id) ← Inline 文件存储          │
└─────────────────────────────────────────────────────────────────┘
```

## Domain

### 实体

```rust
pub struct ProjectVfsMount {
    pub id: Uuid,
    pub project_id: Uuid,
    pub mount_id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub capabilities: Vec<MountCapability>,
    pub default_write: bool,
    pub installed_source: Option<InstalledAssetSource>,
    pub content: ProjectVfsMountContent,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectVfsMountContent {
    Inline,
    ExternalService { service_id: String, root_ref: String },
}
```

### Repository

`ProjectVfsMountRepository`（替代旧 `ProjectFilespaceRepository` + `ProjectVfsMountBindingRepository`）：

```rust
async fn create(&self, mount: &ProjectVfsMount) -> Result<(), DomainError>;
async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectVfsMount>, DomainError>;
async fn get_by_project_and_mount_id(
    &self, project_id: Uuid, mount_id: &str,
) -> Result<Option<ProjectVfsMount>, DomainError>;
async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ProjectVfsMount>, DomainError>;
async fn update(&self, mount: &ProjectVfsMount) -> Result<(), DomainError>;
async fn delete(&self, project_id: Uuid, mount_id: &str) -> Result<(), DomainError>;
```

### InlineFile owner_kind

`InlineFileOwnerKind` 枚举：删除 `ProjectFilespace`，新增 `ProjectVfsMount`。`Story` owner 保留供 Story 局部 inline VFS 使用。

## Schema (migration 0054)

```sql
CREATE TABLE project_vfs_mounts (
    id              UUID PRIMARY KEY,
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    mount_id        TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    description     TEXT,
    capabilities    JSONB NOT NULL DEFAULT '[]',
    default_write   BOOLEAN NOT NULL DEFAULT FALSE,
    installed_source JSONB,
    content         JSONB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, mount_id)
);

-- 1) Inline 来源：合并 filespace + binding(filespace) 行
INSERT INTO project_vfs_mounts (id, project_id, mount_id, display_name, description,
                                capabilities, default_write, installed_source,
                                content, created_at, updated_at)
SELECT b.id,                          -- 用 binding.id 作为新 mount.id（inline_fs_files 引用 owner_id 的那一侧）
       b.project_id,
       b.mount_id,
       b.display_name,
       f.description,
       b.capabilities,
       b.default_write,
       f.installed_source,
       jsonb_build_object('kind', 'inline'),
       LEAST(b.created_at, f.created_at),
       GREATEST(b.updated_at, f.updated_at)
FROM project_vfs_mount_bindings b
JOIN project_filespaces f ON (b.source->>'kind' = 'filespace'
                              AND (b.source->>'filespace_id')::uuid = f.id);

-- 2) External 来源：直接搬 binding 行，content 用 binding.source
INSERT INTO project_vfs_mounts (id, project_id, mount_id, display_name, description,
                                capabilities, default_write, installed_source,
                                content, created_at, updated_at)
SELECT b.id,
       b.project_id,
       b.mount_id,
       b.display_name,
       NULL,
       b.capabilities,
       b.default_write,
       NULL,
       jsonb_build_object(
         'kind', 'external_service',
         'service_id', b.source->>'service_id',
         'root_ref', b.source->>'root_ref'
       ),
       b.created_at,
       b.updated_at
FROM project_vfs_mount_bindings b
WHERE b.source->>'kind' = 'external_service';

-- 3) inline_fs_files owner 改写：旧 owner_id 是 filespace.id，新 owner_id 是 mount.id（即原 binding.id）
UPDATE inline_fs_files i
   SET owner_kind = 'project_vfs_mount',
       owner_id   = b.id
  FROM project_vfs_mount_bindings b
 WHERE i.owner_kind = 'project_filespace'
   AND b.source->>'kind' = 'filespace'
   AND (b.source->>'filespace_id')::uuid = i.owner_id;

-- 4) 清理 owner_kind 检查约束（infra 层有相应 CHECK 时同步调整）

-- 5) DROP 旧表
DROP TABLE project_vfs_mount_bindings;
DROP TABLE project_filespaces;

-- 6) Marketplace：直接清掉 filespace_template 行
DELETE FROM library_assets WHERE asset_type = 'filespace_template';
```

> 关于 mount.id 取 binding.id：因为 inline_fs_files 的 owner_id 必须有稳定 UUID，借用 binding.id 让 step 3 的 UPDATE 能走 binding 关联，避免引入新 UUID 后 inline_fs_files 找不到 owner。fix 任务里 binding 是 Filespace 创建时一次性生成的同名 binding，1:1 关系成立。多 binding 引用同一 Filespace 的极端情况在 b362100c 落地后**没有数据**（前端没有创建多 binding 的入口），无需处理。

## API

### 路由

```text
GET    /api/projects/{project_id}/vfs-mounts
POST   /api/projects/{project_id}/vfs-mounts
GET    /api/projects/{project_id}/vfs-mounts/{mount_id}
PUT    /api/projects/{project_id}/vfs-mounts/{mount_id}
DELETE /api/projects/{project_id}/vfs-mounts/{mount_id}
```

`{mount_id}` 是 path 标识符。改名走 PUT 时 path 用旧 mount_id，body 中 `mount_id` 是新值；后端校验冲突。

### 请求 / 响应

```rust
#[derive(Deserialize)]
pub struct CreateProjectVfsMountRequest {
    pub mount_id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<MountCapability>,
    #[serde(default)]
    pub default_write: bool,
    pub content: ProjectVfsMountContent, // Inline / ExternalService
}

#[derive(Deserialize)]
pub struct UpdateProjectVfsMountRequest {
    pub mount_id: String,         // 允许改名；冲突 -> 409
    pub display_name: String,
    pub description: Option<String>,
    pub capabilities: Vec<MountCapability>,
    pub default_write: bool,
    pub content: ProjectVfsMountContent,
}

#[derive(Serialize)]
pub struct ProjectVfsMountResponse {
    pub project_id: Uuid,
    pub mount_id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub capabilities: Vec<MountCapability>,
    pub default_write: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<InstalledAssetSource>,
    pub content: ProjectVfsMountContent,
    pub surface_ref: String,        // "project-vfs-mount:{project_id}:{mount_id}"
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

注意：`ProjectVfsMountResponse` **不暴露内部 db id**。所有外部消费者只看到 mount_id。

### 校验

- `mount_id` 走 `normalize_identifier`：禁止空白 / `/` / `\` / `:` / 保留字 `main`
- POST / PUT 时 `(project_id, new mount_id)` 唯一
- `capabilities` 走 `normalize_capabilities`：剔除 Exec、去重、空数组回退 read+list+search
- `default_write` 仅当 capabilities 含 Write 时 true
- DELETE 级联清理 `inline_fs_files` 中 `owner_kind=project_vfs_mount, owner_id=mount.id`

### VFS Surface

- `ResolvedVfsSurfaceSource` 删除 `ProjectFilespace { project_id, filespace_id }`，新增 `ProjectVfsMount { project_id, mount_id }`
- VfsBrowser source `{ source_type: "project_vfs_mount", project_id, mount_id }`
- `surface_ref` 字符串变为 `project-vfs-mount:{project_id}:{mount_id}`

## Shared Library

### 资产类型

- `LibraryAssetType::FilespaceTemplate` **删除**（domain enum + DB 枚举值）
- 新增 `LibraryAssetType::VfsMountTemplate`

### Payload

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VfsMountTemplatePayload {
    Inline {
        mount_id: String,
        display_name: String,
        description: Option<String>,
        capabilities: Vec<MountCapability>,
        default_write: bool,
        files: Vec<InlineMountFilePayload>,
    },
    ExternalService {
        mount_id: String,
        display_name: String,
        description: Option<String>,
        capabilities: Vec<MountCapability>,
        default_write: bool,
        service_id: String,
        root_ref: String,
    },
}

pub struct InlineMountFilePayload {
    pub path: String,
    pub content_kind: String, // "text" | "binary"
    pub content: Option<String>,        // text only
    pub mime_type: Option<String>,      // binary only
    pub size_bytes: u64,
    pub data_base64: Option<String>,    // binary only
}
```

### Publish / Install

`shared_library::publish::publish_vfs_mount_payload` 输入 `project_asset_id`（实际就是 `(project_id, mount_id)` 对，发布请求带 mount_id）：

- 读 `ProjectVfsMount` 权威状态
- Inline content：`InlineFileRepo.list_files_by_owner(ProjectVfsMount, mount.id)` → typed payload
- ExternalService content：直接序列化 service_id + root_ref

`shared_library::install::install_vfs_mount_template`：

- 创建一行 `ProjectVfsMount`（mount_id 取 payload mount_id 或 install request 覆盖；写 `installed_source`）
- Inline content：写 `inline_fs_files(owner_kind=project_vfs_mount, owner_id=new_mount.id)`
- ExternalService content：不需要写文件
- 不再有"安装后不自动挂载"中间态——新 mount 直接进入 Project VFS

### Source-status

`source_status` 响应里：

- 删除 `filespaces` 项
- 新增 `vfs_mounts` 项

### AssetPickerDrawer

- KIND_OPTIONS：`filespace` → `vfs_mount`，hint 改为 `Project VFS Mount → vfs_mount_template`
- AssetStep 的 FilespaceList 改为 VfsMountList，使用新 service `listProjectVfsMounts`
- 按 `installed_source` 过滤逻辑保留

### MarketplaceAssetDrawer

`FilespaceTemplateBody` → `VfsMountTemplateBody`：

- 解析 payload 区分 inline / external_service
- inline：显示文件数 + 前 12 个 path
- external_service：显示 service_id + root_ref + capabilities chip

## Session Construction / Runtime

`agentdash-application::vfs::mount` 中：

- `build_derived_vfs` 直接遍历 `ProjectVfsMount` 列表生成 mount table，不再经过 binding-derived 中间步
- Story override / disable / Agent VFS access policy 全部按 `mount_id` 工作；调用面不变
- `Vfs.mounts[i]` 中 inline 类 mount 仍走 `inline_fs` provider，root_ref 改为 `project-vfs-mount:{mount_id}`

`SessionConstructionPlan.surface.vfs` 输出形态不变；前端消费者继续按 mount_id 解析。

## Frontend

### 类目结构

- 删除 `FilespaceCategoryPanel`，新增 `VfsMountCategoryPanel`（路由路径 `/dashboard/assets/vfs-mount`）
- AssetsTabView 类目项：`Filespace` → `VFS Mount`；hint：`Project VFS 挂载点 (Inline 文件 / 外部服务)`

### VfsMountCategoryPanel

骨架与现 `FilespaceCategoryPanel` 相似（CardMenu / OriginBadge / PublishedBadge / Editor Dialog / ConfirmDeleteDialog / Notice）；区别：

- 卡片显示 content kind 标签（Inline / External）
- 创建 dialog 强制选 content kind：
  - Inline：mount_id / display_name / description；保存后立即进入文件编辑（VfsBrowser）
  - ExternalService：mount_id / display_name / description / service_id / root_ref；无文件编辑区，主体只显示运行时 preview
- 编辑 dialog：
  - Inline → 左 metadata + capabilities/default_write，右 VfsBrowser
  - ExternalService → 单列 metadata + service_id / root_ref + capabilities/default_write
- 卡片菜单：编辑 / 发布 / 删除（installed 隐藏发布）

### ProjectSettings ContextTab

- **删除 `MountBindingsPanel`**（Mount CRUD 完全收敛到 Assets/VFS Mount）
- 保留：「Project VFS Mount」跳转按钮（指向 `/dashboard/assets/vfs-mount`）+ 解析后的 VFS Mount preview + Runtime Preview VfsBrowser

### Service / Store

- 新建 `services/projectVfsMounts.ts`：`list / create / update / delete / publish`
- 删除 `services/projectFilespaces.ts`
- `useProjectStore.vfsMountBindingsRevision` 重命名为 `vfsMountsRevision`，含义不变；增删改 mount 后 bump
- `VfsAccessPicker` 数据源换成 `listProjectVfsMounts`，但展示语义不变（mount_id + display_name + capabilities → grant）

### VfsBrowser

- `Source` union 中 `project_filespace` → `project_vfs_mount`，字段 `{ project_id, mount_id }`
- `vfs_surfaces` API 路径不变，仅 surface_ref 形态变化

## VFS Access Policy / Agent Grant

完全不变。`AgentVfsAccessGrant { mount_id, capabilities }` 持久化形态保留；form-state、preset-form-fields、VfsAccessPicker 调用面不动。

## Migration 顺序与影响面

```
Domain (新增 ProjectVfsMount + ProjectVfsMountContent / 删 ProjectFilespace + ProjectVfsMountBinding)
  → Infrastructure (新建 repo + DROP/CREATE migration 0053)
  → Application (vfs::mount 用新实体；shared_library publish/install/source-status 用新类型)
  → API (新路由、删旧路由、surface 类型改名)
  → Frontend types / services / stores / components
  → Spec 更新
```

每一层在 hard cut 后都不保留旧符号；任何引用旧 `ProjectFilespace` / `FilespaceTemplate` 的代码都必须删除或改写，编译错误就是任务进度的一部分。

## Tradeoffs

- **不可逆破坏**：项目预研期允许，但合入后回到双层模型只能回滚 commit
- **Marketplace 已发布数据丢失**：用户已确认无存量
- **mount.id 借用 binding.id**：避开了 inline_fs_files owner_id 的二次 rewrite；前提是 fix 任务后没有"多 binding 引用同 Filespace"实例（事实如此）
- **mount_id 改名经过 PUT body 而非 path rename API**：前端需要清楚同一 PUT 既能改字段也能改 identifier，校验集中在后端
- **取消 Marketplace 二步挂载**：唯一损失是用户失去「装而不挂」的中间态，但权限实际由 Agent grant 控，UX 收益更大

## Required Spec Updates

- `.trellis/spec/cross-layer/shared-library-contract.md` — `vfs_mount_template` 章节、source-status 中 `filespaces` → `vfs_mounts`
- `.trellis/spec/backend/vfs/vfs-access.md` — 重写 Project VFS Mount 路由章节；删除 Filespace / Mount Binding 双层叙述
- `.trellis/spec/backend/shared-library.md` — 资产类型表更名
- `.trellis/spec/backend/index.md` / `cross-layer/index.md` / `frontend/index.md` — 检查并修订引用

## Rollback / Operational Notes

- migration 0054 失败时 fail fast；不写部分迁移
- 开发期可重建数据库；生产 rollback 不在本任务范围
- 任何对外暴露的 surface_ref 字符串在迁移时已改名，前端旧 cache（如有）需要用户重新刷新
