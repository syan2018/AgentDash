# 设计：Native 会话实时状态收口

## 1. 单一状态流

```text
Product command
  -> Complete Agent admission
  -> Accepted receipt
  -> Agent-owned background execution
  -> durable history + ephemeral deltas
  -> canonical live feed
  -> Session reducer + typed side-effect planner
  -> authoritative snapshot convergence
```

Product 不保存第二份执行账本。运行态、active turn、commands、usage 与 terminal 均由 concrete Agent snapshot/history 证明。

## 2. Native 命令受理与执行

- 将 Dash Submit 分成“原子接纳”和“执行推进”两个阶段。
- 接纳阶段完成输入校验、effect 幂等检查、`InputAccepted + TurnStarted` 提交、active execution/cancellation 注册，然后返回 `Accepted`。
- 执行阶段由 Native owner 管理的 task 继续 provider/tool loop；terminal 后更新 Dash effect、Complete Agent effect 与 history。
- Steer/Interrupt 仍同步作用于 authoritative active turn，并快速返回其 command receipt。
- 后台 task 失败必须形成 typed terminal history/effect；禁止只写日志后遗留永久 active turn。
- `inspect` 对 Accepted/Applied 返回同一 effect 的真实当前状态，重复 client id 不启动第二次执行。

## 3. Durable usage

- provider completion 保留 `input_tokens`、`output_tokens` 与 immutable model `context_window`。
- `CoreEvent`/`DashCoreEvent::ProviderRoundCompleted` 传递本轮 usage。
- 新增 Agent history usage payload；folded state 计算 latest round 与 source cumulative totals。
- canonical projector 从该 durable entry 生成 `TokenUsageUpdated`：
  - `last.total_tokens = input + output`
  - `total` 为 source 累计
  - `context.current_context_tokens` 使用最新 provider round
  - `model_context_window/effective_context_window` 使用实际模型配置
  - source 为 `Provider`
- snapshot、changes 与 live callback 复用同一个 history projector，重连不丢 usage。

## 4. Workspace 命令收敛

- 保留 canonical `TurnStarted/TurnCompleted` 作为 Workspace execution refresh 边界。
- Native integration test 订阅 live 后执行 Submit，断言：
  1. HTTP/Complete Agent receipt 为 `Accepted`
  2. durable `TurnStarted` 到达
  3. concurrent read/observe 提供 active turn
  4. Product conversation snapshot 提供 enabled submit/steer 与 cancel
  5. Interrupt 后 durable terminal 到达并回到 ready
- 前端保留 committed conversation while refreshing，避免刷新窗口把 command state 清空。

## 5. Task owner side effect

- 扩展 `AgentRunControlPlaneEffectPlan`，加入精确的 Task plan refresh effect。
- 仅在 `item_completed` 且 item 为 completed、非 error 的 `task_write` 时规划该 effect。
- `useAgentRunWorkspaceControlPlane` 执行 effect 时调用当前 AgentRun 的 `fetchAgentRunTasks`。
- 不恢复 `SessionChatView` 内部第二次 raw-event 扫描。

## 6. 删除旧审计旁路

- 删除 `inspector` Tab descriptor、URI、图标、Context Inspector、旧 Hook audit cards 及布局测试假设。
- 从 Workspace runtime data/state 中移除固定为空、只服务旧审计展示的 `hookRuntime` 字段；“上下文”Tab 保留真实 runtime surface/ContextFrame 相关展示。
- 删除前端 `contextAudit` service。
- 删除 Application `context::audit` bus、AppState 实例以及 frame construction 的 `audit_bus` 参数/emit 调用。
- 不新增替代 API。ContextFrame/history 是唯一上下文接纳证据。

## 7. 竞争与恢复

- live durable overlay 保留到 authoritative snapshot 包含相同 presentation id。
- terminal 触发 authoritative reload；Accepted 不触发 terminal reload。
- 重连先 read baseline，再订阅/process live；usage 与 active turn 都可从 durable history 恢复。
- Task refresh 是可重复 owner read，不创建本地 optimistic Task 状态。

## 8. 历史上下文投影

- Product API 从 `ManagedRuntimeSnapshot.conversation_history` 生成 `SessionProjectionViewResponse`，不恢复旧 Session Runtime owner。
- durable `UserInputSubmitted`、终态 message/reasoning/tool item 与最新 `ContextFrameChanged` 共同组成当前投影。
- 最新 `ContextCompaction` 是消息边界：边界前历史仍保留审计事实，但不再计入模型当前上下文。
- Runtime snapshot revision 同时标识投影版本；投影是无状态读取，不形成第二份持久化账本。
- Composer 的 Stop 可见性只读取 committed conversation execution status；canonical feed 只负责触发 snapshot refresh。
