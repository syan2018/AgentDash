# Companion subagent 工具卡片与协议收束

## Goal

让 Companion 派发 subagent 在会话流里成为可理解、可跟踪、可跳转的产品对象，并收束工具结果里的坐标语义：Agent 可见闭环信息使用 child agent id 与 journal 访问坐标，执行器底层 delivery runtime session id 只保留在内部运行时事实源中。

## Background

当前 Companion 的 subagent 派发在前端没有被当成长期运行对象展示。用户只能看到普通工具卡片或 JSON 结果，无法从派发点直接观察 subagent 进度，也无法跳转到正在运行的 subagent。

代码证据：

- [CollabAgentCardBody.tsx](/D:/ABCTools_Dev/AgentDashboard/packages/app-web/src/features/session/ui/bodies/CollabAgentCardBody.tsx:10) 只展示工具、状态、prompt、model 和 `receiverThreadIds`，没有 AgentRun 跳转或进度投影。
- [CollabAgentCardBody.tsx](/D:/ABCTools_Dev/AgentDashboard/packages/app-web/src/features/session/ui/bodies/CollabAgentCardBody.tsx:35) 将 `receiverThreadIds` 直接作为“目标线程”展示，暴露了执行器协议字段而不是产品语义。
- [DynamicToolCallCardBody.tsx](/D:/ABCTools_Dev/AgentDashboard/packages/app-web/src/features/session/ui/bodies/DynamicToolCallCardBody.tsx:9) 没有 Companion/subagent 专用 renderer，未知 dynamic tool 走 GenericJsonBody。
- [threadItemKind.ts](/D:/ABCTools_Dev/AgentDashboard/packages/app-web/src/features/session/model/threadItemKind.ts:150) 将 `collabAgentToolCall` 归入普通 tool burst，弱化了 subagent 派发作为长期对象的可见性。
- [workflow-contracts.ts](/D:/ABCTools_Dev/AgentDashboard/packages/app-web/src/generated/workflow-contracts.ts:52) 已有 `AgentRunLineageRef`，包含 `run_id`、`agent_id`、`display_title`、`source`、`subagent_count`，可作为 UI 在当前 lifecycle run 下解析跳转和进度展示的事实源。
- [AgentRunWorkspacePage.tsx](/D:/ABCTools_Dev/AgentDashboard/packages/app-web/src/pages/AgentRunWorkspacePage.tsx:447) 已支持父 Run 跳转；[ActiveAgentRunList](/D:/ABCTools_Dev/AgentDashboard/packages/app-web/src/features/agent/active-agent-run-list.tsx:120) 已将 Companion 子 Agent 渲染为带状态的可展开树。
- [tools.rs](/D:/ABCTools_Dev/AgentDashboard/crates/agentdash-application/src/companion/tools.rs:1688) 的 `companion_request target=sub` 异步返回会向 Agent 可见文本和 details 暴露 `child_session_id` / `delivery_runtime_session_id`。

## Requirements

- R1: Companion 派发 subagent 的工具结果必须保留闭环信息：child `agent_id`、必要时的 `frame_id` / `gate_id`，以及访问该 agent 完整 journal 的产品级入口或 URI。`run_id` 属于当前 lifecycle/workspace 上下文，可由前端或 AgentRun projection 解析，不作为 Agent 可见闭环字段的必要组成。
- R2: `delivery_runtime_session_id` 属于内部 delivery/runtime 调度坐标，不能进入 Agent 可见工具 result 文本、普通 tool details、前端默认卡片或模型可消费的 Companion result payload。
- R3: 前端应为 Companion subagent dispatch 提供专用卡片，显示 subagent 标题、状态、最近活动、派发摘要、等待/超时/完成信息，并提供跳转到对应 AgentRun workspace 的操作。
- R4: subagent dispatch 卡片的进度事实源应来自当前 lifecycle run 下的 AgentRun workspace/list/lineage projection，而不是 executor thread id 或 delivery runtime session id。
- R5: 项目应定义 AgentDash 对 `collabAgentToolCall` 的标准拓展与自定义解析策略：兼容上游字段名和原始协议字段，同时把项目语义下的 thread receiver 解析为 AgentDash agent id，并在 UI 层结合当前 lifecycle/workspace 上下文得到跳转目标。
- R6: 原始 `threadId` / `receiverThreadIds` 可作为协议 raw 字段保留，但 AgentDash UI 和业务逻辑应优先使用解析后的产品字段；在 AgentDash 语义下，receiver thread 可被重定向解释为 agent id。
- R7: `companion_request target=sub` 和 Codex 原生 `collabAgentToolCall spawnAgent` 应收敛到同一种前端 presentation model，避免两套卡片和两套跳转逻辑。
- R8: 文档或 spec 更新应解释坐标分层原因：agent id / journal 是用户与 Agent 可闭环访问的业务坐标，lifecycle run 是当前会话/工作区上下文，delivery runtime session 是内部投递绑定坐标。

## Acceptance Criteria

- [x] `companion_request target=sub` 成功、等待完成、等待超时三类工具返回中，Agent 可见文本不包含 `delivery_runtime_session_id`、`child_session_id` 或等价 runtime session id 字段。
- [x] Companion subagent 工具返回 details 中包含 child `agent_id`、必要业务诊断 ref 和 journal access ref，且默认不暴露 delivery runtime session id。
- [x] 前端会话流能将 Companion subagent 派发渲染为专用卡片，展示状态与摘要，并能点击打开 child AgentRun workspace。
- [x] 专用卡片能从 AgentRun projection 更新运行状态；子 Agent 运行中、完成、失败至少各有一个可验证状态路径。
- [x] `receiverThreadIds` 不再作为“目标线程”在普通 UI 中裸露展示；调试视图如需显示，必须明确标注 raw protocol。
- [x] `collabAgentToolCall spawnAgent` 与 `dynamicToolCall companion_request target=sub` 共享解析策略或 presentation model。
- [x] 相关单元测试覆盖后端工具结果字段收敛、前端解析模型、卡片跳转目标、普通 tool burst 排除/特殊处理。
- [x] cross-layer/frontend/backend spec 至少更新一处，记录 AgentDash subagent 坐标分层与 collabAgentToolCall 解析规范的原因。

## Out Of Scope

- 不做历史兼容字段保留策略；项目未上线，按最正确的协议状态收束。
- 不新增独立 subagent 状态后端模型；优先复用现有 AgentRun lineage / workspace / list projection。
- 不把 delivery runtime session id 设计成任何用户或 Agent 可依赖入口。
