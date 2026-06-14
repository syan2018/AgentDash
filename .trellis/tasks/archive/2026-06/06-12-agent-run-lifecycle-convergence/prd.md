# 重建 AgentRun 会话生命周期与前端交互体系

## Goal

完整重新建模 AgentRun 会话体系，让 ProjectAgent draft start、首轮模型配置、后续消息、running steer、pending queue、cancel/terminal、workspace panel/resource browser 与 lifecycle mount 共享同一份后端权威状态。

用户价值是消除当前“前端猜状态、后端多入口各自判断、失败文案误导、资源 surface 分裂”的体验：进入会话时默认模型应被明确选中或明确要求用户选择；idle/ready/running/terminal 下的 Enter/Ctrl+Enter 行为应稳定；pending 队列只在真实需要用户处理时出现；workspace panel 应显示 Agent 当前真正可访问的 lifecycle/resource mount。

同等重要的用户价值是删除所有会动摇目标模型的误导路径。项目处于预研期，完成重构时不保留旧式 session-first command、半可用 launch、前端二次 command 推断、store 吞错、session_runtime resource 主路径等会让后续开发者误判架构所有权的入口。

## Confirmed Facts

- 真实启动失败链路不是单纯 ProjectAgent materialization 失败。`AgentRunWorkspacePage.tsx` 在发送前只校验 `executorConfig.executor` 非空；`executorConfig.provider_id/model_id` 可以为空。若前端传入只有 executor 的配置，后端 `merge_user_executor_config` 当前只补 system prompt，不补 preset 的 provider/model，ProjectAgent 默认模型会被覆盖丢失。
- ProjectAgent 创建/编辑表单当前允许“不指定模型”，`formToPreset` 对空 provider/model 不写入 config；这会让可直接运行的 ProjectAgent 缺少 effective model。
- `projectStore.createProjectAgentRun` 会 catch API 错误、写 store error、返回 `null`；页面收到 `null` 后抛出“创建 ProjectAgent AgentRun 失败。”，从而覆盖后端真实的“缺少模型选择”等 400 文案。
- `SessionChatView` 的 executor hydration 是组件本地状态：`agentDefaults`、frame execution profile、localStorage、executor hint 与 discovered options 分别参与。discovered `default_model` 目前不是一个会在发送前写回 command config 的权威事实。
- AgentRun 后续消息有多个入口：`/messages`、`/steering`、`/pending-messages`、`/pending-messages/{id}/promote`、`/pending-messages/resume`、`/cancel`。后端各入口分别检查 execution state；前端又用 `primaryAction/secondaryAction` 做一次命令分流。
- session runtime 内部有 `TurnState::Claimed` 与 `TurnState::Active` 的区别；AgentRun workspace 当前把它们压成 `SessionExecutionState::Running`，导致 pending enqueue 可在 running-without-active-turn 窗口通过，而 steer/promote 又因为缺 active turn 失败。
- “当前 AgentRun 上一轮已完成，请直接发送下一轮消息。”来自 pending enqueue 入口在非 running 状态下的 conflict 文案；它说明 UI 在某些 completed/idle 视图里仍尝试走 enqueue。
- Ctrl/Cmd+Enter 的 steer 分流在 `SessionChatView` 本地实现：当 `primaryAction.kind === "enqueue"` 且 secondary action enabled 时强行提交 steer。若 workspace snapshot 或 retained UI state 滞后，idle/ready 可能被发送成 steer，并携带过期 `delivery_trace_meta.last_turn_id`，触发 `expected_turn_id 不匹配`。
- “Pending 队列已暂停”由 `pending_queue.paused` 直接驱动，即使 `pending_messages` 为空也会展示。后端 pause state 与用户可见 pending work 当前没有分层表达。
- AgentRun workspace projection 会返回 frame runtime；但前端 `useAgentRunWorkspaceState` 又基于 `delivery_runtime_ref` 二次调用 `resolveVfsSurface({ source_type: "session_runtime" })`。workspace panel 当前消费的是 session_runtime surface 结果，而不是 AgentRun workspace 直接声明的 resource surface。
- 后端已有 `session_construction::resolve_session_frame_vfs` 从 runtime session anchor 找 current AgentFrame 并读取 `vfs_surface_json` 的路径；这证明 AgentFrame surface 是可作为前端资源事实源的。
- ProjectAgent explicit lifecycle 路径需要 ProjectAgent owner surface 与 lifecycle mount 同时存在；仅调整 composer classifier 不能保证前端 workspace panel 看见 lifecycle mount。
- 当前代码仍有多条容易误导后续实现的旧 mental model 路径：ProjectAgent `/launch` 只创建控制面但不发送首轮消息；`SessionRuntimeControlView` 与 AgentRun workspace actions 并存；前端 `SessionChatControlState.primaryAction/secondaryAction` 继续表达业务命令；`useSessionRuntimeState` 和 `useAgentRunWorkspaceState` 都能从 `session_runtime` 解析 VFS surface；`ProjectAgentLaunchResult`、`launchProjectAgent`、Session runtime control routes 等名字会让 RuntimeSession 看起来仍是会话控制面入口。

## Requirements

