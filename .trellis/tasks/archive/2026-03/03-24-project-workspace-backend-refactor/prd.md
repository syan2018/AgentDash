# Project / Workspace / Backend 关系重构与配置体验收敛

## Goal

收敛 `Project`、`Workspace`、`Backend` 三者之间的领域关系与前端配置体验，使系统能够稳定表达并执行下面这条主链：

`Project -> Workspace(逻辑工作空间身份) -> WorkspaceBinding(后端物理绑定) -> RuntimeResolution -> AddressSpace`

同时保留高效的快捷操作入口，尤其是：

- 基于某个 `Backend` 直接浏览和选择目录
- 自动识别该目录对应的工作空间身份信息
- 用识别结果自动补全或创建 `Workspace` / `WorkspaceBinding`

目标不是把流程做得更复杂，而是把“正确的领域模型”和“高效率的操作入口”同时保留下来。

## Background

当前系统已经在运行时链路上开始向 `Workspace` 收口，但领域语义和前端信息架构仍明显滞后：

- `Project` 已基本不再直接依赖单个 `backend_id`
- 任务执行链已经会优先从 task / story / project 解析 `workspace_id`
- Address Space / mount 抽象已经具备正确方向

但与此同时，当前 `Workspace` 仍然更像“某个 backend 上某个目录的绑定记录”，而不是“具有 identify 的逻辑工作空间”：

- 仍要求显式选择 `backend_id`
- 仍要求手填 `container_ref` 或绝对路径
- `Project` 默认 workspace 缺失时仍有“取第一个 workspace”的错误兜底
- 前端项目详情页把领域对象、运行时派生物、权限与模板配置全部打平，导致配置体验混乱

这会让系统在概念上持续摇摆于：

- “Workspace 是逻辑工作空间身份”
- “Workspace 只是某个 backend 上的目录记录”

本任务的目标就是结束这种摇摆。

## Core Decisions

### D1: `Workspace` 必须升级为逻辑工作空间

`Workspace` 表达“这个项目依赖的工作空间是谁”，而不是“它现在挂在哪个 backend 的哪个目录上”。

建议核心字段：

- `id`
- `project_id`
- `name`
- `identity_kind`
- `identity_payload`
- `resolution_policy`
- `default_binding_strategy`

其中：

- `identity_kind` 示例：`git_repo`、`p4_workspace`、`local_dir`
- `identity_payload` 表达识别所需的最小信息
- `resolution_policy` 决定运行时如何从候选绑定中选出实际绑定

### D2: 引入 `WorkspaceBinding` 表达物理绑定

物理世界中的“这个 workspace 在某个 backend 上对应哪个目录”不应继续塞在 `Workspace` 本体上，而应成为独立对象。

建议核心字段：

- `id`
- `workspace_id`
- `backend_id`
- `root_ref`
- `binding_status`
- `detected_facts`
- `last_verified_at`
- `priority`

这样一个逻辑 `Workspace` 可以对应多个候选绑定，例如：

- 开发机上的 Git clone
- CI 节点上的只读副本
- 某个 P4 workspace

### D3: 运行时必须显式经过 `WorkspaceResolution`

`Project` / `Story` / `Task` 只持有逻辑 `workspace_id`。

实际执行前，再根据：

- 当前在线 backend
- binding 可用性
- resolution policy
- 运行上下文约束

解析出本次真正使用的 `WorkspaceBinding`，然后生成 `AddressSpace` 的主 mount。

### D4: 彻底移除“默认没配就取第一个 workspace”的兜底

这类兜底会把逻辑工作空间重新退化成“列表里碰巧排第一的物理目录”，属于错误语义，不应保留。

未配置默认 workspace 时，应显式返回未配置 / unresolved，而不是静默猜测。

### D5: 快捷路径要保留，但只能作为入口，不能反向污染领域模型

系统必须保留高效入口，例如：

