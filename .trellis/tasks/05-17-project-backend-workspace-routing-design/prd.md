# Project Backend Workspace 自动路由设计

## Goal

重新设计 Project、Backend、Workspace、accessible roots、WorkspaceBinding 之间的串联关系，让 Project 在获得 backend 访问权限后，能够基于 backend 自身管理/上报的 workspace inventory 自动发现、匹配和路由可用 workspace，减少当前需要用户在多个设置入口反复手动填写 backend + root_ref + binding 的操作。

最终目标不是简单美化设置页，而是把领域模型收敛成：

`Backend 管理物理可访问资源 → Project 声明可访问哪些 backend / workspace → Workspace 表达逻辑身份 → WorkspaceBinding 由发现和确认流程生成 → RuntimeResolution 自动选择可用 binding`

## Background

当前代码和既有设计已经具备部分正确方向：

- `Workspace` 领域模型已经是逻辑工作空间聚合，包含 `identity_kind`、`identity_payload`、`resolution_policy`、`bindings`。
- `WorkspaceBinding` 已表达某个逻辑 workspace 在某个 backend 上的物理落点，包含 `backend_id`、`root_ref`、`detected_facts`、`priority`。
- `Project` 不再直接持有 `backend_id`，而通过 `default_workspace_id` 进入运行时解析。
- Local Runtime profile 支持配置 `accessible_roots`，runtime health 也会上报 backend 当前可访问路径。
- workspace detection 已能通过 backend + root_ref 探测 Git / P4 / LocalDir facts，并生成 identity contract。

但当前串联仍存在明显断点：

- Project 授权只覆盖用户/用户组，不覆盖“允许访问哪些 backend / backend scope / workspace inventory”。
- `accessible_roots` 与 `WorkspaceBinding.root_ref` 是两套重复配置，用户必须先在 Local Runtime 配 root，再在 Project Workspace 中手动选 backend 和 root。
- 目录浏览入口可以从 backend 直接浏览路径，但没有和 Project 权限、accessible roots、workspace discovery 形成统一权限边界。
- Project 设置页仍把 logical workspace、binding、runtime preview、高级 identity JSON 暴露在同一主流程里。
- 部分前端链路仍在 default workspace 缺失时取第一个 workspace，和“必须显式解析”的设计方向冲突。

## User Value

- Project 管理者只需要授权“这个 Project 可以使用哪些 backend / backend scope”，系统自动发现和候选匹配可用 workspace。
- Backend 或 Local Runtime 可以根据自身镜像、机器、accessible roots、workspace facts 管理可用 workspace，而不是要求每个 Project 手工绑定路径。
- 自动化流程启动时可以按逻辑 workspace identity 自动路由到在线 backend，不依赖用户预先挨个填 binding。
- 设置页能把“权限授权”“逻辑 workspace”“自动发现结果”“运行时解析预览”分开表达，减少误操作和重复劳动。

## Confirmed Facts

- 领域模型已经支持 `Workspace` 作为逻辑身份，`WorkspaceBinding` 作为 backend + root_ref 候选绑定。
- 后端已有 `WorkspaceResolution`，可以按 `PreferOnline` / `PreferDefaultBinding` 选择 binding。
- Local Runtime 已上报 `accessible_roots`，并持久化到 `runtime_health`。
- `ensure_local_runtime` 当前只开放 user scope；project/system shared runtime scope 在 API 中被拒绝。
- `list_backends` / `browse_directory` 当前没有 Project context，不足以表达 Project 级 backend 权限。
- 既有历史任务 `03-24-project-workspace-backend-refactor` 已提出 logical workspace + binding + resolution 的方向，本任务是在此基础上补齐 ProjectBackendAccess 与 backend inventory 自动发现。

## Decisions

- ProjectBackendAccess 首版采用 **backend 级授权 + capability/root policy 预留字段**。Project 先声明可使用哪些 backend / backend scope / capability slot，默认只允许使用 backend 已上报的 `accessible_roots` 内资源。root/workspace 级 allowlist 记录为 backlog，不进入首版主路径。
- Backend workspace inventory 采用 **cloud 持久化快照**。online backend 负责刷新 facts，cloud 保存最近一次可诊断、可匹配的 inventory，不把自动匹配依赖在实时在线查询上。
- WorkspaceBinding 生成策略采用 **匹配已有 logical workspace 自动 upsert binding，创建新 logical workspace 需要用户确认**。这样能消除重复手填 binding，又避免系统静默制造新的业务对象。
- 多 backend 命中同一 logical workspace 时，runtime resolution 使用 **显式优先级的确定性策略**：默认 binding 优先；否则按 binding priority / access priority / backend_id + binding_id 稳定排序，不使用“最近使用”这类隐式状态。
- 首版 UI 先嵌入 Project Settings 的 Workspace tab，拆成 Backend Access、Logical Workspaces、Discovered Candidates、Runtime Preview 四块；等信息密度继续上升后再考虑拆独立 Backend Access 页面。

