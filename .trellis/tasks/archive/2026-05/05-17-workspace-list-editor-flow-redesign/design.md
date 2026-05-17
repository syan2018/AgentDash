# Workspace 列表与编辑流程重整 Design

## Mental Model

本次调整只重整 Project Workspace 页的消费侧心智，不改变后端权限总模型：

- Workspace 是 Project 下的逻辑身份，回答“这个 Project 需要哪个工作空间”。
- Binding 是 Workspace 的运行时落点，回答“当前可以通过哪个 backend/root 使用它”。
- Backend inventory 是 backend owner 或 backend runtime 提供的可用目录事实。
- Candidate 是 inventory 与现有 Workspace 匹配后的待确认项。
- 本机目录识别是个人本机用户的高频入口，回答“我就在这台机器上，把这个目录识别成 Workspace”。

关键取舍：本机目录识别不再叫“高级手填路径”，但它也不是“Project 任意选择 backend/root”。它仍然只能使用 Project 已授权 backend，并通过 detect 生成 identity 与 binding。

## UI Architecture

### Project Settings Workspace Tab

短期仍保留现有 `BackendAccessPanel + WorkspaceList` 页面结构，因为 backend 授权迁移到 Backend 设置页是独立任务。本任务只调整 Workspace 侧呈现：

- Backend Access 区域继续承载授权、刷新 inventory、查看 inventory 的临时入口。
- Candidate 信息需要从 Backend Access 的附属展示提升为 Workspace 创建/详情可以消费的入口。
- WorkspaceList 负责表达 logical Workspace 与 runtime resolution 结果。

### Workspace List Card

每张卡片展示四层信息：

- Identity：名称、identity kind、identity 摘要。
- Defaults：Project default workspace 的明确标记和操作。
- Availability：bindings 总数、ready/online 可用数、默认 binding。
- Resolution：当前会选用哪个 binding，或不可解析原因。

Resolution 在本任务内优先使用前端派生摘要：

- `default_binding_id` 命中且 binding ready，则显示默认 binding。
- 无 default 时按 `resolution_policy` 和 ready binding 粗略派生展示。
- 所有 binding 不可用、无 binding、backend 离线或无授权时展示诊断。

后端精确 runtime resolve API 不是本任务阻塞项；若实现中发现已有可用 API，则优先接入。

### Create Flow

新建 drawer 顶部使用模式切换，不再默认露出 backend/root 表单：

1. `从发现项创建`
   - 展示 unmatched candidates。
   - 选择 candidate 后预填 name、identity、初始 binding。
   - 用户确认后调用现有 `createWorkspace`。
2. `创建逻辑 Workspace`
   - 先填 name、identity kind、关键 identity 字段。
   - 可无初始 binding；如果后端当前要求必须有 binding，需要同步放宽创建校验。
   - 后续由 inventory sync 生成 binding。
3. `本机目录识别`
   - 面向个人本机常用路径，作为可见二级主入口。
   - 默认筛选 `backend_type === "local"`、online、已授权的 backend。
   - 支持目录浏览和 root_ref 输入。
   - 调用 `detectWorkspace(projectId, backendId, rootRef)`，展示 identity、binding、warnings、匹配到的现有 Workspace。
   - 用户确认后有两条动作：
     - `登记到 Backend Inventory`：把识别结果 upsert 到 backend inventory，让它进入 candidate / sync 流程。
     - `创建 Workspace`：直接用识别结果创建 logical Workspace 和初始 binding。
   - 这两条动作语义必须分开，避免用户误以为 Advanced Maintenance 会自动上报 inventory。

### Detail Flow

详情 drawer 拆成稳定分区：

- Identity：名称、identity kind、resolution policy、挂载能力；identity JSON 放高级展开。
- Resolution：当前解析摘要、无法解析诊断、Project default workspace 状态。
- Bindings：展示已确认 binding，突出 Workspace default binding、priority、状态、last verified。
- Candidates：展示匹配或疑似匹配当前 Workspace 的 inventory candidate，可确认生成 binding。
- Advanced Maintenance：手工编辑 binding backend/root/status/detected_facts、工作空间状态、删除。

