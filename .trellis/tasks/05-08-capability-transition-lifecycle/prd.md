# 收口能力流转通知与工具定义热更新

## 背景

在 `builtin_workflow_admin` 的 Plan → Apply 流转测试中，Plan 阶段的工具裁剪已经生效，
但运行时反馈仍暴露出三类链路问题：

- 能力状态更新通知只展示 tool path delta，没有展示新增可用工具的说明/参数摘要。
- `CapabilityChanged` hook 注入以第二条 notification 单独进入 Agent，对 Agent 来说表现为
  “先收到能力变化，再收到 workflow guidance”，注入节拍不稳定。
- pending/live capability transition 的 Agent 可见内容散落在即时 notification 与 hook
  注入之间，没有进入统一的 turn-boundary 队列。

这些问题的共同根因是 runtime context transition 没有成为唯一输出事务：
工具热更、状态事件、steering notification 与 hook evaluation 仍有分叉。

## 目标

- `CapabilityState` 仍作为唯一运行态能力状态容器。
- live phase transition 只向 Hook runtime notice 队列写入一条合并 runtime notice，内容包括：
  - capability / tool state delta；
  - 新增或重新暴露工具的工具说明与参数摘要；
  - `CapabilityChanged` hook 产生的 workflow guidance / context 注入。
- `capability_state_changed` 结构化事件仍独立持久化给前端展示，但不再造成第二条 Agent 注入。
- pending transition 也通过 runtime notice 队列在下一次 `transform_context` 边界统一消费。
- 初始 system prompt 和 provider request tools 都必须基于当前可用工具构建，不应携带被屏蔽工具的说明。

## 非目标

- 不做旧字段兼容或回退方案。
- 不重做 MCP host 的全局 tools/list 协议。若外部宿主无法按 session 动态裁剪 `<functions>`，
  本次需在项目内部 PiAgent/runtime 链路中把真实 request tools 和 prompt 说明收敛正确。
- 不扩展完整前端 capability editor。

## 验收标准

- live runtime context transition 不再对 Agent 发送即时 notification，而是在下一次
  `transform_context` 边界消费一条合并 runtime notice。
- 合并 runtime notice 中包含重新暴露工具的实际 agent-facing tool name、description 和参数摘要。
- `CapabilityChanged` hook trace 与 bundle turn_delta 仍正常记录。
- runtime notice 被消费一次后从队列移除，避免重复注入。
- 相关 Rust 单测覆盖以上行为。
- `cargo fmt` 与相关 crate 测试通过。
