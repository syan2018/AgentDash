# Companion 协作模型剩余升级项

## 背景

这条任务创建时假设 companion 仍“仅 Story owner 可用”。现在其中一大块已经完成：

- Project Agent 已开放 `companion_dispatch` / `companion_complete`
- `conditional_flow_tools()` 与 `FlowCapabilities` 已能按 owner kind 暴露 companion 工具
- Task execution session 仍保持“只能 complete，不能 dispatch”的边界

但真正还没做完的部分也很明确：

- `wait_for_completion=true` 仍被显式拒绝
- 没有 `await_companion(dispatch_id)` 之类的显式等待工具
- companion 继承上下文仍主要停留在当前 slice 模式，没有独立的“记忆继承”模型

## Goal

把这条任务收缩为 companion 的**剩余升级项**，不再重复追踪已经落地的 Project Agent 开放能力。

## 当前待完成能力

### 1. 同步等待模式

当前 `companion_dispatch` 只有异步注册模式。需要补齐：

- 打开 `wait_for_completion=true`
- 明确超时与失败回传语义
- 明确同步模式返回值结构

### 2. 显式 await 模式

如果继续支持“先 dispatch，后汇总”的协作方式，需要：

- `await_companion(dispatch_id)` 或等价工具
- dispatch 结果状态查询与重复等待语义
- 多个 companion 并发后的汇聚模型

### 3. 会话记忆继承

当前 companion 仍以 slice 当前上下文为主，需要继续定义：

- 从已有 session 提取何种“可继承记忆”
- 如何隔离 persona / system prompt 与参考上下文
- 是否允许跨 owner / 跨 agent 身份继承

## 非目标

- 不再把“Project Agent 开放 companion”作为主目标
- 不在本任务内改写 companion 的全部 UI 事件表现
- 不在本任务内重做整个 agent 调度框架

## Acceptance Criteria

- [ ] `wait_for_completion=true` 有明确实现或被替代为稳定同步模式
- [ ] 存在显式 await companion 的工具或等价机制
- [ ] companion 记忆继承有清晰的数据模型与注入边界
- [ ] Task execution session 仍保持 dispatch 边界不被误放开
