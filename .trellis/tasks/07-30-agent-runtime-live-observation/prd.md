# 收束 Agent Runtime 实时观测链路

## Goal

建立一条时间语义一致、可恢复且随会话长度近似线性扩展的 Agent Runtime 观测链路：

- 权威 snapshot 只负责初始、重连、lane 失效和显式刷新；
- live lane 只传递 owner 在真实发生点产生的控制状态变化与 canonical presentation；
- 前端按 update 增量归约，不为每个 token 重读或重放完整会话；
- Native、Codex、Remote 以及平台工具统一遵守 `started -> updated* -> completed(terminal)`；
- terminal、active turn、provider waiting 和工具状态只有一个明确的生命周期 owner。

用户应持续看到及时吐字、立即出现的工具开始态、真实过程更新和稳定终态；会话结束后不得再出现 Thinking 或运行中工具。

## Background

当前链路在每个 `AgentLiveEvent` 上调用一次 `CompleteAgentService::read`，重建完整 history，再把完整 observation 与单条 presentation 一起发送。前端随后再次合并、映射并从头归约整段 conversation。该结构同时造成：

- 长会话中 token 更新积压并成批刷新；
- 新 snapshot 与旧 ephemeral presentation 被放进同一 update，制造终态后重新进入 waiting 的时间倒置；
- broadcast consumer 自身变慢并触发 `Lagged`；
- Runtime/API 静默把 stream error 转换为 EOF；
- Native Core/Host callback 只有工具 request/final result，没有 progress 合同；
- 前端虽然能消费工具更新 fixture，真实 Native 生产链却不能产生这些事件；
- parity inventory 与 production composition 覆盖范围漂移。

相关证据和回归提交见 [research/current-chain-assessment.md](./research/current-chain-assessment.md)。

## Requirements

### R1. Snapshot 与 live lane 分离

- `AgentRuntimeView` 是一次完整 authoritative read 的结果。
- Complete Agent live 合同发布原子 batch；每个 batch 包含 source-local sequence、同一发生边界产生的可选轻量状态快照，以及有序 canonical presentations。
- live update 路径禁止调用 `CompleteAgentService::read`，禁止携带完整 conversation。
- durable commit suffix 必须作为一个 batch 发布，并与该 commit 折叠后的状态属于同一时间边界。
- ephemeral provider/message/reasoning/tool delta 只携带 presentation，不伪装成 authoritative state。
- Runtime 只附加 Product thread identity、验证 source，并无损转发 live batch。

### R2. 明确观测状态与 conversation 的边界

- 从完整 `AgentObservation` 中抽取不含 conversation 的共享状态值，覆盖 revision、context、lifecycle、execution、command availability、interactions、thread/source evidence。
- authoritative read 返回“状态 + durable conversation”。
- live durable batch可携带在 owner commit 时直接投影的同一状态值；前端只在其 revision 不旧于当前值时原子替换。
- presentation 不能建立 active turn、receiving、command availability 或 interaction control 事实。

### R3. 原子 stream 协议与显式失效

- 浏览器流使用 generated typed frame，至少区分 baseline、update 与 reset-required。
- 建连与重连必须先建立订阅，再读取 baseline，避免 baseline/read 与 live attach 之间丢失 ephemeral 事件。
- sequence gap、broadcast lag、source mismatch、协议错误必须产生可观测的 reset-required 或 typed connection error；API 不得静默吞错。
- reset 时丢弃当前 ephemeral lane并重新读取 authority；不得尝试从 Runtime/Product 数据库恢复 ephemeral。
- serialization failure、lagged count、source 和最后成功 sequence 进入 diagnostics/logging。

### R4. 前端增量投影

- `AgentRuntimeConnection` 是同一 target 唯一的 baseline/live owner。
- connection 分别暴露当前 authoritative/control state、baseline replacement 与有序 live batches；不得把每个 ephemeral record 累积回 authoritative conversation。
- `useSessionStream` 仅在 baseline replacement 时重建 reducer；普通 update 使用 functional state update 增量归约当前 records。
- presentation identity 用于去重/替换；同一 item identity 用于消息、reasoning 与工具 body 生命周期合并。
- 每条 update 的工作量与当前 batch 大小相关，不得随完整历史长度线性或平方增长。

### R5. Thinking 与 receiving 生命周期

- Composer receiving 只来自 Runtime execution state。
- Thinking 只可附着到当前 authoritative active turn；provider waiting presentation不能创建或复活 active turn。
- `active_turn = null`、active turn identity 变化或对应 terminal state到达时，立即清理旧 turn 的 waiting/reasoning streaming 状态。
- 同一 turn 的 terminal 是前端展示归约的吸收态；旧 attempt/waiting presentation 不得重新打开它。
- 多 provider round、retry 和 steering 必须按 turn + round/attempt identity 区分，不能用一个裸 turn map 覆盖全部阶段。

### R6. 工具执行事件流

- Core/Host/Tool Broker 的工具合同改为有类型的执行事件流，而不是只返回最终结果。
- 事件流至少表达 started、零到多个 typed progress、唯一 terminal result；terminal 前 EOF 是 typed failure/lost。
- before-tool deny、validation failure、cancel 和 handler error 也必须形成完整且唯一的 canonical item lifecycle。
- Native adapter 根据已接纳 owner projector，把工具事件投影为 `ItemStarted`、对应 family 的 update/delta 和带 typed terminal evidence 的 `ItemCompleted`。
- `fs_apply_patch` 在执行开始时立即展示完整拟议 patch；执行过程中按真实文件处理结果报告进度；最终 record携带完整实际结果。
- command/shell 输出、MCP progress、dynamic tool partial 和其他支持 update 的工具必须穿过同一合同。
- RuntimeWire 使用 callback correlation 传递 progress 和 terminal；Remote placement 不得把多事件工具调用压缩回单一 request/response。

