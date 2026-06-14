# Launch Frame 提交边界与 HookRuntime 刷新

Parent: `06-11-session-model-delivery-state-chain`

## Goal

修复 connector accepted 前推进 current frame 和 HookRuntime stale frame cache 两个后端状态错位点，让 AgentFrame 与 HookRuntime 都以 accepted AgentRun 状态为准。

## Dependencies

- 可与 API contract/receipt child 并行启动。
- 与 command receipt child 的交汇点是 accepted commit：frame commit 与 receipt accepted 应在同一 accepted 边界之后。

## Requirements

- 移除 `SessionLaunchOrchestrator` 中 connector start 前创建 AgentFrame revision/current_frame 的路径。
- prepare 阶段可构造 pending frame surface，但 current frame 只在 accepted commit 后更新。
- connector preparation/start failure 不改变 `LifecycleAgent.current_frame_id`。
- `ConnectorError::InvalidConfig` 失败不写入 user input/turn started/current frame。
- HookRuntime cache 命中后校验 current HookControlTarget；target 变化时 rebuild/replace。
- mismatch 错误保留给真实不一致，不作为正常 frame 切换路径。

## Acceptance Criteria

- [ ] connector start failure 测试断言 current frame id 保持原值。
- [ ] missing model selection InvalidConfig 测试断言 current frame id 保持原值。
- [ ] accepted turn 后 frame revision/current_frame 按预期更新。
- [ ] HookRuntime stale cached frame 被刷新为 current frame。
- [ ] 正常 frame 切换后不会抛 `Hook runtime target mismatch`。
