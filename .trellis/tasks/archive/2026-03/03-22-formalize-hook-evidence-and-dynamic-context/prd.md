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
- [x] 结构化 evidence 写入与读取链路正式打通，不再依赖纯文本启发式作为主判据。
- [x] hook runtime API、trace、diagnostics 与前端 UI 展示保持一致。
- [x] 至少一条真实 session 验证完整经过 continue -> stop -> phase advance -> record。
- [x] companion / subagent / approval / dynamic context 的关键状态都可在统一观测面中追踪。
- [x] 相关前后端测试补齐并通过。

## Technical Notes
- 这是一个跨层任务，必须遵守 `.trellis/spec/guides/cross-layer-thinking-guide.md`。
- 当前阶段先继续完善前端对 hook 事件流的呈现，不改变既有 hook 状态面板所在位置。
- 需要避免把 builtin workflow 做成硬编码执行引擎，builtin workflow 应保持为数据驱动配置层。
- 2026-03-22 当前阶段已完成：
  - `hook_event` 已通过 `session_info_update` 正式进入 ACP 会话事件流。
  - 前端会话流已显示 `hook_event / turn_started / turn_completed / executor_session_bound`。
  - 已在真实 session `sess-1774148877320-151e00e5` 上验证 `user_prompt_submit / before_stop / session_terminal` 三类 hook_event 会落入 session jsonl，并在页面主事件流中可见。
  - `WorkflowRecordArtifactType` 已正式扩展 `checklist_evidence`，`report_workflow_artifact` tool 与 workflow run API 均已贯通该类型。
  - `checklist_passed` completion 现在以当前 phase 的 `default_artifact_type` 为正式 evidence 判据；Trellis builtin task/story/project workflow 的 `check` phase 已改为数据驱动声明 `default_artifact_type=checklist_evidence`、`default_artifact_title=检查证据`。
  - 已在真实 task `f753b253-1f2e-49e0-b877-0cc49be2b3b0` / run `25c70efc-80dc-4da3-b069-d8dc6cbb2c0e` 上验证：
    - `report_workflow_artifact` 实际写入 `artifact_type=checklist_evidence`
    - `before_stop` 先 `continue`，满足条件后再 `stop`
    - hook 自动推进 `check -> record`
    - 前端主事件流可见 `hook:before_tool:allow / hook:after_tool:refresh_requested / hook:before_stop:continue / hook:before_stop:stop / workflow_phase_advanced_by_hook`
  - 已通过 MCP + preview 复验 Workflow 面板中的结构化记录产物 type chip 正确显示 `checklist_evidence`。
  - approval 链路已在正式会话流中收口：
    - `ToolExecutionPendingApproval / ToolExecutionApprovalResolved` 已由 Pi Agent connector 映射为工具卡片状态
    - 前端工具卡片支持直接批准 / 拒绝，并对 `approval_state=rejected` 显示“已拒绝执行”
  - companion / subagent 链路已纳入统一观测面：
    - `companion_dispatch_registered / companion_result_available / companion_result_returned` 已进入主事件流
    - 前端系统事件卡片可直接展示 companion 生命周期节点
  - dynamic context / hook runtime 观测面已在会话页统一展示：
    - Hook Runtime 面板可见 `sources / policies / constraints / fragments / diagnostics / trace`
    - 主事件流可见 `hook_event`，因此运行态决策与静态 runtime snapshot 已形成闭环

## Final Validation Summary
- 2026-03-22 已重新核对当前实现与前端观测面：
  - approval：工具卡片可见 pending approval、approve / reject 操作与 rejected 终态
  - companion / subagent：主事件流可见注册、结果可用、结果回传三类系统事件
  - dynamic context：Hook Runtime 面板持续展示当前 session 生效的动态注入来源、约束与诊断
  - hook trace：主事件流与 Hook Runtime trace 面板能互相对照 trigger / decision / completion / diagnostics
- 当前任务定义内的目标已全部满足，后续若继续推进 record/archive/adoption 自动化、companion 结果采纳控制流等能力，应另立后续任务追踪，不再阻塞本任务结案。

## Follow-up Scope
- 继续推进 record / archive / adoption 自动化闭环。
- 在后续任务中补强 companion 结果采纳、hook 化 follow-up 与更细粒度的 workflow 约束编排。
