# Project Backend Workspace 自动路由设计

## 目标边界

本设计把 Project、Backend、Workspace、WorkspaceBinding 的主链路收敛为：

1. Backend 上报或刷新自己可用的 workspace inventory。
2. Project 通过 ProjectBackendAccess 获得某些 backend / scope / capability slot 的访问权。
3. 系统把 backend inventory 与 Project 下的 logical Workspace identity contract 做匹配。
4. 对匹配已有 Workspace 的结果自动 upsert WorkspaceBinding。
5. RuntimeResolution 只在 ProjectBackendAccess 允许的 backend / binding 中选择执行落点。

本任务不负责 server 侧能力扩展治理。backend 能力如何被 server 请求、批准、下发、确认和撤销，由 `05-17-backend-capability-expansion-governance` 跟踪。本任务只消费 backend 已声明、已上报、已确认可用的能力。

## 领域模型

### ProjectBackendAccess

新增 Project 级授权聚合或实体，用于表达“这个 Project 可以使用哪些 backend 能力”。

建议字段：

- `id`
- `project_id`
- `backend_id`
- `backend_scope_kind`
- `backend_scope_id`
- `capability_slot`
- `access_mode`
- `capabilities`
- `root_policy`
- `priority`
- `status`
- `created_by_user_id`
- `updated_by_user_id`
- `created_at`
- `updated_at`

字段语义：

- `backend_id` 首版必填，scope 字段用于后续把授权扩展到 project/system managed backend 池。
- `access_mode` 首版可用 `use_existing_inventory`，后续治理任务可以扩展 `request_expansion`。
- `capabilities` 采用 JSON 或枚举数组，表达 browse、detect、execute、read、write、prepare 等能力。
- `root_policy` 首版默认 `all_reported_roots`，只允许 backend 已上报的 accessible roots。精细 root/workspace allowlist 只做 schema 预留。
- `priority` 参与自动 binding priority 和 runtime resolution。
- `status` 至少包含 `active` / `paused` / `revoked`。

### BackendWorkspaceInventory

新增 cloud 持久化快照，表达某 backend 最近一次发现的可用 workspace facts。

建议字段：

- `id`
- `backend_id`
- `root_ref`
- `identity_kind`
- `identity_payload`
- `detected_facts`
- `capabilities`
- `confidence`
- `status`
- `warnings`
- `source`
- `discovered_at`
- `last_verified_at`
- `created_at`
- `updated_at`

字段语义：

- `identity_payload` 使用现有 workspace identity contract 归一化结果。
- `detected_facts` 保留 local/backend 采集事实，不作为直接主键。
- `status` 至少包含 `available` / `stale` / `offline` / `error`。
- `source` 标记来自 `runtime_register`、`manual_refresh`、`scheduled_refresh` 或后续 `capability_expansion_ack`。

Inventory 不是 WorkspaceBinding。它是 backend 的能力事实快照；WorkspaceBinding 是某个 Project logical Workspace 选择使用这个事实后的业务链接。

### WorkspaceBinding

沿用现有模型，但绑定写入应从手填主路径改为匹配流程产物。

建议调整：

- 新增或复用 `priority`：来自 ProjectBackendAccess priority 与 inventory confidence 的组合。
- `detected_facts` 写入 inventory 的 facts 快照。
- `status` 由 sync 过程设置为 `ready`、`offline` 或 `error`。
- 可考虑在 metadata 或后续字段中记录 `inventory_id`，便于诊断与刷新。

## 数据流

### 1. Backend inventory refresh

入口：

- Project 设置页手动刷新。
- ProjectBackendAccess 创建或恢复时自动刷新。
- backend runtime register / reconnect 后触发后台刷新。

流程：

1. API 校验当前用户对 Project 有 edit 权限，且 ProjectBackendAccess 允许使用该 backend。
2. API 通过 RuntimeGateway Setup Action 调用 backend inventory refresh。
3. local backend 在 accessible roots 边界内扫描/探测 workspace facts。
4. cloud 归一化 identity payload，写入 `backend_workspace_inventory`。
5. 返回 refresh summary：新增、更新、错误、warnings。

