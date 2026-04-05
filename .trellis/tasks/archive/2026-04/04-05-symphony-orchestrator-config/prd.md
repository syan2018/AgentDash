# Symphony Milestone: Orchestrator 编排配置扩展

## 目标

为 Project Agent 的自主调度能力提供配置基础。不是在平台层硬编码一套 poll-dispatch daemon，而是让 Project Agent 能通过配置声明其调度策略偏好。

## 设计决策记录

### D1: 配置归属 — 已确认 Per-Project

Orchestrator 配置在 Project 层。只有 Project 层有相对完整的 Agent 伴随逻辑。

### D2: 核心配置字段（Draft）

需要区分"平台基础设施配置"和"Agent 行为偏好"：

**平台基础设施（由平台强制执行）**:
- `stall_timeout_ms`: session 无活动超时（平台级安全网）
- `max_turns_per_task`: 单 task 最大 turn 数（防失控）

**Agent 行为偏好（由 Project Agent 自行解释和执行）**:
- `poll_interval_ms`: Agent 被定时唤醒的间隔
- 其他策略字段待讨论（如自动 continuation 策略、优先级规则等）

### 待讨论

- [ ] 这些字段放在 `ProjectConfig` 里扩展，还是作为独立的 `OrchestratorConfig` 结构？
- [ ] Agent 行为偏好是否应该跟 Workflow/Lifecycle 绑定而不是 ProjectConfig？
- [ ] 是否需要 per-story override 能力？

## 依赖

无前置依赖。本 task 是整个 milestone 的配置基石。

## 参考

- Symphony spec §5.3 (Front Matter Schema)
- Symphony spec §6.1 (Config Precedence)
- `crates/agentdash-domain/src/project/value_objects.rs` — 现有 `ProjectConfig`
- `crates/agentdash-application/src/task/restart_tracker.rs` — 现有 `RestartPolicy`