- 建立 `AgentConversationSnapshot` 或等价 contract，作为前端会话页唯一权威输入。它必须同时表达 lifecycle/control refs、execution state、active turn、allowed commands、model config、pending queue、resource surface 与 recovery/diagnostic reason。
- 建立 `ConversationCommandIntent` 或等价命令契约。前端发送的只能是后端 snapshot 已授权的 intent：`start_draft`、`send_next`、`enqueue`、`steer`、`promote_pending`、`resume_pending_queue`、`cancel`。键盘事件只能选择已授权 intent，不能自行解释业务状态。
- 建立模型配置解析契约：ProjectAgent preset、frame execution profile、用户显式 override、executor discovery default 的优先级必须在后端/contract 层可解释。ProjectAgent summary、draft AgentRun、runtime frame 都应暴露同形 `effective_executor_config`，发送前要么得到完整 `executor/provider/model`，要么进入明确的 `model_required` 状态。
- ProjectAgent 默认模型不能被“只有 executor 的用户配置”覆盖为空；用户 override 应按字段级合并，而不是把 preset config 整体替换掉。
- `AgentRunWorkspaceView` 需要扩展或升级为会话 snapshot：actions 不只给 enabled boolean，还要给 command intent、required runtime/turn preconditions、keyboard mapping、可展示文案与 stale guard。
- `pending_queue` 需要区分队列内部暂停事实、可见 pending 消息、用户可恢复动作、以及 session terminal 后的历史暂停状态。没有待处理消息且不需要用户处理时，不应展示 pending 提示框。
- resource surface 必须由 AgentRun workspace snapshot 直接投影，前端 workspace panel/resource browser 消费同一份 surface。Agent 能访问的 VFS/mount 与前端能看到的 mount 不能分裂。
- lifecycle mount 应成为 resource surface 中的一等 mount，ProjectAgent explicit lifecycle 与 Workflow AgentCall 都要有明确可测边界。
- 后端启动/消息/steer/pending/cancel 入口需要共享一个 state resolver，避免每个 route 分别从 RuntimeSession state、LifecycleAgent status、frame presence 推导命令能力。
- 前端 `SessionChatView` 应从 snapshot 的 command model 渲染 composer、按钮、keyboard hint、pending row 与 model selector，不再把 `control_plane.status + actions + local optimistic state` 拼成业务事实。
- 命令型 API store 不应吞掉真实错误；页面必须展示后端结构化错误或 snapshot diagnostic。
- 实现完成时必须做误导路径清算：所有与目标模型冲突的旧 DTO、route、service、store action、hook、test expectation、generated type、frontend adapter 和文案都要删除、改名为 trace/diagnostic，或收束到 `AgentConversationSnapshot` / `ConversationCommandIntent` / `resource_surface`。不允许留下会被新代码自然引用的“半可用旧入口”。

## Acceptance Criteria

- [ ] `design.md` 包含当前状态 Mermaid 图，覆盖模型选择、启动、send_next/enqueue/steer、pending paused、resource surface 五类入口。
- [ ] `design.md` 包含目标状态 Mermaid 图，明确会话 snapshot、command intent resolver、model config resolver、resource surface resolver 与前端消费边界。
- [ ] `design.md` 明确状态机：draft/model_required/ready/running/cancelling/terminal，以及 Enter/Ctrl+Enter 在每个状态的授权语义。
- [ ] `design.md` 明确 ProjectAgent 默认模型字段级合并规则，以及前端 selector 如何显示后端权威模型配置。
- [ ] `design.md` 明确 pending queue 展示规则：队列暂停、可见消息、可恢复动作、历史终止状态分别如何投影。
- [ ] `design.md` 明确 lifecycle mount/resource surface 的唯一事实源，解释为什么 workspace panel 不再从 session_runtime 二次猜 surface。
- [ ] `implement.md` 给出分阶段重构计划，每阶段可独立验证，且不依赖隐含父子任务上下文。
- [ ] `implement.md` 包含后端 application/API/contracts、前端 focused tests、resource surface tests、键盘交互 tests 与必要人工 smoke 验证。
- [ ] `research/current-state.md` 保存代码证据索引，覆盖每个入口的当前文件与断点。
- [ ] `design.md` 和 `implement.md` 包含误导路径清算规则，明确哪些旧路径必须删除、哪些只能保留为 trace/diagnostic、哪些必须转接到新 snapshot。
- [ ] 实现计划包含 grep/audit gate：旧 command inference、session-first control path、legacy ProjectAgent launch、session_runtime resource 主路径、store null error wrapper 等残留不得通过验收。
- [ ] `implement.jsonl` 和 `check.jsonl` 指向相关 spec 与 research 文件，供后续 Trellis sub-agent 使用。
- [ ] 任务保持 planning 状态，等待用户审阅后再进入实现。

## Out Of Scope

- 本规划任务不直接实施重构代码。
- 本规划任务不保留旧行为兼容层作为长期方案；实现阶段应删除误导路径，而不是给旧路径补兼容说明。
- 本规划任务不重新设计 connector protocol；connector 仍消费最终 resolved `AgentConfig`、VFS 与 turn command。
- 本规划任务不把 RuntimeSession 恢复成业务控制面根；RuntimeSession 继续是 trace/delivery substrate。

## Open Question

- `AgentRun` 是否继续作为前端 URL 和产品名？推荐保留：URL/public identity 仍用 AgentRun，内部和 contract 层引入 `AgentConversationSnapshot` 来承载会话状态。这样不会把用户可见模型再次切成 Session/Run 两套入口。