首版可以复用现有 `workspace.detect` 的单 root 能力逐步过渡，但目标形态应是一个批量 inventory action，例如 `workspace.inventory_refresh`。

### 2. Project backend access sync

入口：

- 新增 ProjectBackendAccess 后。
- 用户在 Project 设置页点击“同步 workspace”。
- inventory refresh 完成后自动触发。

流程：

1. 读取 Project 下所有 logical Workspace。
2. 读取该 Project active ProjectBackendAccess 对应的 backend inventory。
3. 对每条 inventory 使用现有 `identity_payload_matches` / contract matcher 匹配 Workspace。
4. 对匹配已有 Workspace 的 inventory 自动 upsert WorkspaceBinding。
5. 对未匹配 inventory 生成 `WorkspaceCandidate` preview，不自动创建 Workspace。
6. 对多重匹配、contract 不确定、低 confidence 返回诊断。

### 3. Runtime resolution

RuntimeResolution 选择 binding 前必须额外校验：

- binding.backend_id 在 Project active ProjectBackendAccess 中。
- binding.root_ref 仍来自该 backend 已上报或已确认的 inventory / accessible roots。
- access capabilities 覆盖本次 session 需要的 read/write/execute/prepare 能力。

选择策略：

1. 如果 Workspace 有有效 default binding 且 access 允许，优先使用。
2. 否则选择 online backend 中 priority 最高的 binding。
3. priority 相同按 backend_id + binding_id 稳定排序。
4. 不使用最近使用时间作为默认选择依据，避免隐式状态改变自动化行为。

## API 草案

### ProjectBackendAccess

- `GET /api/projects/:project_id/backend-access`
- `POST /api/projects/:project_id/backend-access`
- `PATCH /api/projects/:project_id/backend-access/:access_id`
- `DELETE /api/projects/:project_id/backend-access/:access_id`

### Inventory

- `POST /api/projects/:project_id/backend-access/:access_id/inventory/refresh`
- `GET /api/projects/:project_id/backend-access/:access_id/inventory`

### Matching / sync

- `POST /api/projects/:project_id/workspaces/sync-backend-bindings`
- `GET /api/projects/:project_id/workspaces/candidates`
- `POST /api/projects/:project_id/workspaces/candidates/:candidate_id/create-workspace`

首版可以不持久化 candidate：`candidates` 可由 inventory + current Workspace 即时投影得到。若 UI 需要可复现审核流，再升级为持久化 candidate。

## 目标操作流程

用户对齐时优先使用操作流程，而不是先看字段和接口。目标态按“谁拥有决策权”拆成三条线：

### 1. Backend owner 管理自己的可用性

入口：`Settings → Backend / Local Runtime`，主语是 backend。

流程：

1. Backend owner 在本机 runtime 或 backend 设置中维护 accessible roots、能力槽、机器/镜像标签。
2. Backend 自己刷新 workspace inventory，cloud 保存该 backend 最近一次可用 workspace 快照。
3. Backend owner 在 backend 设置里选择“允许哪些 Project 使用这个 backend / capability slot”。
4. 授权后系统生成或更新 ProjectBackendAccess。
5. 如果 backend 在线，授权动作可以顺手触发 inventory refresh；离线时仅保留授权，等待下次上线刷新。

这里的核心不是 Project 去“拿” backend，而是 backend owner 把自己可用能力开放给某些 Project。

### 2. Project owner 消费 backend 授权

入口：`Project Settings → Workspace`，主语是 Project。

流程：

