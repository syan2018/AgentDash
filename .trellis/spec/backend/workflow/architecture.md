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
- `workflow/value_objects.rs` 是可序列化 Workflow value types 的 facade；具体类型按 binding、contract、capability、mount directive、hook rule、ports、lifecycle definition、activity definition、run state 子模块组织。`workflow/validation.rs` 承载 definition、topology 与 activity lifecycle 校验。类型定义和校验分离，原因是持久化契约与规则演进有不同的变化节奏。

## Current Baseline

| 文档 | 当前职责 |
| --- | --- |
| `activity-lifecycle.md` | Activity executor、run startup、template install/update、drop-step migration 契约 |
| `lifecycle-edge.md` | DAG edge kind、校验、运行时推进规则 |
| `lifecycle-run-link.md` | LifecycleRunLink 关联层、Session 降级、run-oriented API 契约 |
| `../story-task-runtime.md` | Story / Task / Session / LifecycleRun 关系拓扑 |
| `../../frontend/workflow-activity-lifecycle.md` | 前端 Activity lifecycle 编辑与运行观察契约 |

## Module Boundaries

| 模块 | 当前职责 |
| --- | --- |
| `workflow/value_objects.rs` | public facade 与 value object 测试入口 |
| `workflow/value_objects/binding.rs` | Workflow binding scope 类型与 owner 映射 |
| `workflow/value_objects/contract.rs` | Workflow contract、session terminal state、effective session contract |
| `workflow/value_objects/capability.rs` | CapabilityConfig、tool capability path / directive / reduction |
| `workflow/value_objects/mount_directive.rs` | VFS mount capability directive wire types |
| `workflow/value_objects/hook_rule.rs` | Workflow hook trigger 与 rule spec |
| `workflow/value_objects/ports.rs` | input/output port、gate/context strategy、standalone fulfillment |
| `workflow/value_objects/lifecycle_def.rs` | Lifecycle node、edge 与 step definition |
| `workflow/value_objects/activity_def.rs` | Activity definition、executor、completion/iteration/join/transition policy |
| `workflow/value_objects/run_state.rs` | Activity / lifecycle runtime state value types |
| `workflow/validation.rs` | Workflow contract validation、Lifecycle DAG validation、Activity lifecycle transition/port/policy validation |

## Local Decisions

- 普通 ProjectAgent 单 workflow 默认创建一活动 lifecycle，原因是 freeform/session 与 workflow 过程应共享 Activity 运行模型。
- artifact edge 自动提供 flow dependency，原因是数据依赖本身已经表达执行顺序，重复 flow edge 会制造两套 dependency 事实。

## Contract Appendices

- [Activity Lifecycle Backend Contract](./activity-lifecycle.md)
- [Lifecycle Edge](./lifecycle-edge.md)
- [Story / Task Runtime](../story-task-runtime.md)
- [Activity Lifecycle Frontend Contract](../../frontend/workflow-activity-lifecycle.md)
