# 取消后会话运行态模型收敛

## Goal

建立 AgentRun 工作台、RuntimeSession trace、平台 turn 和 connector live agent 之间的统一运行态事实源，使取消、打断、终态收口和下一轮发送共享同一状态机。用户在打断 Agent 会话后，工作台只在执行器确认为可复用时进入 ready；下一条消息、Ctrl+Enter、pending message 和 canvas / hook 触发路径都消费同一运行态投影。

该任务解决的故障族：

- 打断 Agent 会话后，第二条消息偶发 `Failed to fetch`。
- 打断后再次发送时，Pi Agent 报 `Agent is already processing a prompt. Use steer() or follow_up() to queue messages, or wait for completion.`。
- AgentRun 已经结束或不在运行中时，Ctrl+Enter 仍可能走 steer 并触发 `expected_turn_id` mismatch。
- 取消后工作台提前显示 ready，实际 connector / Agent loop 仍在收尾。
- AgentRun workspace command 通过 agent 最新 runtime 猜测 delivery target，当前 run / agent 与 delivery runtime 的绑定缺少精确收口。

## Confirmed Facts

- `SessionRuntimeService::cancel` 先 `request_cancel`，再调用 `connector.cancel`，然后向 turn processor 发送 interrupted terminal，随后立即返回。
- `SessionTurnProcessor` 持久化 terminal 后调用 `TurnSupervisor::clear_active_turn`，平台 `TurnState` 会变为 idle。
- `SessionCoreService::inspect_session_execution_state` 优先读取 `SessionRuntimeRegistry` 的 running snapshot；registry idle 后退回读取 `SessionMeta`。
- AgentRun Workspace 的 `send_next / enqueue / steer / cancel` action 由 `inspect_session_execution_state` 推导，未表达 connector closing / Pi Agent loop still busy。
- `PiAgentConnector` 复用 `agents` map 中的 `Agent`；下一轮 prompt 会进入 `Agent::prompt`，而 `Agent::prompt` 通过 `AgentState.is_streaming` 拒绝并发运行。
- `Agent::abort` 只取消 token；`is_streaming=false` 在 Agent loop spawned task 收尾阶段写回。
- `Agent::wait_for_idle` 当前采用先读状态再等待 notify 的形态，存在错过 idle notify 的风险。
- AgentRun context 当前通过 `execution_anchor_repo.latest_for_agent(agent.id)` 获取 delivery runtime session，而 workspace command identity 是 `run_id + agent_id`。

## Requirements

- 定义统一 Runtime Execution State，覆盖 `idle`、`claiming`、`running`、`cancelling`、`interrupted`、`completed`、`failed` 等用户命令需要区分的阶段。
- `TurnState`、cancel path、stream ingestion、terminal processor 和 connector live session 共同维护同一执行状态；取消请求到执行器 idle 确认之间必须保持可见的 closing / cancelling 状态。
- AgentRun Workspace action projection 只从统一运行态派生：
  - ready 状态允许 `send_next`。
  - running 状态按 active turn ref 允许 `steer` / `enqueue` / `cancel`。
  - cancelling / closing 状态禁用 `send_next` 和 `steer`，保留清晰状态原因。
- Pi Agent connector 的 cancel contract 需要表达执行器 idle / closed 边界，使平台状态收口晚于或同步于真实 Agent loop 收尾。
- `Agent::wait_for_idle` 和 Pi Agent lifecycle helper 使用不会丢通知的等待形态，作为取消收口的可靠 primitive。
- connector start failure、cancel terminal、stream terminal、manual interrupted recovery 都必须通过同一状态转移写入 terminal fact。
- AgentRun workspace command 解析 delivery runtime 时使用当前 `run_id + agent_id` 范围内的 anchor，不再用 agent 全局 latest 作为 command target。
- 前端 chat control、Ctrl+Enter、pending message 和 workspace panel 使用后端 action projection；客户端不自行根据过期运行态猜测 steer / send_next。
- 该任务可调整 Rust contract / DTO、数据库 migration 和前端 generated contract；项目处于预研期，目标是干净模型收敛。

## Acceptance Criteria

- [ ] 打断 Pi Agent 会话后，在执行器尚未 idle 前，AgentRun Workspace projection 返回 cancelling / closing 语义，`send_next` 与 `steer` 均不可用。
- [ ] 打断 Pi Agent 会话后，执行器确认 idle 且 terminal fact 持久化完成后，workspace 才返回 ready，并且下一条 `send_next` 成功进入新 turn。
- [ ] 打断后立即重复发送不会出现 `Agent is already processing a prompt` 暴露到用户工作台；若执行器仍在收口，后端返回结构化 command unavailable / conflict，而不是 native fetch failure。
- [ ] AgentRun 已经处于 ready / terminal / cancelling 状态时，Ctrl+Enter 不会调用 steer，也不会产生 `expected_turn_id` mismatch。
- [ ] `SessionCoreService::inspect_session_execution_state` 或其替代 projection 同时反映 platform turn、connector live session 和 terminal trace facts。
- [ ] `PiAgentConnector::cancel` 与 Agent loop idle 收口有回归测试，证明 cancel 后下一轮 prompt 不会撞上旧 `is_streaming`。
- [ ] AgentRun workspace command 通过当前 `run_id + agent_id` 定位 delivery runtime；存在多个 anchors 时不会选到其它 run 或旧 runtime。
- [ ] `send_next` / `cancel` / `steer` 的后端 route 测试覆盖 running、cancelling、ready 和 terminal 状态。
- [ ] 前端 chat control 测试覆盖 cancelling 状态、ready 状态 Ctrl+Enter、running 状态 Ctrl+Enter 的行为差异。
- [ ] 最终核验 gate 明确证明旧状态漂移被移除：代码中不存在让 workspace action 仅依赖 platform active turn 而忽略 connector closing / live busy 的路径。
- [ ] 最终核验 gate 明确证明目标架构完整迁移：AgentRun command、runtime-control projection、Pi connector cancel、frontend chat control 都消费统一运行态 contract。

## Out Of Scope

- Canvas workspace module 的展示体验优化。
- HookRuntime ownership 的新一轮业务扩展。
- LifecycleRun / LifecycleAgent / AgentFrame 的领域重建。
- RuntimeSession trace 历史页面的视觉重做。
