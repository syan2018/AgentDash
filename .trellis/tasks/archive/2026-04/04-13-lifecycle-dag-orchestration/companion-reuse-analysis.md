# Companion 与 Lifecycle 的关系

> 结论：两者是独立机制，互不侵入。Companion 不需要为 lifecycle 做任何改动。

## 1. 边界划分

| | Lifecycle Agent Node | Companion Subagent |
|---|---|---|
| **触发者** | LifecycleOrchestrator 服务 | Agent session 内的 companion_request tool |
| **Session 创建** | Orchestrator 直接调 SessionHub | companion_request 内部创建 |
| **上下文注入** | 标准 session 创建流程（workflow contract + context_bindings） | Companion slice（从 parent snapshot 裁剪） |
| **前驱产出访问** | Lifecycle VFS locator | 从 parent snapshot 继承 |
| **结果回收** | Agent tool call 推进 node 状态 | companion_respond → pending action on parent |
| **身份标识** | SessionBinding label + LifecycleRun.node_state | CompanionSessionContext |

**唯一交集**：agent node session 内部通过 companion_request 派发 subagent。这是标准 companion 用法，和 lifecycle 编排无关。

## 2. Companion 不需要改什么

- CompanionSessionContext：不扩展 lifecycle 字段
- Slice mode：不新增 LifecycleAware 模式
- companion_respond：不增加 lifecycle artifact 回写
- Tool visibility：不为 lifecycle 做特殊处理

Lifecycle 身份信息在 SessionBinding 和 LifecycleRun.node_state 上，不污染 companion 数据结构。
