# Backend 能力扩展治理设计

## Goal

设计 server 侧的 backend capability / accessible root 扩展治理模型，让高权限用户可以集中管理大量 backend 的可用能力，同时让普通用户和自动化流程能够通过受控请求扩展可用目录或 workspace 能力，而不需要远程登录每台本机 backend 修改 profile。

本任务关注“谁可以要求 backend 扩展能力、扩展如何被 local backend 接受/拒绝、server 如何审计和展示结果”。它与 `05-17-project-backend-workspace-routing-design` 分工如下：

- 自动路由任务消费 backend 已声明/已上报的能力，用于 ProjectBackendAccess、workspace inventory 和 WorkspaceBinding 自动匹配。
- 本任务定义这些 backend 能力如何从 server 侧被请求、批准、下发、确认、撤销和审计。

## Background

当前可访问根主要由 Tauri / Local Runtime profile 在本机启动前配置。server 能看到 runtime health 上报的 `accessible_roots`，但没有成体系的 server 侧能力扩展流程。

随着 ProjectBackendAccess、backend inventory、自动 workspace 路由和未来 worktree / prepare workspace 能力推进，单纯依赖“用户去每台机器本机设置页手动加目录”会变成瓶颈：

- 管理大量设备时，需要高权限用户批量扩展 backend 能力。
- 普通用户创建 worktree 或新的 workspace 时，希望流程尽量顺滑，但不能让 server 静默扩大任意本机文件访问面。
- 托管 backend 与个人本机 backend 的信任边界不同，必须在模型上显式区分。

## Confirmed Facts

- `BackendConfig` 已有 `visibility`、`share_scope_kind`、`share_scope_id`、`capability_slot` 等字段。
- `ensure_local_runtime` 当前只开放 user scope；project/system shared runtime scope 在 API 中被拒绝。
- Local Runtime profile 支持 `accessible_roots`，runtime 注册和 runtime health 都会上报这些路径。
- 现有设置页主要展示 backend 状态和可访问路径，没有 server 侧能力扩展申请、批准、下发和审计入口。
- `05-17-project-backend-workspace-routing-design` 已决定首版 ProjectBackendAccess 采用 backend 级授权 + capability/root policy 预留字段。

## Decisions

- 个人模式不拆分 `personal_interactive` / `personal_managed`。首版只保留 `personal`，避免过早引入“预授权 parent root”等细策略。
- server 不应直接静默修改个人本机任意可访问根；personal backend 是最终裁决者。
- backend server-control 信任模式应作为 `BackendConfig.control_mode` 一等字段落库，不只从 `visibility` / `share_scope_kind` 临时推导。首版取值为 `personal`、`project_managed`、`system_managed`。
- 能力扩展采用 **持久请求 + 在线 relay apply + backend ack** 的混合机制：server 先落库表达期望，在线 backend 可立即收到 apply command；最终生效状态只由 backend ack / reject 决定。
- personal backend 新增任意绝对路径默认需要本机确认。server 前端只展示请求与结果；确认入口在 Tauri Local Runtime 面板，后续可接系统通知。
- capability expansion request 支持 `expires_at` / lease 字段，但首版不强制所有请求都有 TTL。持久 policy 用手动撤销；worktree / prepare 类临时请求必须显式 TTL。
- worktree 创建、workspace prepare 不另起独立授权事实源；首版作为 `capability_expansion_request.source_kind` / `requested_resources` 的一种来源，后续 provision 模型可复用同一 ack 机制。

## Requirements

### R1. Backend 必须声明 server-control 信任模式

首版至少区分：

- `personal`：个人本机模式。server 可以代表同一 backend owner 发起能力扩展请求，但本机 backend 是最终裁决者。若操作仍位于现有 accessible roots / workspace binding 边界内，可直接执行；若要新增任意绝对路径为可访问根，应由本机确认或由本机侧明确配置接受策略。
- `project_managed`：Project 共享托管 backend。Project owner / 管理员可按 policy 扩展能力。
- `system_managed`：系统级托管 backend。server admin 可批量管理能力扩展，适合设备池/执行集群。

