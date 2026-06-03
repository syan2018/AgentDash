# 完善运行中 Steer 用户输入上下文

## Goal

让 Agent 页在会话运行中提交的 steer 输入成为协议层、持久化层、模型上下文投影和前端时间线共同认可的用户输入事实。前端应能精确影响后端运行中 turn，并准确展示会话当前是否可 steer、steer 是否已被后端接收，以及该输入属于运行中 steer 而不是新 turn prompt。

本任务同时收敛 session 控制面：AgentDash 的 session 交互协议应是 Codex app-server protocol 的标准化扩展。前端 HTTP API、workflow lifecycle control、relay command、本机 handler、connector bridge 和 Backbone session event 应共享 Codex `turn/start`、`turn/steer`、`turn/interrupt`、`ThreadItem::UserMessage` / `UserInput` 的语义边界；AgentDash 只补充 Codex 未覆盖的 runtime anchor、submission kind、来源与审计元数据。

## User Value

- 用户在运行中补充指令后，时间线立即基于后端事实展示该输入，不再等到 agent 返回消息后才间接刷新。
- steer 输入会进入 transcript / context projection / resume 基线，后续模型上下文和审计回看都能看到这条用户输入。
- 前端输入栏状态来自真实 runtime control surface：可发送 steer 时发送 steer，不可 steer 时展示准确原因。

## Confirmed Facts

- Codex app-server protocol 使用 `turn/steer`，并要求 `expected_turn_id` 匹配当前 active turn。
- Codex thread history 将同一 turn 内的普通 prompt 与 mid-turn steer 都表达为 `ThreadItem::UserMessage { content: Vec<UserInput> }`，依赖显式 turn boundary 保持归属。
- AgentDash `BackboneEvent` 已声明 payload 优先对齐 Codex app-server protocol；Codex 未覆盖的能力才走 AgentDash 扩展。
- 当前实现已经有运行中 steer 控制 API，但后端只投递到 connector，没有把 steer 输入写入 session event 事实源。
- 当前前端主要通过 `Platform(SessionMetaUpdate key="user_message_chunk")` 渲染用户输入；这是字符串 meta key，不是可复用的协议标注。
- 当前控制面分散在 `SessionRuntimeControlView`、`LifecycleAgentSteeringService`、`SessionControlService`、relay `command.steer`、local `handle_steer` 与 connector `steer_session`，尚未由同一份 Codex-aligned control contract 串起。
- `SessionRuntimeControlPlaneView` 当前已经表达 anchored/running/terminal 等状态，但前端输入栏仍可能展示“未连接到 Agent dispatch”这类和运行中 steer 事实不一致的文案。

## Requirements

- 后端接收运行中 steer 时必须以 active turn 为前置条件，继续沿用 Codex `turn/steer` 的 `expected_turn_id` 语义。
- steer 被后端成功接收后，必须写入一条协议级用户输入事件，携带 Codex app-server `UserInput` 内容，并标注 `submission_kind=steer`。
- 普通 prompt 与 steer 应共享同一种用户输入协议事件，只通过提交来源/语义字段区分，避免重复造一条 UI 私有事件。
- session start / steer / interrupt / cancel / approval 等控制入口必须使用 Codex app-server protocol 对齐的控制 DTO 或内部 command，不再让各层以 `ContentBlock`、自由 JSON 或私有状态字段重复表达同一个 turn control 语义。
- `SessionRuntimeControlView.actions` 必须从真实控制面能力派生，前端只消费该 view 决定按钮与输入栏状态，不再自行拼接 dispatch 连接推断。
- session transcript、context projection、resume / fork / rollback 的原始事件重建必须消费该协议事件。
- 前端 stream/feed 必须消费协议事件展示用户输入，并在 UI 上标注 steer 来源。
- 输入栏控制态必须以 lifecycle runtime state + session execution state + steering capability 为依据，不能因为缺少 dispatch 连接信息而错误阻塞正在运行的 steer。
- 失败的 steer 不得写成已接收用户输入事实；API 错误应保留为控制面错误。

## Acceptance Criteria

- [ ] 运行中提交 steer 后，后端在同一 active turn 下写入带 `submission_kind=steer` 的用户输入事件。
- [ ] 前端时间线无需等待 agent 后续消息，即可从 session stream / event page 显示该 steer 用户输入。
- [ ] UI 能区分普通用户 prompt 与运行中 steer 用户输入。
- [ ] context projection / projected transcript 包含 steer 输入，且 role 仍为 user。
- [ ] Codex connector 继续通过 `turn/steer` 投递，带 expected active turn id；非 steerable / turn mismatch 不落用户输入事件。
- [ ] relay / local / cloud 路径使用同一协议事件，不新增只服务单端的字符串 meta key。
- [ ] 浏览器 API、workflow application、relay、本机 handler、connector bridge 的 steer control 都经过同一个 Codex-aligned command shape，字段包括 session/thread id、expected active turn id、`Vec<UserInput>` 和 submission metadata。
- [ ] `SessionRuntimeControlView` 返回的 control plane 与 actions 足以解释前端输入栏状态，不再出现 running + steerable 时提示未连接 dispatch 的错误状态。
- [ ] 相关 TypeScript protocol 生成产物与前端 typecheck 保持同步。

## Out Of Scope

- 不引入兼容旧事件的迁移层；项目未上线，事件协议按正确形态收敛。
- 不新增数据库 migration；本任务是事件 payload / 协议 / 投影语义修正。
- 不改变非运行中 follow-up prompt 的排队策略。

## Planning Status

用户已明确要求清晰且彻底地把 Codex app-server protocol 切入控制面。当前仍处于 Trellis planning；完成本轮规划补强后，按 workflow gate 进入 `task.py start`。
