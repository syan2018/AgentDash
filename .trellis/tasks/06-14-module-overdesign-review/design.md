# 模块过度设计重新评估设计

## Review 边界

本任务的第一阶段交付物是架构清理评估。评估对象按运行链路切分，而不是按目录机械切分：

- Lifecycle / Workflow / Task：控制面、运行状态、Activity/Run 事实源、workflow 启动链路。
- AgentRun / Session / Runtime Gateway：运行会话、workspace projection、mailbox/control、session feed 和 agent runtime delegate。
- VFS / Local / Relay / Extension：本机 runtime、relay 协议、VFS mount/tool composer、extension host 边界。
- Frontend / Contracts / Permission：前端 feature、生成契约、权限授权、跨层 DTO 和 runtime state 消费方式。

## 评估口径

每条问题按以下结构记录：

- 证据：真实文件路径和函数、类型、模块或状态字段。
- 问题类型：过度设计、模块过厚、重复事实源、抽象泄漏、跨层耦合、职责漂移。
- 影响面：受影响的用户流程、模块、数据流或未来维护风险。
- 建议边界：目标事实源、模块职责、可删除/合并的抽象或应下沉的 contract。
- 优先级：P0/P1/P2/P3，其中 P0 代表阻塞正确架构收敛，P1 代表高维护成本或高耦合风险。

## 协作方式

使用 Trellis research subagent 并行审查独立模块面。subagent 只读业务代码，并将发现写入本任务 `research/` 目录。主会话不等待其它 subagent 互相协作；主会话负责：

- 本地扫描模块规模、依赖和重复模式。
- 读取 subagent 结果并进行二次去重。
- 汇总成一份面向后续拆分的总评估。

## 输出形态

最终报告写入本任务目录下的 `overdesign-review.md`。报告不作为长期 spec；后续若某条结论形成稳定设计原则，再按 Trellis 规则更新 `.trellis/spec/`。

## 第一轮实现边界

第一轮实现不把所有 review 发现同时展开，而是只处理会造成事实源漂移、控制面竞争或用户可见错误状态的收束点：

- Lifecycle runtime truth source：cancel、Task projection、LifecycleRun status aggregation 回到 orchestration reducer / association / anchor / node 坐标。
- AgentRun control surface：workspace conversation/mailbox 成为 command/action 的唯一控制投影，RuntimeSession runtime-control 收窄为 trace/detail。
- Permission / contract capability surface：PermissionGrant 成为唯一授权事实源，pending grant query、typed permission DTO、capability catalog contract 先形成稳定跨层 contract。

本轮暂不处理：

- `RelayRuntimeToolProvider`、local `CommandHandler`、`vfs/mount.rs`、Tauri profile/claim 的装配层瘦身。
- `AgentRuntimeDelegate` 大规模 trait 拆分。第一轮只允许做必要硬化，避免和 AgentRun projection 收束交叉过大。
- 前端大组件全面重组。前端改动只服务于后端 contract / projection 收束后的消费路径。

## 并行收束方式

主会话作为本任务协调者，只维护任务文档、整合冲突、运行最终检查和提交。实现工作默认交给 `trellis-implement` subagent，并要求各 subagent 只处理自己工作流内的文件。

并行工作流之间的边界：

- Lifecycle 工作流可以修改 workflow/task/domain runtime 相关代码和 focused tests，但不改 AgentRun workspace API。
- AgentRun 工作流可以修改 workspace/query/conversation/runtime-control/frontend command consumption，但不改 PermissionGrant。
- Permission 工作流可以修改 permission contract/API/repository/frontend permission/capability catalog，但不改 Lifecycle runtime。

若 subagent 发现需要跨边界改动，只记录在任务产物或交回主会话，不自行扩大范围。
