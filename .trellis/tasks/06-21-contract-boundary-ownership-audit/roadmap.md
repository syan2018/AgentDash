# 多模块重构 Roadmap

## 目标

本轮收敛 runtime/control/capability/contract 四组边界，让运行坐标、能力事实、控制命令与 wire contract 都有稳定 owner。父任务继续作为总索引；各模块只在自己的父任务目录下维护工作项。

## 当前状态

| 簇 | 父任务 | 状态 | 当前结论 |
| --- | --- | --- | --- |
| Runtime Failure / Placement | `06-21-runtime-failure-placement-convergence` | completed | backend disconnect、session MCP fallback 与 standalone backend id 来源已按 lost/session-route-bound/显式 backend id 收束 |
| Runtime Coordinate | `06-21-runtime-coordinate-convergence` | completed | current delivery binding 与 `CurrentDelivery` selection 已落地；raw anchor ordering 只作为 history evidence |
| Control Surface | `06-21-control-surface-command-boundary` | completed | lifecycle create/continue、extension backend target、relay taxonomy、Terminal mount utility 与 command availability 边界已完成 |
| Capability / Exposure | `06-21-capability-exposure-fact-convergence` | implementation_ready | AgentFrame revision 是 exposure fact；AgentRun effective capability/admission 是最终能力读取入口；CE03/CE04 仍按顺序推进 |
| Contract Boundary | `06-21-contract-boundary-ownership-audit` | in_progress | CB04-A/C/E/F/G 已完成；CB04-B/D 等待 runtime/capability 上游事实稳定后再动 |

## 正在收口

| 项 | 归属簇 | 状态 | 说明 |
| --- | --- | --- | --- |
| RC API route resolver | Runtime Coordinate | completed | API command context 已改用 `DeliveryRuntimeSelectionService::CurrentDelivery`，缺失 current delivery 时与 workspace projection 一致返回 delivery missing |
| RC SubjectExecutionView history | Runtime Coordinate | completed | whole-run association 只纳入可证明 root agent；agent view 的 delivery runtime ref 从 `LifecycleAgent.current_delivery` 投影 |
| CS draft command cleanup | Control Surface | completed | 后端 command availability 不再暴露 draft start；前端 draft start 是本地 action，真正创建仍走 ProjectAgent run create command |
| CE Grant/admission convergence | Capability / Exposure | completed | Grant 作为 AgentRun 授权系统投影为 final visible capability 或 admission decision；runtime hub 通过 AgentRun effective capability 边界获取执行用 tool surface |

## 后续顺序

1. 先完成当前 verifying 项的编译、contract 与 focused tests。
2. CE03 先实现 AgentRun capability service 到 AgentFrame revision 的 exposure recovery。
3. CE04 在 CE03 稳定后实现 WorkspaceModule visibility resolver。
4. CB04-B/D 在 Runtime Coordinate 与 Capability / Exposure 的最终读取入口稳定后再迁移。

## 冲突规避

| 并行区域 | 可并行原因 | 避免冲突方式 |
| --- | --- | --- |
| RC route/view cleanup | API route 与 lifecycle read model 文件边界清晰 | 不同时编辑 `delivery_runtime_selection.rs` 公共 API |
| CS draft command cleanup | contract/frontend command surface 与 RC/CE 低重叠 | contracts 生成由主会话统一执行 |
| CE Grant/admission | permission/capability/session hub 内聚 | 不并行改 workspace query、run view builder、conversation snapshot |
| CB04 DTO migration | contract owner 机械迁移 | 等上游 owner 稳定后按 CB04 子目录拆分执行 |
