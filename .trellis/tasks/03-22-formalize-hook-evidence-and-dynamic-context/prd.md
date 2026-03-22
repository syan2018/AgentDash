# 正式升级 Hook 结构化 Evidence 与动态上下文机制

## Goal
把当前阶段性的 hook stop gate、checklist evidence、动态上下文能力升级为正式、可追踪、可联调的产品级机制，并确保前后端与运行时的观测面一致。

## Requirements
- 建立结构化 phase note / checklist evidence 的正式写入与判定链路，替代 `last_assistant_text` 启发式。
- 建立 Hook Runtime -> Agent 外循环 -> 前端可观测面的完整链路，统一 stop gate、ask approval、companion、subagent、dynamic context 的事件模型。
- 为 Pi Agent 提供正式 hook surface，使 hook 信息提供与 agent 内循环解耦，但能在外循环及时影响 tool decision / continue / stop。
- 前端需要统一展示 hook 触发、状态推进、阻塞原因、completion 判定、approval / companion 状态，并保持设计语言一致。
- 通过真实联调验证 continue -> stop -> phase advance -> record 的完整流程。

## Acceptance Criteria
- [ ] 结构化 evidence 写入与读取链路正式打通，不再依赖纯文本启发式作为主判据。
- [x] hook runtime API、trace、diagnostics 与前端 UI 展示保持一致。
- [ ] 至少一条真实 session 验证完整经过 continue -> stop -> phase advance -> record。
- [ ] companion / subagent / approval / dynamic context 的关键状态都可在统一观测面中追踪。
- [x] 相关前后端测试补齐并通过。

## Technical Notes
- 这是一个跨层任务，必须遵守 `.trellis/spec/guides/cross-layer-thinking-guide.md`。
- 当前阶段先继续完善前端对 hook 事件流的呈现，不改变既有 hook 状态面板所在位置。
- 需要避免把 builtin workflow 做成硬编码执行引擎，builtin workflow 应保持为数据驱动配置层。
- 2026-03-22 当前阶段已完成：
  - `hook_event` 已通过 `session_info_update` 正式进入 ACP 会话事件流。
  - 前端会话流已显示 `hook_event / turn_started / turn_completed / executor_session_bound`。
  - 已在真实 session `sess-1774148877320-151e00e5` 上验证 `user_prompt_submit / before_stop / session_terminal` 三类 hook_event 会落入 session jsonl，并在页面主事件流中可见。
