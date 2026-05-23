# Workflow Architecture

## Role

Workflow 子系统表达业务过程定义、Activity lifecycle 运行态和状态推进规则。它把工作流执行收敛到 Activity / Executor / Attempt / Transition 模型，并通过事件驱动的 `LifecycleEngine` 维护 durable run state。

## Invariants

- Activity lifecycle 是 workflow 运行、编辑和观察的主模型。
- durable state advancement 只能通过 ActivityEvent 进入 `LifecycleEngine`。
- Scheduler 负责 durable claim 和 executor 启动；executor 只通过事件把结果交还给 engine。
- Function executor 即使立即完成，也必须产出 Activity terminal event，而不是直接修改 run state。
- Activity session binding 使用 `lifecycle_activity:{run_id}:{activity_key}#{attempt}` 定位当前 work。
- Lifecycle edge 只有 `flow` 和 `artifact` 两类；artifact edge 隐含 node-level flow dependency。
- 多 activity lifecycle 必须显式声明 edges；运行时不按数组顺序推断推进路径。

## Current Baseline

| 文档 | 当前职责 |
| --- | --- |
| `activity-lifecycle.md` | Activity executor、run startup、template install/update 契约 |
| `lifecycle-edge.md` | DAG edge kind、校验、运行时推进规则 |
| `../story-task-runtime.md` | Story / Task / Session / LifecycleRun 关系拓扑 |
| `../../frontend/workflow-activity-lifecycle.md` | 前端 Activity lifecycle 编辑与运行观察契约 |

## Local Decisions

- 普通 ProjectAgent 单 workflow 默认创建一活动 lifecycle，原因是 freeform/session 与 workflow 过程应共享 Activity 运行模型。
- artifact edge 自动提供 flow dependency，原因是数据依赖本身已经表达执行顺序，重复 flow edge 会制造两套 dependency 事实。

## Contract Appendices

- [Activity Lifecycle Backend Contract](./activity-lifecycle.md)
- [Lifecycle Edge](./lifecycle-edge.md)
- [Story / Task Runtime](../story-task-runtime.md)
- [Activity Lifecycle Frontend Contract](../../frontend/workflow-activity-lifecycle.md)
