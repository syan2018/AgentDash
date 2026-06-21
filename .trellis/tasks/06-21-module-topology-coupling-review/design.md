# 项目主模块拓扑与耦合关系 review 设计

## Review Shape

本 task 是一个 review 编排父任务。主会话承担 coordinator 角色，负责维护任务 artifacts、调度 subagents、合并研究产物和输出后续 backlog。具体模块盘查由 subagents 完成。

Review 分三轮：

1. 第一轮：主模块拓扑盘查。
2. 第二轮：按第一轮发现的高风险耦合点做交叉深挖。
3. 第三轮：综合排序，生成后续 Trellis task 候选清单。

## Module Slices

第一轮按互不重叠的模块簇切分：

| Slice | Scope | Output |
| --- | --- | --- |
| 后端分层骨架 | crate 依赖、API/Application/Domain/Infrastructure/SPI/Contracts 边界 | `research/01-backend-layer-topology.md` |
| Workflow / Lifecycle / Task | graph definition、run/orchestration、subject execution projection | `research/02-workflow-lifecycle-task-topology.md` |
| Session / AgentRun / Runtime | RuntimeSession、Agent loop、mailbox、Backbone stream、chat control | `research/03-session-agentrun-runtime-topology.md` |
| Capability / Permission / Extension / VFS | capability frame、permission grant、runtime gateway、VFS/extension surfaces | `research/04-capability-permission-extension-vfs-topology.md` |
| Local / Relay / Desktop | cloud-local relay、local command handlers、desktop shell、workspace routing | `research/05-local-relay-desktop-topology.md` |
| Frontend / Contracts | app-web feature topology、generated DTO、streams、shared packages | `research/06-frontend-contracts-topology.md` |

## Output Schema

每个 subagent 产物必须按同一 schema 输出，方便后续自动合并：

```md
# <slice name>

## 模块清单

| Module | Responsibility | Main files |
| --- | --- | --- |

## 主链路拓扑

用编号链路描述入口、事实源、状态转换、投影、跨层输出。

## 耦合点

| Coupling | From | To | Relationship | Evidence | Risk |
| --- | --- | --- | --- | --- | --- |

## 下一轮深挖问题

| Priority | Question | Why now | Suggested reviewer scope |
| --- | --- | --- | --- |

## 已由既有 review 覆盖

列出不需要重复盘查的点，并引用已有 task 文件。
```

## Coupling Taxonomy

耦合关系按以下类型归类：

- 事实源耦合：两个模块都能写入或派生同一业务事实。
- 控制面耦合：多个入口能触发同一 runtime action 或 command。
- 契约耦合：DTO、事件流、generated contract、手写类型或 mapper 出现多处事实表达。
- 装配耦合：composition root、builder、service locator、runtime provider 持有跨域依赖。
- UI 状态耦合：前端 store、hook、view model、component props 对同一后端事实进行重复推断。
- 运行态耦合：Session、LifecycleRun、AgentFrame、RuntimeSession、Mailbox、VFS mount 等运行态坐标互相反查。

## Round 2 Trigger

第一轮结束后，主会话只做结构整合，不补写模块结论。第二轮 subagents 从第一轮报告中选取以下对象继续深挖：

- P0/P1 风险数量最多的耦合簇。
- 横跨三个以上模块的事实源或控制面。
- 与初版收尾稳定性直接相关的主链路。
- 与 generated contracts、RuntimeSession、LifecycleRun、PermissionGrant、RuntimeGateway、VFS mount 等权威事实源相关的分裂点。

## Final Deliverables

- `research/00-review-index.md`：列出所有 subagent 产物、覆盖范围和缺口。
- `research/coupling-matrix.md`：跨模块耦合矩阵。
- `research/followup-backlog.md`：后续 Trellis task 候选项，按 P0/P1/P2 排序。
- 必要时创建子 task，但只有当某个 follow-up 已具备独立验收条件时才创建。

## Coordination Rules

- subagent prompt 必须以 `Active task: .trellis/tasks/06-21-module-topology-coupling-review` 开头。
- subagent 必须明确“直接执行任务，不等待其他 subagent，不再 spawn subagent”。
- subagent 不修改业务代码，只能写入本 task 的 `research/` 文件。
- 主会话只整合已落盘研究结论，并保留来源文件路径。