- 在某个 backend 上直接浏览目录
- 直接选择一个 Git 仓库目录
- 直接选择一个 P4 workspace 根目录

但这些入口的语义应是：

1. 从 backend 和目录出发采集事实
2. 自动识别 identity kind / payload
3. 生成或匹配逻辑 `Workspace`
4. 自动创建或更新对应 `WorkspaceBinding`

也就是说：

- “backend 直选目录”是创建/绑定 workspace 的快捷入口
- 不是把 `Workspace` 再次定义回“backend + path”

## Domain Model Proposal

### 1. Workspace

建议职责：

- 表达项目依赖的逻辑工作空间
- 承载 identity 与 resolution policy
- 作为 Project / Story / Task 的引用目标

不再承载：

- 单个 backend 绑定
- 单个绝对路径
- 某类 VCS 的即时探测结果

### 2. WorkspaceBinding

建议职责：

- 表达逻辑 workspace 在某个 backend 上的一个可候选落点
- 缓存探测到的事实，例如 `.git`、远端 URL、P4 client name、workspace root 结构
- 保存最近校验状态，支撑运行时选择与诊断

### 3. WorkspaceResolution

建议职责：

- 基于 workspace + bindings + runtime environment 给出最终绑定结果
- 生成最终 `main` mount 所需的信息
- 对 unresolved / ambiguous 给出明确诊断原因

建议输出至少包含：

- `workspace_id`
- `selected_binding_id`
- `backend_id`
- `resolved_root_ref`
- `resolution_reason`
- `warnings`

## Frontend Information Architecture

当前问题的根源不是样式，而是信息架构错误。项目详情配置不应继续作为“全能抽屉”承载所有概念。

建议将项目配置重构为独立页面，例如 `/projects/:id/settings`，并拆为四个一级区域：

### 1. 概览

- 项目名称、描述
- 权限摘要
- 模板属性
- 共享状态

### 2. 执行默认

- default agent
- default logical workspace
- workflow assignments
- 其他运行默认项

### 3. 上下文资源

- context containers
- mount policy
- session composition
- 其他运行时资源视图

### 4. 工作空间

- 逻辑 workspace 列表
- 每个 workspace 的 identity 信息
- bindings 列表与校验状态
- runtime resolution 预览

### 关键前端原则

- `AddressSpace` 预览必须作为“派生结果”展示，而不是作为 workspace tab 下的一坨附属内容
- 页面必须明确区分“逻辑对象”“物理绑定”“运行时解析结果”
- `Backend` 状态与目录能力展示，应服务于 binding 创建和 resolution 诊断，而不是孤立展示

## Workspace Creation / Binding UX

### 主路径：以逻辑 Workspace 为中心

推荐主流程：

1. 选择工作空间类型：`Git` / `P4` / `Local`
2. 填写或确认 identify
3. 展示在线 backend 上的候选匹配结果
4. 选择默认 binding 策略
5. 完成创建

### 快捷路径：以 Backend 目录为中心

必须保留的快捷流程：

1. 先选择某个在线 backend
2. 浏览或输入目标目录
3. 系统自动探测：
   - 是否为 Git 仓库
   - remote / repo identity
   - 是否为 P4 workspace
   - 根目录特征和元数据
4. 系统尝试：
   - 匹配已有 logical workspace
   - 或直接生成一个新的 logical workspace 草案
5. 用户确认后保存

### 交互原则

- 自动识别结果应尽量减少手填
- 手动输入绝对路径应降级到“高级手动绑定”
- 如果自动识别不确定，应明确告知“不确定点”，而不是静默写入错误 identity
- 如果识别到和现有 workspace 冲突，应引导用户选择“合并到已有”或“创建新的”

## Backend / API Refactor Direction

### 后端目标

- `Project` 配置只指向逻辑 `workspace_id`
- `Task` / `Story` / `Project` 统一使用逻辑 workspace 进入运行时解析
- `AddressSpace` 构建消费 `WorkspaceResolution`，不直接偷读项目下的“第一个 workspace”