### R2. Server 侧需要能力扩展请求模型

设计能力扩展请求，至少表达：

- 请求发起者：server admin、project owner/editor、普通用户、自动化流程。
- 目标 backend / backend scope / capability slot。
- 目标能力：browse、detect、execute、read/write、prepare、workspace inventory source、未来 worktree root。
- 目标路径或资源范围。
- 请求原因与关联 project / workspace / task / workflow。
- 状态：pending、accepted、rejected、applied、expired、revoked、failed。

### R3. Backend 必须 ack 或拒绝 server 请求

能力扩展不能只在 server 上写 policy 后假定成功。local backend 需要回报：

- 是否接受该请求。
- 最终实际生效的 capability / accessible roots / inventory source。
- 拒绝或降级原因。
- 生效时间和过期/撤销信息。

### R4. 支持高权限集中管理设备能力

高权限用户应能在 server 设置面管理 `project_managed` / `system_managed` backend：

- 批量选择 backend 或 backend scope。
- 下发新增/移除能力扩展 policy。
- 查看每台 backend 的 ack 状态和失败原因。
- 触发 inventory refresh。

### R5. 支持普通用户受控扩展请求

普通用户或自动化流程可以发起能力扩展请求，例如：

- 创建 worktree 前请求 backend 准备一个可用工作目录。
- workspace prepare 发现当前 binding 不满足 identity contract，请求 backend 执行受控扩展或准备。

personal backend 默认需要本机接受新增任意绝对路径；project/system managed backend 可按 policy 自动接受。

### R6. 设置页需要 server 侧管理入口

现有 Tauri Local Runtime 设置页只能管理当前本机 profile。server 设置页还需要表达：

- 当前 backend control mode。
- server 侧 pending / accepted / rejected capability requests。
- backend ack 结果。
- 与 ProjectBackendAccess / workspace inventory 的关系。

### R7. 审计与撤销必须是一等能力

所有能力扩展都应记录：

- 操作者、来源、目标 backend、目标路径/能力、关联 project/workspace/task。
- policy 命中原因。
- backend ack / reject 结果。
- 撤销或过期时间。

撤销后 ProjectBackendAccess、workspace inventory、runtime resolution 不应继续使用失效能力。

## Acceptance Criteria

- [ ] 明确 backend server-control 信任模式及其字段归属。
- [ ] 明确 capability expansion request 的领域模型、状态机和 API 边界。
- [ ] 明确 server policy 与 backend ack 的关系，不把 server 期望误认为 backend 已生效能力。
- [ ] 明确 personal backend、project managed backend、system managed backend 的不同裁决规则。
- [ ] 明确高权限批量管理设备能力的设置页需求。
- [ ] 明确普通用户 / 自动化流程触发能力扩展请求的产品路径。
- [ ] 明确能力扩展如何影响 runtime health、backend inventory、ProjectBackendAccess 和 WorkspaceBinding。
- [ ] 明确审计、撤销、过期和失败诊断要求。
- [ ] 形成 `design.md`，包含数据模型、API 草案、relay/pull/ack 机制选择、UI 信息架构和主要风险。
- [ ] 形成 `implement.md`，拆分实现阶段、验证命令、风险文件和回滚点。

## Out Of Scope

- 不直接实现 ProjectBackendAccess 自动路由；该能力由 `05-17-project-backend-workspace-routing-design` 跟踪。
- 不把个人本机任意路径扩展做成 server 静默操作。
- 不在首版设计 root/workspace 级精细 allowlist 作为 Project 授权主路径。
- 不要求立即实现完整 worktree 生命周期，只定义未来接入能力扩展请求的边界。

## Open Questions

无阻塞问题。剩余实现细节进入 `design.md` / `implement.md`。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
