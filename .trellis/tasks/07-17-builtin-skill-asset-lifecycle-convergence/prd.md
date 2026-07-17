# 收敛内嵌 Skill 资产生命周期与 Runtime 投影

## Goal

让平台内嵌 Skill 在每个 Project 的 Assets 中具有确定、可解释的存在状态，并让
AgentRun lifecycle 只负责选择和投影已经 provision 的 SkillAsset，消除 mount
投影、runtime binding 时序与项目资产写入之间的隐式耦合。

## Background

- 平台 builtin catalog 当前注册 `canvas-system`、`workspace-module-system`、
  `companion-system`、`routine-memory`、`memory-manager` 五个 embedded bundle。
- 项目 Assets 的 Skill 页面读取 project-scoped `skill_assets`，不会直接枚举
  embedded bundle。
- 当前 `AgentRunLifecycleSurfaceProjector` 在
  `BuiltinLifecycleSkillPolicy::EnsureAndProject` 下调用
  `SkillAssetService::bootstrap_builtins`，使 project asset 的创建依赖 AgentRun
  lifecycle 调用历史。
- 新 Agent Runtime 在 runtime binding 建立前构造 launch anchor frame；owner
  bootstrap 仍先按 runtime session 反查 binding，首次构造因此走无 builtin
  projection 的 fallback。后续 surface 只使用 `PreserveProjected`，无法补回资产
  或 mount metadata。
- `skill_asset_fs` 已是只读 provider，但 SkillAsset mutation API 仍允许修改或删除
  `builtin_seed`，与 embedded bundle 受管资产语义不一致。

## Requirements

- R1：embedded bundle registry 是平台 builtin Skill catalog 的唯一事实源；所有
  catalog 条目都必须被 provision 为每个 Project 的 `builtin_seed` SkillAsset。
- R2：新 Project 和 clone Project 创建完成时必须显式 provision 全量 builtin
  SkillAsset；服务启动时必须对所有既有 Project 执行同一幂等 reconciliation，
  并在失败时中止启动。
- R3：`AgentRunLifecycleSurfaceProjector` 必须是只读 projection，不得创建、更新或
  删除 SkillAsset；builtin policy 只表达本次 surface 要投影的 builtin keys。
- R4：ProjectAgent 首次 launch anchor frame 必须在 runtime binding 尚不存在时，
  直接使用 dispatch 已持有的 run、agent、frame 和 runtime thread 坐标生成
  lifecycle mount，不得通过 repository 反查正在构造的坐标。
- R5：AgentFrame identity 必须在 surface composition 前确定，并与最终持久化的
  frame ID 相同，使 lifecycle mount 不依赖“先持久化 frame 再反查”的循环。
- R6：projector 必须验证所有最终投影的 SkillAsset keys 已在目标 Project 中存在；
  缺失时返回明确 projection error，不得静默生成缺少 Skill 的 runtime surface。
- R7：`builtin_seed` SkillAsset 是 embedded catalog 的 project-scoped 受管投影，
  update、delete、upload overwrite、library install overwrite 与 publish 都必须
  拒绝把它当作用户资产处理；同步只能经过 provisioning service。
- R8：移除 `EnsureAndProject` 及 mount-time bootstrap 命名和实现，不保留旧行为或
  fallback。
- R9：`workspace_module_present` 成功后必须提交 typed
  `workspace_module_presented` control-plane presentation；前端必须立即按 concrete
  presentation URI 打开目标面板并展示成功事件，不得等待与 presentation intent
  无关的 Workspace runtime refresh。

## Acceptance Criteria

- [x] 新建 Project 后，未启动任何 Agent，Skill Assets API 已能列出 catalog 中全部
  五个 `builtin_seed` SkillAsset 及其完整 bundle 文件。
- [x] 服务启动 reconciliation 能为既有 Project 创建缺失 builtin、同步内容漂移，
  且重复执行不产生重复资产。
- [x] ProjectAgent 首次 `DispatchLaunchAnchor` 在不存在 runtime binding 的条件下
  生成包含 `companion-system`、`canvas-system`、`workspace-module-system` keys
  的唯一 lifecycle mount。
- [x] lifecycle projection 只读取并验证 Project SkillAsset；测试证明 projection
  期间没有 create/update/delete repository 调用。
- [x] 缺失任何声明的 builtin 或 explicit SkillAsset 时，projection 返回包含
  Project ID 与 key 的明确错误。
- [x] 普通 update/delete builtin SkillAsset 返回 conflict；user/imported
  SkillAsset 现有 mutation 行为保持正确。
- [x] `EnsureAndProject`、`BuiltinLifecycleSkillBootstrapper` 和 owner bootstrap
  的 runtime binding 反查入口从生产代码中删除。
- [x] 相关 Rust 单元/集成测试、格式化与定向检查通过，前端若调整 builtin 操作入口，
  对应 TypeScript 测试和 typecheck 通过。
- [x] `workspace_module_presented` 在 canonical AgentRun journal 中可见，前端将其
  渲染为成功事件；Canvas 打开计划为 immediate，不触发 AgentFrame/resource
  surface mutation 或阻塞式 refresh。

## Out of Scope

- 不改变 SkillAsset 数据库 schema 或 HTTP DTO；本任务无需新增 migration。
- 不引入平台级可编辑 builtin 副本或 builtin fork/copy 产品能力。
- 不改变 user、remote imported、marketplace installed SkillAsset 的来源模型。
