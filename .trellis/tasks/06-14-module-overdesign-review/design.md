# 模块过度设计重新评估设计

## Review 边界

本任务的直接交付物是架构清理评估，不修改产品代码。评估对象按运行链路切分，而不是按目录机械切分：

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