1. Project owner 看到“哪些 backend 已经授权给当前 Project 使用”，但默认不在这里新增 backend 授权。
2. Project Workspace 页面展示 logical Workspace 列表、matched bindings、unmatched candidates。
3. 已有 logical Workspace 能匹配 inventory 时，系统自动 upsert WorkspaceBinding。
4. 未匹配 inventory 只生成 candidate preview，由 Project owner 确认是否创建新的 logical Workspace。
5. Project owner 只维护业务语义：Workspace identity、默认 Workspace、runtime resolution preview。

Project 页面应该是“我能用什么、会解析到哪里、哪些候选需要确认”，不是“我去打开哪个 backend 的权限”。

### 3. Runtime 自动路由

入口：Task / Story / Project session 启动。

流程：

1. 运行时从 Task / Story / Project 找到 effective Workspace。
2. WorkspaceResolution 只考虑当前 Project 已被授权的 backend。
3. 优先使用 default binding；否则选择在线且 priority 最高的 binding。
4. 如果没有 ProjectBackendAccess、binding 不匹配 inventory、backend 离线或能力不足，返回明确诊断，不回退到第一个 Workspace 或第一个 backend。

### 4. 例外与高级入口

- 手动输入 `root_ref`、编辑 `identity_payload` JSON、手工调整 binding status 仍可保留，但放在 Workspace 高级编辑中。
- “同一 backend 只给某个 Project 用其中某个 root”暂不进入主流程，只作为 root policy backlog。
- server 侧批量扩展 backend accessible roots / worktree 创建治理由 `05-17-backend-capability-expansion-governance` 处理。

## 前端信息架构

目标态应拆成两处：

1. **Backend 设置页**：backend inventory、accessible roots、授权给哪些 Project、刷新入口、backend 级诊断。
2. **Project Workspace 页**：已授权 backend 摘要、logical Workspace、discovered candidates、runtime preview。

高级操作：

- 手动输入 `root_ref`
- 编辑 `identity_payload` JSON
- 手工修改 binding status

这些入口保留，但不作为主路径。

## 数据库与迁移

需要新增 migration：

- `project_backend_access`
- `backend_workspace_inventory`

同时更新新建库的 `CREATE TABLE IF NOT EXISTS` 初始化逻辑。

Repository 边界：

- 新增 `ProjectBackendAccessRepository` 或纳入 Project 相关 repository，但跨 Workspace sync 不应塞进普通 ProjectRepository。
- 新增 `BackendWorkspaceInventoryRepository`。
- 自动 upsert WorkspaceBinding 涉及 ProjectBackendAccess + Inventory + Workspace，建议放在 application 层 command service，必要时引入显式 command port，而不是把跨聚合事务伪装成单一 repository 方法。

## 关键权衡

### 持久化 inventory，而不是只实时查询

原因：

- offline backend 也要可诊断。
- Project 设置页需要稳定展示“上次发现了什么”。
- 自动匹配不应依赖所有 backend 同时在线。

代价：

- 需要 stale / refresh / last_verified_at 语义。
- 需要处理 inventory 与真实文件系统漂移。

### 自动 upsert binding，但不自动创建 Workspace

原因：

- 匹配已有 logical Workspace 是机械链接，减少重复配置。
- 创建新的 logical Workspace 是业务建模动作，仍应由用户确认。

代价：

- 首次接入 Project 时仍需要确认未匹配 candidates。

### backend 级授权为首版主路径

原因：

- root/workspace 级 allowlist 会显著增加 UI、策略和迁移复杂度。
- 当前用户判断该场景不常用。

代价：

- 无法精细表达同 backend 下某个 Project 只能用某个 root；作为 backlog 和 schema 预留。

## 风险与约束

- 目录浏览、detect、refresh、runtime execution 必须共享 ProjectBackendAccess 校验，否则会产生权限边界分叉。
- `browse_directory` 现有实现需要收敛到 accessible roots 边界；否则 UI 能看到超出 detect/runtime 的路径。
- 清理“default workspace 缺失时取第一个 workspace”的前端残留，避免 runtime preview 与实际执行语义不一致。
- 不修改已提交 migration，只新增递增 migration。
