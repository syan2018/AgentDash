# Project Filespace 资产化与 Agent VFS 能力分配迁移设计

## Architecture

本任务将现有 Project inline VFS 拆为三层：

1. **Project Filespace Asset**
   - Project 内可编辑资产。
   - 文件内容使用 `inline_fs_files` typed storage。
   - 支持 Marketplace 显式 publish / install。

2. **Project VFS Mount Binding**
   - Project runtime 挂载策略。
   - 描述 Filespace 或 external service 如何以某个 mount id 进入 VFS。
   - 保存 capabilities、default_write、display name 等运行态属性。

3. **Project Agent VFS Access Policy**
   - Agent 能力设置。
   - 决定某个 Agent 能看到哪些 Project 级 VFS mount，以及每个 mount 在该 Agent session 中的有效访问权限。
   - 作为 Session Construction 的 Project-level VFS access policy resolver，输出 per-Agent effective mount table。

Story inline VFS 保持局部绑定，不进入 Project Filespace 资产层。

## Domain Model

### ProjectFilespace

建议新增领域实体：

```rust
pub struct ProjectFilespace {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub installed_source: Option<InstalledAssetSource>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

文件存储：

```text
inline_fs_files.owner_kind = "project_filespace"
inline_fs_files.owner_id = project_filespace.id
inline_fs_files.container_id = "files"
inline_fs_files.path = mount-relative file path
```

需要扩展 `InlineFileOwnerKind`，新增 `ProjectFilespace`。现有 `Project` / `Story` owner 继续保留给 Story 局部上下文与迁移过渡脚本使用；迁移完成后 Project 级 Filespace 主线使用 `project_filespace`。

### ProjectVfsMountBinding

建议作为 Project config 的结构化字段，或独立表。考虑后续查询、引用校验和迁移清晰度，优先独立表：

```rust
pub struct ProjectVfsMountBinding {
    pub id: Uuid,
    pub project_id: Uuid,
    pub mount_id: String,
    pub display_name: String,
    pub source: ProjectVfsMountSource,
    pub capabilities: Vec<MountCapability>,
    pub default_write: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum ProjectVfsMountSource {
    Filespace { filespace_id: Uuid },
    ExternalService { service_id: String, root_ref: String },
}
```

Project mount id 在同 Project 内必须唯一，并继续禁止占用系统保留 id（例如 `main`）。

### Agent VFS Access Policy

建议挂在 `AgentPresetConfig` 内，作为 Agent 级可配置能力的一部分：

```rust
pub struct AgentVfsAccessGrant {
    pub mount_id: String,
    pub capabilities: Vec<MountCapability>,
}

pub struct AgentPresetConfig {
    pub vfs_access_grants: Option<Vec<AgentVfsAccessGrant>>,
    // existing fields...
}
```

语义建议：

- `None`：未显式配置，按产品默认策略解析。
- `Some([])`：显式不授予任何 Project Filespace mount。
- 产品默认策略：新建 Project Agent 不授予任何 Project Filespace / Project VFS mount 权限。
- grant capabilities 是 Project mount binding / provider supported capabilities 的进一步收窄，不能越权增加底层 mount 不支持的能力。
- `capabilities=[]` 等价于不授予该 mount；最终 VFS 中应移除该 mount。
- `capabilities=[read, list]` 表示该 Agent session 中该 mount 的 `Mount.capabilities` 只包含 read/list；write/search 等工具必须按有效 mount capabilities 拒绝。
- migration 直接移除旧 `project_container_ids`，不转换为 `vfs_access_grants`；该字段没有用户可见配置入口，不作为兼容迁移来源。
- 运行路径不得读取原始 Project mount binding 来判权；所有 VFS tool 权限判断以 `SessionConstructionPlan.surface.vfs` 中的 effective mount capabilities 为准。

若希望和 `ToolCapabilityDirective` 完全同构，可后续扩展为 `vfs:<mount_id>` path directive；本任务第一版优先使用结构化 `vfs_access_grants`，因为 VFS access policy 的产物是实际 mount capabilities，不是工具 schema visibility。

## VFS Construction Flow

新的 session VFS 装配顺序：

```text
Workspace/system mounts
  -> ProjectVfsMountBinding derived mounts
  -> Story disabled inherited mount filtering
  -> Story local context containers append/override
  -> Agent VFS access policy resolve Project-owned reusable mounts
  -> Agent knowledge / skill / canvas / lifecycle projections
  -> validate_vfs
```

关键约束：

- Agent VFS access policy 只作用于 Project-owned reusable mounts。
- Access policy resolver 必须输出 effective mounts：未授权 mount 被移除，部分授权 mount 的 `capabilities` 被改写为交集结果。
- Story local inline mounts 是 Story 上下文事实，不迁移为 Project Filespace；是否对其再做 Agent 过滤留给后续单独任务。
- Workspace `main` 是否受 Agent VFS access policy 管控需要作为产品决策明确；默认建议仍由 file_read / file_write / shell_execute 工具能力控制。
- `CapabilityState.vfs.active` 必须等于 `SessionConstructionPlan.surface.vfs`。

## Shared Library

新增：

```rust
LibraryAssetType::FilespaceTemplate

pub struct FilespaceTemplatePayload {
    pub files: Vec<FilespaceTemplateFilePayload>,
}

pub struct FilespaceTemplateFilePayload {
    pub path: String,
    pub content_kind: String,
    pub content: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub data_base64: Option<String>,
}
```

第一版直接支持 text / binary 文件，不为 binary 文件设计特殊排除。`content_kind="text"` 时使用 `content`；`content_kind="binary"` 时使用 `mime_type + size_bytes + data_base64`。安装时必须还原为 `inline_fs_files` 的 typed content，避免 Marketplace roundtrip 丢失图片或其它二进制资产。

发布：

- 前端只传 `asset_kind = "filespace"`、`project_asset_id` 和元数据。
- 后端读取 ProjectFilespace 与 `inline_fs_files` 文件，生成 typed payload。
- text 文件写入 UTF-8 `content`；binary 文件写入 standard base64 `data_base64`，并保留 `mime_type` 与 `size_bytes`。
- payload digest 走现有 Shared Library digest 策略。

安装：

- 后端创建 ProjectFilespace。
- 将 payload files 写入 `inline_fs_files(owner_kind='project_filespace', container_id='files')`。
- binary payload 必须按 `data_base64` 解码为原始 bytes，并校验解码后大小与 `size_bytes` 一致。
- 写入 `InstalledAssetSource`。
- 不自动创建 ProjectVfsMountBinding，除非安装请求显式携带 `attach=true` 之类选项；第一版建议不自动挂载。

## API Surface

建议新增：

```text
GET    /api/projects/{project_id}/filespaces
POST   /api/projects/{project_id}/filespaces
GET    /api/projects/{project_id}/filespaces/{filespace_id}
PATCH  /api/projects/{project_id}/filespaces/{filespace_id}
DELETE /api/projects/{project_id}/filespaces/{filespace_id}

GET    /api/projects/{project_id}/vfs-mount-bindings
POST   /api/projects/{project_id}/vfs-mount-bindings
PATCH  /api/projects/{project_id}/vfs-mount-bindings/{binding_id}
DELETE /api/projects/{project_id}/vfs-mount-bindings/{binding_id}
```

文件浏览与编辑继续复用 VFS Surface API，新增 source：

```rust
ResolvedVfsSurfaceSource::ProjectFilespace { project_id, filespace_id }
```

Project preview surface 从 Project mount bindings 派生，而不是从 `project.config.context_containers` 派生。

## Frontend

### Assets

新增 Assets 类目：

```text
Filespace / 文件空间
```

能力：

- 列表卡片：display name、key、文件数、来源状态、是否被挂载。
- 创建：填写 key / display name / description，默认创建 `README.md` 或空文件空间。
- 编辑：复用 `VfsBrowser`，source 为 ProjectFilespace。
- 发布：接入 `PublishLibraryAssetDialog`，新增 `assetKind="filespace"`。
- 删除：若存在 mount binding 引用，必须提示并由后端阻止或要求先解除挂载。

### Project Settings

- VFS 资源 tab 改为 runtime preview / diagnostics。
- 移除 `ContextContainersEditor` 的 Project inline 创建职责。
- 提供跳转到 Assets Filespace 与 Project VFS mount binding 管理的入口。

### Agent Editor

- 在 Agent capability / knowledge / skill 附近新增 VFS 能力分配面板。
- 数据源来自 Project VFS mount bindings + Filespace metadata。
- 支持按 mount 配置有效 read/write/list/search 权限。
- UI 必须把 Project mount binding 支持能力作为上限，不允许勾选底层不支持的 capability。
- 不展示旧 `project_container_ids` 等价状态；该字段随迁移直接退役。

## Migration

新增迁移大致步骤：

1. 新建 `project_filespaces` 表。
2. 新建 `project_vfs_mount_bindings` 表。
3. 扩展 `inline_fs_files.owner_kind` check，允许 `project_filespace`。
4. 扫描每个 Project `config.context_containers`：
   - `inline_files`：创建 ProjectFilespace，迁移文件到 `inline_fs_files(project_filespace, filespace_id, "files")`，创建 Filespace mount binding。
   - `external_service`：创建 ExternalService mount binding。
5. 清理 Project config 中的 `context_containers` 字段或迁移为不再被运行主线读取的空值。
6. 从 `project_agents` 表、domain entity、DTO、前端类型中移除 `project_container_ids`，不生成默认 `vfs_access_grants`。

Story `context_containers` 与 `disabled_container_ids` 不生成 Filespace asset。Story disabled 仍按 mount id 生效。

## Tradeoffs

- 独立 `project_filespaces` 表比继续塞 Project config 更重，但能支撑 Marketplace 来源、引用校验、列表查询和文件归属。
- `vfs_access_grants` 使用结构化字段比塞进 `ToolCapabilityDirective` 少一些统一感，但更贴合 effective mount capability 计算，避免把 VFS access 和 tool schema visibility 混为一谈。
- 新 Agent 默认无 Project VFS 权限会增加一次显式授权操作，但与“VFS 是 Agent 能力设置”的产品语义一致。
- 安装 FilespaceTemplate 后不自动挂载会多一步操作，但避免 Marketplace 安装改变 Agent runtime 权限面。

## Rollback / Operational Notes

项目仍在预研期，不设计长期兼容分支。迁移失败应 fail fast。

开发期可通过数据库重建或迁移回滚脚本处理；生产级 rollback 不作为本任务要求。

## Required Spec Updates

- `.trellis/spec/backend/vfs/vfs-access.md`：补 Project Filespace 与 Project mount binding 的 canonical construction。
- `.trellis/spec/backend/shared-library.md`：补 `filespace_template` asset type。
- `.trellis/spec/cross-layer/shared-library-contract.md`：补前后端 payload schema 与 install/source-status 契约。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md` 或新增 VFS capability spec：记录 Agent VFS access policy 与 tool capability 的边界。