### API 方向

建议将接口语义拆清楚：

- `workspace` 相关 API：管理逻辑 workspace
- `workspace-binding` 相关 API：管理具体 backend 绑定
- `workspace-resolution` 相关 API：提供运行时解析预览
- `workspace-detection` 相关 API：支持从 backend + path 自动识别

其中 `workspace-detection` 与 `workspace-resolution` 都是“派生能力”，不应继续混在 `workspace CRUD` 里。

## Phased Plan

### Phase 1: 修正错误语义与兜底

- 删除“默认 workspace 未配置时取第一个”的所有兜底
- 对 unresolved 显式报错和返回诊断
- 梳理当前 project / story / task 三条链上的 workspace 选择逻辑

### Phase 2: 拆分逻辑 Workspace 与物理 Binding

- 重构领域模型与 DTO
- 后端 persistence 改为以逻辑 workspace 为中心
- 补齐 binding 状态与探测事实存储

### Phase 3: 引入 detection / resolution 服务

- 新增 backend + path 的识别服务
- 新增 runtime resolution 服务
- 让 Address Space 构建显式依赖 resolution 结果

### Phase 4: 重构前端项目配置页

- 拆出独立项目设置页
- 将 workspace 管理提升为一级区域
- 将 AddressSpace 预览改为 resolution 派生视图

### Phase 5: 补齐快捷创建与诊断体验

- 支持 backend 直选目录
- 自动识别 Git / P4 / local identity
- 支持“匹配已有 workspace / 新建 workspace”分支
- 展示 binding 健康状态与最终解析原因

## Acceptance Criteria

- [x] `Workspace` 在领域模型中已明确成为逻辑工作空间，而不是 backend+path 绑定记录
- [x] `WorkspaceBinding` 已成为独立对象，用于表达 backend 上的物理候选落点
- [x] 运行时执行链显式经过 `WorkspaceResolution`
- [x] 系统中不再存在“默认没配就取第一个 workspace”的静默兜底
- [x] 项目配置页能清晰区分逻辑 workspace、binding 和 runtime resolution
- [x] 保留“backend 直选目录”的快捷入口，但其结果会回收成 logical workspace + binding
- [x] 自动识别流程可对 Git / P4 / local dir 给出明确识别结果或不确定原因
- [x] Address Space 预览能解释“当前为什么解析到这个 backend/root”

## Out Of Scope

- 为旧 API / 旧数据库字段保留兼容层
- 继续在当前 project detail drawer 上做小修小补
- 先美化 UI 但不修正模型
- 把自动识别做成隐式黑箱且不给诊断

## Risks

### 风险 1：快捷入口重新把模型拉回 `backend + path`

控制方式：

- 明确区分“快捷入口”和“领域落点”
- 所有快捷入口最终都要回收成 workspace + binding

### 风险 2：前端先改页面，后端模型仍旧含混

控制方式：

- 必须先修正领域模型和 API 语义，再重做信息架构

### 风险 3：检测结果不稳定，导致误识别

控制方式：

- detection 结果保留置信度与诊断信息
- 有歧义时要求用户确认，而不是自动落库

## Related Tasks

- `03-10-extend-address-space-entries`
- `03-10-extend-source-resolvers`
- `03-23-agent-workflow-architecture-convergence`

## Related Areas

- `crates/agentdash-domain/src/project/`
- `crates/agentdash-domain/src/workspace/`
- `crates/agentdash-api/src/routes/projects.rs`
- `crates/agentdash-api/src/routes/workspaces.rs`
- `crates/agentdash-api/src/routes/address_spaces.rs`
- `crates/agentdash-api/src/routes/project_agents.rs`
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs`
- `frontend/src/features/project/`
- `frontend/src/features/workspace/`
- `frontend/src/stores/projectStore.ts`
- `frontend/src/stores/workspaceStore.ts`
- `frontend/src/types/index.ts`
