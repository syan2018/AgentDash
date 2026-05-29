# 清理 capability_state_changed 冗余事件

## Goal

删除 `capability_state_changed` 这一冗余的 `SessionMetaUpdate` 事件类型。该事件与 `context_frame(kind: "capability_state_update")` 完全重叠，导致同一次能力状态变更在前端产生两张卡片。顺带清理 `"hook_trace"` 白名单死代码。

## Background

- `capability_state_changed` 是早期引入的通知型事件
- 后来引入了结构更丰富的 `context_frame(capability_state_update)`，包含完整的 delta sections
- 两者在 live apply 场景下同时发出，前端同时展示两张语义相同但样式不同的卡片
- pending 场景只发 `capability_state_changed` 不发 `context_frame`，需要补齐

## Requirements

### R1: 删除后端 `capability_state_changed` 发射链路

- 删除 `eventing.rs` 的 `emit_capability_state_changed` 方法
- 删除 `facade.rs` 的对应转发方法
- 删除 `runtime_context_transition.rs` 两处调用（L131 live / L211 pending）
- 删除 `launch/commit.rs` 的 `capability_events` 循环发射

### R2: pending 场景补发 context_frame

- `runtime_context_transition.rs:211` 原来只发 `capability_state_changed`（pending 模式）
- 改为在此处也发一个 `context_frame(kind: "capability_state_update", apply_mode: "applied_on_next_turn")` 以保留信息

### R3: 前端迁移 refresh 触发源

- `SessionPage.tsx:339` 的 `case "capability_state_changed"` 分支触发 `refreshSessionRuntimeContext()`
- 将此逻辑迁移到 `context_frame` 事件处理中（当 kind 为 `capability_state_update` 时触发 refresh）

### R4: 前端清除渲染残留

- 从 `systemEventVisibility.ts` 的 `VISIBLE_SYSTEM_EVENT_TYPES` 中移除 `"capability_state_changed"`
- 从 `SessionSystemEventCard.tsx` 中移除 `EVENT_TYPE_LABELS` / `EVENT_TYPE_DEFAULT_MESSAGES` 中的 `capability_state_changed` 条目
- 删除 `buildCapabilityStateDetailLines` 及其相关分支

### R5: 顺带清理 hook_trace 死代码

- 从 `VISIBLE_SYSTEM_EVENT_TYPES` 移除 `"hook_trace"`（永远不会被匹配到）
- 从 `EVENT_TYPE_LABELS` / `EVENT_TYPE_DEFAULT_MESSAGES` 移除 `"hook_trace"` 条目

### R6: 更新测试

- `hub/tests.rs` 中多处断言 `capability_state_changed` 事件存在 — 改为断言 `context_frame(capability_state_update)` 事件存在
- `canvas/tools.rs` 测试中的事件顺序断言 — 改为匹配 `context_frame`

## Acceptance Criteria

- [ ] 后端不再发出 key 为 `"capability_state_changed"` 的 SessionMetaUpdate 事件
- [ ] Live apply 场景：只产生一张 `context_frame(capability_state_update)` 卡片
- [ ] Pending 场景：产生一张带 `apply_mode: "applied_on_next_turn"` 的 context_frame 卡片
- [ ] 前端 `refreshSessionRuntimeContext()` 仍在能力状态变更时正确触发
- [ ] `"hook_trace"` 从可见白名单和 label 映射中移除
- [ ] `cargo test` 通过（含 hub/tests 和 canvas/tools 测试）
- [ ] 前端编译通过，无 TS 错误

## Constraints

- 不改变 `context_frame` 的渲染逻辑本身（ContextFrameStream 已经能正确展示 capability delta）
- `context_compacted` + `compaction_summary` 双发模式不在本次清理范围（非冗余）
