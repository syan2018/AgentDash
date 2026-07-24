# 修复 Native 会话实时状态与审计链

## Goal

恢复 Native Agent 会话进行期间的单一、持续、可操作状态，使用户能够可靠看到运行态与上下文用量、发送 steer、停止轮次，并在 Agent 更新 Task 后立即看到状态栏变化；同时审计 `inspector://session` 的失效数据链，保留仍有产品价值且符合当前权威模型的能力，删除已经过时且没有权威数据源的设计。

## Background

- Native Complete Agent 在写入 `Accepted` 后仍同步等待整个 provider/tool 回合终态，导致 Composer 请求在回合期间一直 pending；前端 `isSending` 因此持续为真，Steer 在发起命令前就被本地拦截。
- Native provider bridge 已取得每轮 input/output token，但 `CoreEvent -> DashCoreEvent -> HistoryPayload -> BackboneEvent` 没有 usage 事实，页面永远收不到 `token_usage_updated`，重连也没有可恢复的 durable usage。
- 前端同时消费 Agent canonical feed 与 Product workspace snapshot。turn 边界已经进入统一 live-event planner，但缺少覆盖真实 Native active snapshot、命令刷新和 HTTP receipt 时序的生产级回归测试。
- `main` reference 曾在成功收到 `task_write` 的 `item_completed` 后主动刷新 Task 状态栏；当前通用 live-event planner 重构时删除了该逻辑，却没有把 Task owner 刷新加入新的 effect plan。
- `inspector://session` 仍轮询 `/runtime/context/audit`，但该 API 路由已删除。页面依赖的 Hook Runtime 数据在当前 workspace state 中固定为 `null`；后端 `InMemoryContextAuditBus` 仍被 ContextFrame 构造链写入，却已没有查询消费者。
- 项目未上线，不采用兼容层或回退路径；旧设计如果已被当前权威模型取代，应直接删除。

## Requirements

### R1 Native 会话实时状态

- Native Agent 的 turn started、terminal、token usage、工具过程事件必须通过同一 canonical live feed 到达当前 AgentRun 页面。
- 页面刷新、feed 重连和 snapshot hydration 后，运行态不得因旧 snapshot 覆盖 live 状态。
- 运行态结束后必须收敛到 Agent authoritative snapshot，不保留独立持久化运行账本。

### R2 steer 与 stop 命令

- Native Submit 必须在 Agent 原子接纳命令、写入 active turn 后立即返回 `Accepted` receipt；provider/tool 执行继续由 Agent owner 推进，不占用 Composer HTTP 请求。
- active turn 出现后，输入区必须展示并执行当前有效的 steer 与 cancel 命令。
- UI 运行态、键盘命令和 cancel receipt 必须来自同一聚合状态，不能出现“按钮显示但点击直接返回”的状态。
- 命令提交、终态和重连后必须重新读取权威命令视图。

### R3 Context usage

- Native provider 每轮确认的 input/output usage 与有效 model context window 必须成为 Agent-owned durable history 事实，并投影为 `token_usage_updated`。
- `last` 表示最新 provider round 的当前上下文，`total` 表示 source 累计 usage；不得用 UI 估算替代 provider 事实。
- Native Agent 发出的 token/context usage 更新必须实时进入 Session reducer，并驱动输入区上下文用量展示。
- snapshot hydration 与 live overlay 必须保持相同事件语义，重连后不得清空已确认 usage。

### R4 Task 状态栏

- 成功完成 `task_write` 后，当前 AgentRun 的 Task 状态栏必须立即重新读取 Task owner。
- 失败、进行中或其他工具事件不得触发错误刷新。
- 该行为应进入统一 live-event side-effect 规划，不恢复散落在 UI 组件中的专用扫描逻辑。

### R5 Session audit 存废

- 删除失效的 `inspector://session` Tab、Context Inspector、固定为空的 Hook Runtime 审计展示及相关布局注册。
- 删除无消费者的 `InMemoryContextAuditBus`、Context 构造注入参数和 producer；Agent-native `ContextFrame`/history 保持唯一上下文接纳与展示证据。
- 保留仍由当前 owner 提供真实数据的“上下文”Tab，不建立新的审计旁路。

### R6 回归覆盖

- 建立覆盖 Native Agent 生产事件到页面行为的跨层测试，而不只测试手工构造的 UI 状态。
- 覆盖 running、steer、stop、usage、task refresh、重连/刷新边界以及审计存废结果。

### R7 历史上下文投影与动作一致性

- `runtime/context/projection` 必须由当前 Managed Runtime 权威快照生成，不能依赖已删除的 Session Runtime 镜像。
- canonical conversation history 中的用户、助手、工具与上下文帧必须形成可读取的上下文构成；压缩边界之前的消息不得重复计入当前上下文。
- Stop 的可见性必须只由权威 conversation execution snapshot 决定，临时收流状态不能制造一个不可点击的 Stop。
- 投影 API 是当前产品合同；缺失路由必须显式报错，前端不能把 404 静默伪装成“暂无投影”。

## Acceptance Criteria

- [x] Native Agent active turn 期间，页面持续显示运行态，stop 可执行，Ctrl/Cmd+Enter 可 steer。
- [x] 普通 Submit 在 active turn 建立后返回 `Accepted`，不等待 provider terminal。
- [x] stop/steer 不依赖刷新页面，不出现可见按钮但缺少有效命令的情况。
- [x] token/context usage 在 provider round 完成时更新，包含有效 context window，刷新或重连后保持已确认值。
- [x] Agent 成功执行 `task_write` 后，状态栏在同一会话内更新，无需切换页面。
- [x] snapshot 与 live 竞争时，已提交 live 记录不会被旧 snapshot 回退。
- [x] `inspector://session`、旧 Context Audit bus 与 producer 完整删除；“上下文”Tab 继续只展示当前权威数据。
- [x] 相关前端定向测试、Native Complete Agent/AgentRun 集成测试、类型检查通过。
- [x] 历史上下文投影从 Managed Runtime canonical history 恢复，消息/工具明细不再为空。
- [x] Stop 只随权威运行态出现，不再因 feed 临时状态显示为灰色不可点。
- [x] 不修改或覆盖工作区中其他会话的成果。

## Out of Scope

- Codex Complete Agent 的 `live_events()` 接线。
- 为旧 runtime projection、旧 context audit 或旧 API 提供兼容与回退。
- 与本问题无关的会话 UI 重设计。