## Requirements

### R1. Project 必须能声明 Backend 访问权限

引入或设计 Project 级 backend access/grant 模型，表达：

- Project 可访问哪些 backend、backend scope 或 backend capability slot。
- 访问模式，例如 browse、detect、execute、read/write、prepare。
- 是否允许使用 backend 上所有 accessible roots，还是仅允许某些 root policy。
- 权限校验必须进入 API / runtime resolution，而不是只停留在前端展示。

### R2. Backend 必须拥有自己的 workspace inventory / discovery 责任

Backend 不应只是被动接受 `backend_id + root_ref`，而应能基于自身 `accessible_roots` 发现可用 workspace：

- Git repo facts。
- P4 workspace facts。
- LocalDir facts。
- binding labels / machine role / image role 等可用于匹配的附加信息。
- 发现结果应能被 cloud 查询、缓存或刷新，并带有诊断信息。

### R3. Project 授权 Backend 后应自动匹配 Workspace

当 Project 获得某个 backend 访问权限后，系统应能：

- 从 backend inventory 中找出与 Project logical workspace contract 匹配的候选。
- 自动创建或刷新 `WorkspaceBinding` 草案。
- 对匹配已有 workspace、创建新 workspace、冲突或歧义给出明确诊断。
- 用户只需要确认关键结果，而不是手工填写每个 binding。

### R4. Workspace 主流程必须以逻辑身份为中心

Project Workspace 页面应把主流程改为：

- 创建或选择 logical workspace。
- 查看系统自动发现的候选 backend/root。
- 确认默认 resolution policy 或默认 binding。
- 查看 runtime resolution preview。

手动输入 `root_ref`、编辑 `identity_payload` JSON、手工改 binding status 应降级为高级模式。

### R5. 权限边界必须统一

目录浏览、workspace detect、inventory refresh、runtime execution 都必须经过同一套 ProjectBackendAccess / backend scope / accessible roots 边界。

不应出现：

- browse 可以看见 detect 不允许的路径。
- Project 没有 backend access 但仍能通过 workspace binding 启动执行。
- runtime resolution 选择了当前 Project 无权访问的 backend。

### R6. 移除隐式 workspace 兜底

继续清理 default workspace 缺失时取第一个 workspace 的残留逻辑。未配置或无法解析时应展示明确诊断。

### R7. 迁移策略保持“当前正确状态”

项目仍在预研期，不需要为旧 API / 旧字段设计兼容方案。涉及数据库模型时需要提供 migration，并优先把模型迁到最正确的状态。

### R8. Backend 能力扩展治理作为关联任务处理

本任务只要求 ProjectBackendAccess / backend inventory 的模型为后续能力扩展治理预留边界，不直接设计 server-control、backend ack、批量扩展 accessible roots 或 worktree 创建授权。

相关内容拆分到 `05-17-backend-capability-expansion-governance`：

- server 侧如何请求/批准/下发/审计 backend 能力扩展。
- `personal` / `project_managed` / `system_managed` backend 信任模式。
- 高权限用户如何集中管理大量设备能力。
- 普通用户或自动化流程如何通过受控请求扩展可用目录。
- 未来 worktree / prepare workspace 如何触发 backend 能力扩展。

## Acceptance Criteria

- [ ] 明确 ProjectBackendAccess / BackendGrant 的领域归属、字段、权限语义和 API 边界。
- [ ] 明确 backend workspace inventory / discovery 的数据模型、刷新时机、事实字段、缓存位置和诊断语义。
- [ ] 明确 Project 授权 backend 后如何自动匹配或生成 WorkspaceBinding，包括冲突、歧义、离线 backend 的处理。
- [ ] 明确与 `05-17-backend-capability-expansion-governance` 的边界：本任务只消费 backend 已声明/已上报的能力，不负责扩展这些能力。
- [ ] 明确设置页信息架构：Project backend access、logical workspace、discovered candidates、runtime resolution preview 分别在哪里展示。
- [ ] 明确目录浏览 / detect / runtime execution 如何共享权限校验。
- [ ] 列出需要移除的“取第一个 workspace”残留点，并定义替代诊断行为。
- [ ] 形成 `design.md`，包含领域边界、数据流、API 草案、数据库迁移方向和主要权衡。
- [ ] 形成 `implement.md`，拆分可执行阶段、验证命令、风险文件和回滚点。

## Out Of Scope

- 不在本任务中直接实现完整自动化。
- 不设计旧字段兼容层。
- 不把问题收束为单纯的 UI 表单改版。
- 不把 backend inventory 做成只服务某个前端页面的临时接口。
- 首版不实现“同一个 backend 只给某个 Project 使用其中某一个 root”的精细 allowlist；仅作为 backlog / schema 预留。
- 不设计 server 侧 backend 能力扩展治理；该主题由 `05-17-backend-capability-expansion-governance` 跟踪。

## Open Questions

- 无阻塞问题。进入实现前只需用户 review 并确认规划方向。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