## Data Flow

### Existing Data

- `WorkspaceList` 已接收 `workspaces`、`defaultWorkspaceId`、`onSetDefault`。
- `useWorkspaceStore` 已提供 `fetchWorkspaces`、`detectWorkspace`、`createWorkspace`、`updateWorkspace`。
- `backendAccess.ts` 已提供 access、inventory、candidate、sync 相关 API。
- `BackendConfig` 已包含 `backend_type`、`owner_user_id`、`online`、`accessible_roots` 等本机识别可用字段。

### New Frontend State

优先使用局部状态和派生数据：

- `WorkspaceList` 加载 candidates 与 project backend accesses，供创建/详情使用。
- 使用 `useMemo` 派生 authorized backend map、local backend list、candidate groups、card resolution summary。
- Workspace server cache 仍由 `workspaceStore` 维护。

### Candidate To Workspace

candidate 创建 Workspace 可复用现有 `createWorkspace`：

- `identity_kind` 来自 candidate。
- `identity_payload` 来自 candidate。
- 初始 binding 使用 candidate 的 `backend_id`、`root_ref`、`detected_facts`、`status`，priority 可先置 0。
- name 由 identity 或 root tail 派生，用户可改。

### Manual Detect

本机目录识别复用现有 detect API：

- detect 只负责识别和预填，不直接保存。
- 登记 backend inventory 时走 ProjectBackendAccess inventory endpoint，后端负责再次 detect 并 upsert `BackendWorkspaceInventory`。
- 保存 Workspace 时仍走 `createWorkspace` 或 `updateWorkspace`。
- detect 返回 matched workspace ids 时，UI 需要提示用户可能应该打开已有 Workspace，而不是重复创建。

### Backend Inventory Register

新增 Project-scoped endpoint：

- `POST /projects/{project_id}/backend-access/{access_id}/inventory/register`
- 输入：`root_ref`
- 行为：
  - 校验当前用户对 Project 有 edit 权限。
  - 校验 access 属于 Project 且 active。
  - 通过 Runtime Gateway 调用 `workspace.detect`。
  - 将检测成功结果 upsert 为 `BackendWorkspaceInventorySource::CapabilityExpansionAck` 或等价的能力扩展来源。
  - 返回登记后的 inventory item。

这个 endpoint 不扩大 local runtime 的 `accessible_roots`。如果目录不在 backend 可访问边界内，detect 会失败并把错误返回给 UI。

## Boundaries

本任务包含：

- Workspace list/card 的信息架构调整。
- Workspace create/detail drawer 的模式和分区调整。
- Candidate 创建入口。
- 本机目录识别入口与登记 backend inventory 动作。
- Inventory 展示点击与刷新链路的回归验证。
- Workspace 抽屉登记成功后通知 Backend Access 面板刷新 candidates 和已展开 Inventory。

本任务不包含：

- Backend 设置页承载 backend owner 授权 Project 的正式迁移。
- server-side 高权限用户批量拓展 local runtime accessible roots 的权限模型。
- worktree 创建能力。
- 同一个 backend 只给某个 Project 使用单一 root 的精细 root policy 编辑。

## Migration Notes

本任务预计不需要数据库迁移。若为了允许“创建空 logical Workspace”发现后端 create API 强制要求 binding，需要在后端 domain/API 中调整校验，并补充相应测试；这是正确模型的一部分，不做兼容绕路。

## Trade-offs

- 将本机目录识别作为可见二级入口，会让 UI 上仍能看到 backend/root，但不会让它压过 Workspace identity 模型。
- Candidate 入口更符合 backend 授权 Project 的方向，但依赖 backend 已上报或刷新 inventory。
- Detail 中保留 Advanced Maintenance 能降低调试成本，但必须在视觉上和常规编辑分层，避免普通用户以为需要维护 detected_facts。
