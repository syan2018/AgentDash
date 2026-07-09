# 手动触发上下文压缩

## Goal

为 AgentRun 的会话上下文状态提供一个手动压缩入口，让用户可以在上下文用量面板里主动维护当前会话的模型上下文映射。

入口需要覆盖两个状态：

- 运行中会话：不打断当前 active turn，记录一次 one-shot 手动压缩意图，并在下一轮进入 provider 前强制执行上下文压缩。
- 空闲会话：立即启动一次 compact-only turn，只执行上下文压缩、更新最新 projection/head/context frame，然后结束，不生成普通 assistant 回复。

## Requirements

- 手动压缩必须复用现有 AgentDash-owned compaction 链路：生成 summary、携带 `compacted_until_ref` / `first_kept_ref`、提交 `runtime_session_compactions` / projection head，并透传 `context_compacted` 事件给前端。
- “强制”只绕过 token 阈值，不绕过可压缩边界校验；如果没有合法 cut point，命令应给出 no-op 结果，而不是写入不完整 projection。
- AgentRun 外部调用方只能提交 `compact_context` command intent，不能传入或判断 running/idle mode；running/idle 状态与调度选择由 AgentRun 内部根据自身 execution state 维护。
- 运行中触发时，API 返回已排队结果；当前 turn 继续运行，下一轮 pre-provider 边界消费该请求。
- 空闲触发时，API 返回已启动 compact-only turn 结果；该 turn 不持久化 `UserInputSubmitted`，也不发起普通 provider assistant 响应。
- 手动请求需要有幂等命令语义：同一 `client_command_id` 重放时不能重复排队或重复启动压缩轮。
- 前端入口放在现有上下文用量浮层中，复用 `context_compacted` / compaction summary frame 驱动的 projection refresh。
- 压缩事件需要显式标注手动来源，至少包含 `trigger = "manual"` 与 `reason = "user_requested"`，便于 projection provenance 和 UI 诊断。
- 压缩 summary 必须作为后续模型的 continuation handoff，而不是普通回复；summary 生成 prompt 要求模型总结上下文且不要继续对话，安装后的 compact summary context 要让下一次正常 provider 请求从压缩后的上下文继续工作。
- 手动和自动压缩都应记录足够的 provenance 与 diagnostics：`trigger`、`reason`、`phase`、`strategy`、`implementation`、manual `request_id`、no-op / failure reason。
- 压缩后模型上下文恢复必须以 projection checkpoint 为准，再叠加 checkpoint 之后的事实事件；不能回放已被压缩覆盖的历史前缀。

## Acceptance Criteria

- [ ] AgentRun conversation commands 中出现 `compact_context` 命令，运行中与空闲会话均可用，启动中/取消中/缺失 runtime session 等状态不可用并给出 disabled code。
- [ ] `POST /agent-runs/{run_id}/agents/{agent_id}/runtime/context/compact` 接受 command-only 请求，并返回 command receipt 与 outcome。
- [ ] API、frontend、route handler 和非 AgentRun 业务服务不传递 running/idle mode；AgentRun 内部 fulfillment 根据自有 execution state 把 command intent 接受为下轮待消费、维护 turn，或拒绝。
- [ ] 运行中触发后不会新开 turn；下一轮 provider 请求前消费手动请求并提交 `context_compacted` projection。
- [ ] 空闲触发后会新开 compact-only turn；该 turn 只产生压缩相关事件与 terminal 事件，不产生普通 assistant 消息。
- [ ] 无合法可压缩区间时不会写 projection，命令结果和事件诊断能表达 `no_eligible_messages`。
- [ ] projection head、compaction segment、context frame、前端上下文浮层都会在压缩完成后显示最新状态。
- [ ] 压缩 summary prompt 覆盖当前目标、已完成工作、关键决策、约束、文件/工具状态、错误修复、待办和下一步，并明确禁止 summarizer 继续会话。
- [ ] `context_compacted` payload、compaction record 和 projection provenance 能区分 manual/auto、user_requested/token_pressure、pre_provider/standalone_compact_turn。
- [ ] 重复 `client_command_id` 不会重复消费或重复启动。
- [ ] Rust 单元/服务测试覆盖运行中排队、空闲 compact-only、no-op、幂等、非法状态、resume/fork checkpoint、压缩后 token usage refresh；前端测试覆盖 service path、面板按钮行为和 projection estimate 状态。

## Notes

- 当前自动压缩链路已经能提交完整 projection；本任务的重点不是重写压缩算法，而是补齐手动命令、状态分流和 compact-only 执行模式。
- 入口可以先只做上下文浮层按钮；斜杠指令后续可以复用同一个 API 和 command receipt 语义。
- 三份 reference 调研产物在 `research/` 下；综合评估见 `research/reference-synthesis.md`。