### R7. Durable terminal 自身完成收敛

- durable `ItemCompleted` 必须携带完整 item body与 terminal evidence；durable `TurnCompleted` 必须携带完整 turn terminal。
- 正常 terminal live batch到达后，前端无需例行完整 read 才能结束 Thinking、receiving 或工具卡。
- authoritative read只用于初始、重连、gap/reset、显式 refresh 和 command response要求的收敛。
- durable live 与后续 baseline使用稳定 presentation identity去重；baseline replacement不得重放命令式 UI 副作用。

### R8. 全实现一致性

- Native、Codex、Remote Complete Agent 都必须满足相同的 snapshot/live batch合同和 canonical item lifecycle。
- generated Rust/TypeScript/RuntimeWire contract同步更新；删除被替代的旧 shape，不保留兼容分支。
- parity inventory 只能声明 production composition实际覆盖的能力；不存在的测试符号和 fixture-only“完整覆盖”声明必须收敛。
- specs必须更新为 snapshot/live 分离后的唯一合同，不能继续要求“每个 presentation update 同步 authoritative read”。

### R9. 性能与诊断约束

- 多个连续 text delta不得触发多个 Complete Agent authoritative read。
- update payload大小只与当前 batch和可选轻量状态相关，不随历史 conversation 大小增长。
- 前端普通 live update不得从空 state重放全部历史。
- lag/reset、parse failure、serialization failure均可定位到 target/source/connection epoch/sequence。

## Acceptance Criteria

- [ ] AC1：测试服务连续发布至少 100 个 text delta 时，authoritative read计数保持为零；浏览器按增量顺序观察全部 delta。
- [ ] AC2：长历史与短历史发送相同 batch时，Runtime update序列化体积不包含历史 conversation，处理路径不存在 history replay。
- [ ] AC3：构造“旧 waiting 已排队、owner 随后提交 terminal”的时序，前端最终 `active_turn = null`、`isReceiving = false`，且无 Thinking。
- [ ] AC4：sequence gap与 broadcast lag均产生 typed reset，前端丢弃 ephemeral lane并只执行一次 authoritative baseline恢复；服务端保留诊断。
- [ ] AC5：真实 Native `fs_apply_patch` 组合测试观察到同一 item identity 的 started、至少一个 progress/update、completed，并在 completed 前逐步更新卡片。
- [ ] AC6：真实 command tool组合测试在命令结束前持续显示 stdout/stderr delta，终态后卡片不再处于运行态。
- [ ] AC7：MCP 与 dynamic tool分别证明零个或多个 progress均能到达，terminal严格唯一。
- [ ] AC8：Remote/Wire loopback证明工具 progress在 callback response前有序到达，断线不会伪造 completed。
- [ ] AC9：正常 durable terminal batch无需额外 terminal refresh即可得到完整消息/工具/turn终态；重连 baseline渲染结果完全一致。
- [ ] AC10：前端连接测试证明普通 live batch只归约新增 records；baseline replacement才允许完整重建。
- [ ] AC11：真实 production tracer覆盖 input → waiting → text → tool start/update/complete → final assistant → turn terminal → reload，期间无未知工具、停滞刷新、幽灵 Thinking 或悬空 item。
- [ ] AC12：Runtime contract生成、TypeScript typecheck、相关 Rust tests、前端 tests、production composition tests通过，parity inventory与实际测试符号一致。
- [ ] AC13：源码门禁证明 live reconcile/update路径不调用 `CompleteAgentService::read`，API stream error不再使用无诊断的 `Err(_) => break`。

## Out of Scope

- 不增加 Runtime/Product durable journal、projection repository或 ephemeral replay存储。
- 不通过轮询、固定 debounce、React `flushSync` 或扩大 broadcast capacity掩盖链路问题。
- 不保留旧 Runtime update、旧 callback result-only 或旧前端 stream shape的兼容入口。
- 不改变 Product、Lifecycle、Agent source的持久化 owner边界。
- 不重做 Session 卡片视觉设计；只修正其数据和生命周期输入。
- 不为本任务引入数据库字段或 migration；若实施中发现必须持久化新事实，必须退回规划重新审查 owner边界。

## Key Decisions

- authoritative snapshot 与 process-local live lane是两个不同时间语义，使用不同合同。
- Complete Agent owner在状态转移发生点发布 batch；Runtime不通过后读 snapshot“补齐”旧事件。
- durable commit suffix与同 commit state原子发布，避免中间帧。
- 工具调用使用多事件执行流；progress不是日志，也不是前端猜测。
- terminal live batch足以收敛正常会话；read是恢复机制，不是每次终态的常规组成。
- frontend保留一个 connection owner和一个 Session reducer，不建立第二套 turn/item事实源。
- 本项目未上线，直接删除旧合同和漂移测试，不做兼容或回退方案。

## Risks and Deferred Items

- Runtime contract、Host callback、Wire 与前端 generated types同时变化，必须按可编译纵向切片实施，避免长期半迁移。
- Codex vendor原生 progress形态与 AgentDash-owned tool callback progress来源不同，但都必须在 adapter边界收敛为同一 canonical lifecycle。
- 高频 command output需要保持增量 payload有界；具体 bounded/truncation策略沿用既有工具 owner合同，本任务不重新定义输出保留上限。
- broadcast仍可保持有界且允许 lag；本任务要求显式 reset和恢复，不要求 ephemeral可靠持久化。
