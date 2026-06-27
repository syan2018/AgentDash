# 多模块重构 Roadmap

## 目标

本轮收敛 runtime/control/capability/contract 四组边界，让运行坐标、能力事实、控制命令与 wire contract 都有稳定 owner。父任务继续作为总索引；各模块只在自己的父任务目录下维护工作项。

## 当前状态

| 簇 | 父任务 | 状态 | 当前结论 |
| --- | --- | --- | --- |
| Runtime Failure / Placement | `06-21-runtime-failure-placement-convergence` | completed | backend disconnect、session MCP fallback 与 standalone backend id 来源已按 lost/session-route-bound/显式 backend id 收束 |
| Runtime Coordinate | `06-21-runtime-coordinate-convergence` | completed | current delivery binding 与 `CurrentDelivery` selection 已落地；raw anchor ordering 只作为 history evidence |
| Control Surface | `06-21-control-surface-command-boundary` | completed | lifecycle create/continue、extension backend target、relay taxonomy、Terminal mount utility 与 command availability 边界已完成 |
| Capability / Exposure | `06-21-capability-exposure-fact-convergence` | completed | AgentFrame revision 是 exposure fact；AgentRun effective capability/admission 是最终能力读取入口；CE02/CE03/CE04/CE05 已完成，RuntimeGateway channel parity 归属 CS07 |
| Contract Boundary | `06-21-contract-boundary-ownership-audit` | completed | CB04-A/B/C/D/E/F/G 已完成；application read model 与 API adapter DTO mapping 边界已覆盖本轮队列 |

## 正在收口

| 项 | 归属簇 | 状态 | 说明 |
| --- | --- | --- | --- |
| RC API route resolver | Runtime Coordinate | completed | API command context 已改用 `DeliveryRuntimeSelectionService::CurrentDelivery`，缺失 current delivery 时与 workspace projection 一致返回 delivery missing |
| RC SubjectExecutionView history | Runtime Coordinate | completed | whole-run association 只纳入可证明 root agent；agent view 的 delivery runtime ref 从 `LifecycleAgent.current_delivery` 投影 |
| CS draft command cleanup | Control Surface | completed | 后端 command availability 不再暴露 draft start；前端 draft start 是本地 action，真正创建仍走 ProjectAgent run create command |
| CE Grant/admission convergence | Capability / Exposure | completed | Grant 作为 AgentRun 授权系统投影为 final visible capability 或 admission decision；runtime hub 通过 AgentRun effective capability 边界获取执行用 tool surface |
| CE03 Canvas exposure recovery | Capability / Exposure | completed | Canvas expose 收束为先写 AgentFrame revision，再由 persisted frame adoption 刷新 live VFS / hook runtime / WorkspaceModule presentation；旧 direct live write path 已拆除 |
| CE04 WorkspaceModule visibility resolver | Capability / Exposure | completed | WorkspaceModule visibility 从 AgentRun effective capability view / latest AgentFrame fact 派生，tool-local capability bypass 已移除 |
| RC08 resource surface coordinate | Runtime Coordinate | completed | AgentRun workspace/resource surface DTO 补充 current frame VFS 与 launch anchor source/evidence coordinate |
| CB04-D capability catalog read model | Contract Boundary | completed | capability catalog 返回 application read model，由 workflow API adapter 映射 browser-facing contract DTO |
| CB04-B AgentRun workspace snapshot split | Contract Boundary | completed | AgentRun workspace/conversation snapshot 返回 application read model，lifecycle API adapter 负责映射 browser-facing contract DTO，并保留 resource surface coordinate 语义 |

## 后续顺序

1. 当前父任务工作项已完成；后续只保留跨模块回归和必要 spec 同步。
2. 新增 RuntimeGateway action/channel admission parity 时从 CS07 所属父任务继续推进。

## 冲突规避

| 并行区域 | 可并行原因 | 避免冲突方式 |
| --- | --- | --- |
| RC route/view cleanup | API route 与 lifecycle read model 文件边界清晰 | 不同时编辑 `delivery_runtime_selection.rs` 公共 API |
| CS draft command cleanup | contract/frontend command surface 与 RC/CE 低重叠 | contracts 生成由主会话统一执行 |
| CE Grant/admission | permission/capability/session hub 内聚 | 不并行改 workspace query、run view builder、conversation snapshot |
| CB04 DTO migration | contract owner 机械迁移 | 等上游 owner 稳定后按 CB04 子目录拆分执行 |
