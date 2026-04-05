# Symphony Milestone: Stall Detection 超时守卫

## 目标

检测并处理 running session 的"静默挂死"——session 进程仍在但长时间无任何新事件输出。
这是平台级的安全网，不依赖 agent 判断。

## 核心行为

```
每次检查（tick 或独立 timer）:
  对每个 running session:
    elapsed = now - max(last_event_at, started_at)
    if elapsed > stall_timeout_ms:
      kill session → 触发 retry 或标记 failed
```

## 架构定位（设计对账 #2 结论）

> **Session 通用基建。覆盖所有 session 类型，不绑定 orchestrator tick 或 TurnMonitor。**

## 推荐实现

### Per-session stall timer（独立 timer）

在 session 创建时注册一个超时 timer。每次收到事件时 reset timer。
超时后通过 SessionHub 触发 cancel + retry。

**优点**:
- 覆盖所有 session 类型（task session + project agent session）
- 不依赖 TurnMonitor 或 orchestrator tick
- 与"TurnMonitor 迁移到 Hook 层"的方向不冲突

**实现位置**: `SessionHub` 内部，作为 session 生命周期的一部分。

## 现有基础

- `SessionHub` 有事件广播和订阅能力
- `TurnMonitor` 已有事件循环
- `RestartTracker` 已有 retry 策略
- 当前无任何 stall 检测逻辑

## 待讨论

- [ ] 选项 A / B / C？（倾向 C 作为最小实现，后续可扩展到 A）
- [ ] stall_timeout_ms 的合理默认值？Symphony 默认 300000(5min)，对 coding agent 是否合理？
- [ ] stall 后的处理：直接 kill+retry？还是先发一个 "are you still there?" 探测？
- [ ] 是否需要区分 "turn stall"（等待 agent 响应）和 "session stall"（整个 session 无活动）？

## 依赖

- symphony-orchestrator-config（stall_timeout_ms 配置）

## 参考

- Symphony spec §8.5 Part A (Stall Detection)
- Symphony spec §10.6 (Timeouts)
- `crates/agentdash-application/src/task/gateway/turn_monitor.rs`
