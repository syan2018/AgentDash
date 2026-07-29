# Research: Compaction、ContextFrame 与前端上下文可见性契约

- Query: 完整评估 Compaction 期间的前端交互状态、完成后的上下文刷新、压缩后模型输入的用户可见性，以及 ContextFrame 是否真实承载了压缩后的完整上下文契约。
- Scope: internal
- Date: 2026-07-28

## Findings

### 结论

当前实现已经解决了一个重要问题：Native Agent 在后续 provider round 中会重新读取最新持久状态，并把已接受压缩摘要 ContextFrame 的同一份 `rendered_text` 同时用于模型输入和公开时间线。它没有解决“用户看到的当前上下文等于 Agent 下一轮真实输入”这一更高层契约。

现状存在四个相互关联的契约断点：

1. Native 的真实模型输入由“压缩摘要 ContextFrame + retained conversation suffix + 当前 Surface/initial/append ContextFrame + provider tools”共同组成，但上下文投影 API 只按压缩事件位置截断公开历史，恰好会丢掉发生在压缩事件之前、却仍由 `retained_from` 保留给模型的消息。
2. 生产代码生成的压缩 ContextFrame 只有 `SystemNotice`，没有使用协议和前端已经支持的 `CompactionSummary` 结构，因此 retained boundary、token/message 统计、策略、触发源和 source range 没有随 frame 完整暴露。
3. 压缩失败和 lost 在 Native canonical projection 中也被投影成成功完成；前端进一步把所有 `contextCompaction` 固定解释为 completed，并始终显示“上下文已压缩”。上下文投影会把这个伪成功事件当成有效边界。
4. Codex 的 provider 内部压缩状态并不在当前 `thread/read` 契约中。Runtime 已诚实标记为 `AgentObserved/Observed`，但产品上下文投影丢掉 authority/fidelity，使 UI 无法区分 Native 的可精确重建输入与 Codex 的观察性公开历史。

因此，问题不应被定义为“把压缩后所有内容塞进一个超大的 ContextFrame”。正确抽象应是 Complete Agent 提供一个有 authority/fidelity 的有序 `ModelInputRecipe`（或 `AgentContextSnapshot`）：其中 ContextFrame 以完整值或引用出现，retained conversation 以明确范围/记录 ID 出现，工具以结构化 digest/定义出现，provider 私有 checkpoint 则以 opaque evidence 出现。压缩摘要本身仍应是一个完整、类型化的 `CompactionSummary` ContextFrame。

### 已有任务覆盖状态

| 既有任务 | 状态 | 已解决 | 尚未覆盖 / 与当前实现冲突 |
| --- | --- | --- | --- |
| `07-17-agent-runtime-compaction-state-protocol-review` | completed | 建立了 Complete Agent 所有权、compaction lifecycle、Runtime source/snapshot/change seam 的总体方向；已识别 Codex 只能观察公开 thread history，不能声称拥有 provider 内部精确 checkpoint。 | 没有形成产品侧“真实模型输入配方”的可查询契约，也没有解决前端 active lifecycle、retained suffix 与失败边界。 |
| `07-23-contextframe-input-authority-restoration` | in_progress；PRD 验收项已勾选，真实页面复验/归档仍未完成 | Native 每轮重新物化最新 context；已接受 compaction summary frame 的同一份 `rendered_text` 用于 provider input 与公开 frame；不再把 assistant/tool history 错当 ContextFrame。 | 把“summary 同源”误当成“完整上下文可见”。生产 summary frame 仍只有 `SystemNotice`；retained suffix 和 tools 不在产品投影；外部 Agent 精确度仍未传到 UI。真实页面验证仍缺失，见 `implement.md:87`。 |
| `07-24-native-session-live-state-audit` | in_progress | 恢复 session context projection 路由和 Native live state 更新；前端在 compaction summary frame 到达时可触发 projection reload。 | reload 后取得的 projection 本身仍不等于真实输入；失败/lost 没有 summary frame 且 lifecycle 语义错误；没有手工 `pnpm dev` 验收。 |

### 1. ContextFrame 本身已经有足够的压缩元数据类型，但生产 producer 没有使用

- `crates/agentdash-agent-protocol/src/backbone/context_frame.rs:5-8` 明确规定 ContextFrame 是 AgentDash 拥有的 model-context delivery，concrete Agent 消费 `rendered_text`，并把同一值发布到 history/live/audit。
- `crates/agentdash-agent-protocol/src/backbone/context_frame.rs:11-27` 的 frame 主体包含 frame ID、kind、source、apply/delivery metadata、`rendered_text` 和 typed sections。
- `crates/agentdash-agent-protocol/src/backbone/context_frame.rs:530-555` 已定义丰富的 `CompactionSummary` section，包括 summary、`tokens_before`、`messages_compacted`、`compaction_id`、projection version、strategy、trigger、phase、source sequence range、first-kept reference、compacted-until reference 和 timestamp。
- 但 `crates/agentdash-agent/src/dash/history.rs:224-268` 的 `accepted_compaction_summary_frame` 只创建一个 `ContextFrameSection::SystemNotice`。`retained_from`、source digest、token/message 统计、strategy、trigger、source range 均没有进入 frame。
- `crates/agentdash-agent/src/dash/store.rs:166-216` 会原子地持久化 Applied 与 Completed；`retained_from` 仅存在于 `HistoryPayload::Compaction::Applied`，而不在用户最终看到的 frame 结构中。

影响：

- 前端虽然能显示摘要文字，但不能从 frame 中解释“压掉了什么、保留从哪里开始、由什么触发、采用什么策略、对应哪个 source range”。
- frame 的 `rendered_text` 可以与模型输入同源，但 frame 的结构化证据并不完整；这正是“只给一个 Summary，其余保存内容对用户隐藏”的直接成因之一。
- 这不是协议能力缺失，而是 producer 与既有协议/renderer 脱节。

状态：**未覆盖，P1**。

### 2. Native 下一轮真实输入与产品 context projection 使用了不同的边界算法

Native provider input 的真实物化路径：

- `crates/agentdash-agent/src/dash/service.rs:2070-2112` 扫描 Applied/Completed lifecycle，选出最新成功完成的 compaction frame，并从 Applied 中的 `retained_from` 计算 conversation history 起点。
- `crates/agentdash-agent/src/dash/service.rs:2113-2191` 从 retained suffix 重建 InputAccepted、AgentOutput、ToolCall、ToolResult，再组合当前 Surface、initial context、completed compaction frame 与 append frames。
- `crates/agentdash-agent/src/dash/service.rs:2194-2226` 在每个 provider round 前重新读取最新持久状态和最新成功 compaction frame。
- `crates/agentdash-agent/src/dash/service.rs:2487-2501` 每轮重新物化 system prompt 与 provider tools。
- `crates/agentdash-agent/src/dash/service.rs:2505-2565` 按 ContextFrame 的精确 `rendered_text` 组装 accepted context。
- `crates/agentdash-integration-native-agent/src/bridge_execution.rs:172-236` 的生产 compactor 默认保留最后 8 条消息，生成 `retained_from`，并由 summary 与 retained reference 生成 source digest。

产品 context projection 的算法：

- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:53-67` 找最后一个 `ItemCompleted(ContextCompaction)`，并把它命名为 `active_compaction`。
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:79-90` 仅收集 compaction event 位置之后的 ContextFrame，并排除其之前的 records。
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:219` 把这个历史上的最后完成边界返回成 `active_compaction_id`。
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:267-295` 不会把 ContextCompaction 本身物化成 segment。
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:503-547` 的测试只构造了“old user -> completed compaction -> new user”，并断言 old user 被删除；没有构造实际 compactor 的“retained messages 位于 compaction lifecycle event 之前”的顺序。

这两个算法在正常生产数据上不等价：

```text
durable history:
  ... compacted messages
  retained message A
  retained message B
  Compaction Started
  Compaction Applied(summary frame, retained_from=A)
  Compaction Completed

Native provider input:
  summary frame + A + B + post-compaction messages

current product projection:
  summary category/evidence + post-compaction messages
```

所以“压缩完成后会刷新前端”即便成立，刷新得到的仍是错误上下文：A/B 仍真实发给模型，却被 UI 丢弃。

状态：**未覆盖，P1（核心 correctness 问题）**。

### 3. “完整上下文”不等于“单一 ContextFrame”

既有 07-23 任务正确恢复了 ContextFrame 的语义边界：

- ContextFrame 是 AgentDash 主动提供给 Agent 的 system/context input。
- 用户/assistant/tool conversation history 仍是 conversation records，不能为了统一展示而伪装成 ContextFrame。
- provider-native tool definition 也不是文本 ContextFrame；它必须保留结构化 schema。

因此用户需要的“压缩后的完整信息以 ContextFrame 形式完整暴露”，应拆成两层契约：

1. **压缩产物 frame**：一个类型完整的 `CompactionSummary` ContextFrame，携带 exact `rendered_text`、retained boundary、source digest/range、strategy、trigger、统计和 revision。
2. **当前模型输入配方**：有序列出所有活跃输入贡献，不把不同语义强行扁平化：
   - active ContextFrames：完整 frame 或可解析引用；
   - retained conversation range：canonical record IDs、起止 sequence、顺序和 membership；
   - structured provider tools：definition IDs/digest，必要时完整 schema；
   - provider private context/checkpoint：opaque evidence、authority/fidelity；
   - source revision、context revision、recipe digest 和 capture timestamp。

建议命名为 `AgentContextSnapshot` / `ModelInputRecipe`，由 Complete Agent seam 提供，Runtime 保持原样投影，Product 只负责面向用户分类，不再从 presentation event 的位置猜测模型输入。

状态：**架构契约缺失，P1**。

### 4. Native 可以通过重放恢复当前 frame 输入，但没有持久化一等的 active frame set / recipe

- `crates/agentdash-agent/src/dash/history.rs:64-72` 的 initial context installation 保存完整 `context_frames`。
- `crates/agentdash-agent/src/dash/history.rs:74-93` 的 DashSurface 保存其完整 `context_frames` 和 structured tools。
- `crates/agentdash-agent/src/dash/history.rs:531-548` 的 folded `AgentHistoryState` 保存当前 initial context、surface、active compaction 和各 compaction state，但没有 active context revision、ordered contributions、recipe digest 或独立 active frame set。
- `crates/agentdash-agent/src/dash/service.rs:2505-2565` 在需要 provider input 时临时把 stable surface frames、initial context frames、最新 compaction frame 和 append frames重新排序并拼接。
- `crates/agentdash-agent/src/dash/service.rs:2568-2592` 的 append frame set 也不是 folded state 字段，而是每次扫描全部 history：遇到 `SurfaceApplied` 收集 `SystemAppend`，遇到 `SurfaceRevoked` 清空。

所以当前 frame set **可由完整 Dash history 确定性重放恢复**，并非数据已经丢失；但它只存在于 provider materializer 的隐式算法中，没有一个可跨 Service API / Runtime / Product 投影的一等值。这解释了为什么 Agent 执行可以正确，而前端只能用 canonical timeline 另行猜测。

目标契约不必重复持久化另一份易漂移数据，但必须把同一 reducer/materializer 的输出作为带 revision/digest 的 query snapshot 暴露；Provider input 与用户 context popup 应消费同一 recipe 结果，而不是各自重建。

状态：**底层可恢复，跨层契约未覆盖，P1**。

### 5. 压缩失败和 lost 被伪装成成功，且会错误改变 UI 上下文边界

- `crates/agentdash-agent/src/dash/history.rs:185-205` 的 durable source 已能区分 Started、Applied、Completed、Failed(lost)。
- `crates/agentdash-agent/src/dash/history.rs:895-975` 的 fold 也正确维护 active/completed/failed/lost，并在 terminal 后清除 active state。
- 但 `crates/agentdash-integration-native-agent/src/canonical_projection.rs:173-207` 把 Started 投影为 `ItemStarted(ContextCompaction)`，把 Applied 投影为 deprecated `ExecutorContextCompacted` 加 `ContextFrameChanged`，而 Completed 与 Failed 都投影成同一个 `ItemCompleted(ContextCompaction)`。
- `packages/app-web/src/features/session/model/types.ts:629-630` 对所有 `contextCompaction` 固定返回 `"completed"`。
- `packages/app-web/src/features/session/ui/toolCardRegistry.ts:157-163` 最终也只允许 inProgress 或 completed。
- `packages/app-web/src/features/session/ui/bodies/ContextCompactionCardBody.tsx:7-11` 无论当前处于 Started、Completed、Failed 还是 Lost，正文都写“上下文已压缩，降低后续 token 用量”。

更严重的是，`session_context_projection.rs:53-67` 会把这个伪造的 `ItemCompleted` 当作上下文截断边界。因此一次失败/lost 不仅展示错误，还会使 UI 假装旧上下文已不再参与模型输入。

正确契约：

- terminal 事件必须携带 `succeeded | failed | lost` outcome，或者使用明确的 typed terminal variants。
- 只有 `Applied + succeeded Completed` 才能推进 context revision/active recipe。
- Failed/lost 只结束 pending operation，不得改变 active recipe。
- frontend gate、timeline card、context popup 必须从同一 lifecycle state 读取。

状态：**现有实现冲突，P1**。

### 6. 前端“压缩中”只覆盖 HTTP 请求，不覆盖真实 lifecycle

- `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:251-280` 的 `compactPending` 只来自本地 action state。
- `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:282-294` 在 command API 返回后立刻把状态变成“压缩请求已接受”，并马上触发 `onRefresh`。
- `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:471-509` 的 projection 只在 target 或 `refreshKey` 改变时重新拉取；它本身没有订阅 durable compaction lifecycle。
- Native Applied 阶段会发 compaction summary `ContextFrameChanged`，当前 session view model 可因此增加 refresh key，这使成功路径通常会再次刷新；但这是 frame 到达带来的间接效果，不是“Started/Applied/Completed/Failed/Lost 状态机”驱动。

因此：

- API accepted 后到真正 Applied/Completed 之间，按钮已经不再 pending。
- 其它需要冻结的输入/重试/二次 compaction 等交互没有统一 gate。
- reload/reconnect 后，本地 action state 丢失，无法从 durable active compaction 恢复“压缩中”。
- 失败/lost 没有可信 UI 终态。
- 首次 `onRefresh` 很可能发生在 context revision 尚未更新时；后续是否再刷新依赖具体 canonical event。

应由 Runtime snapshot/change 暴露 durable active operation，并由 session command availability 统一计算哪些交互在 Started 到 terminal 之间不可用。前端不应以 HTTP promise 生命周期代表 compaction 生命周期。

状态：**部分覆盖，P1**。

### 7. Context projection API 无法承载完整 frame、输入配方或精确度

- `crates/agentdash-contracts/src/runtime/session.rs:100-127` 的 segment provenance 只有 compaction ID/version/type/strategy/trigger/phase。
- `crates/agentdash-contracts/src/runtime/session.rs:131-162` 的 segment 只有 preview、token estimate 等展示字段，不包含完整内容或 ContextFrame。
- `crates/agentdash-contracts/src/runtime/session.rs:177-190` 的 context usage item 只有 kind、label、name、tokens、source、seq、turn，不包含 frame 的 `rendered_text`、sections 和 delivery metadata。
- `crates/agentdash-contracts/src/runtime/session.rs:228-255` 的 response 只有 projection fields、segments 和 usage；没有 source authority、semantic fidelity、active recipe、context revision、recipe digest 或完整 ContextFrames。

结果：

- canonical timeline 可以单独显示 frame，但 context popup 无法从自己的 API 获得完整 frame。
- popup 无法证明自己展示的是 Exact、Observed 还是 Estimated。
- `active_compaction_id` 只是投影器推断出的最后完成事件，不是当前正在运行的 operation，也不是 active recipe revision。
- 后端即使修复 refresh，API shape 仍不足以表达正确结果。

状态：**未覆盖，P1**。

### 8. Codex 只能提供 Observed 公开历史，UI 当前没有表达这个事实

- `crates/agentdash-integration-codex/src/complete_agent.rs:1070-1073` 的压缩命令直接调用 provider 的 `thread/compact/start`。
- `crates/agentdash-integration-codex/src/complete_agent.rs:1181-1220` 通过 `thread/read(includeTurns: true)` 读取公开 thread turns，并将它们 canonicalize 成 conversation history。
- `crates/agentdash-integration-codex/src/complete_agent.rs:1235-1244` 明确将 snapshot 标为 `AgentObserved`、`Observed`，并注明 App Server 没有稳定的 durable snapshot/context revision。
- `crates/agentdash-agent-service-api/src/snapshot.rs:99-108` 的 `AgentSnapshot` 只有 applied surface、initial context evidence 和 public conversation history，没有 provider private context/checkpoint。
- `crates/agentdash-agent-runtime-contract/src/managed_projection.rs:417-422` 已把 authority、fidelity 和 conversation history带到 Runtime snapshot。
- 但产品 SessionProjectionView response 没有这些字段，所以 UI 无法说明“这是观察到的公开 thread history，不是 provider 下一轮精确 input”。

不能用本地合成一个“完整 Codex ContextFrame”来补齐隐藏状态，那会伪造 provider authority。预研期正确做法是：

- Native：暴露 `Exact` recipe。
- Codex：暴露 `Observed` recipe，列出可观察 conversation/ContextFrame 与 opaque provider compaction evidence；精确内容不可用时明确标记 unavailable。
- 如果未来 provider API 提供稳定 context revision/checkpoint，再提升 fidelity；不要设置兼容 fallback 或用猜测填空。

状态：**底层已诚实表达，产品层丢失，P1**。

### 9. 现有测试大量验证了“可渲染的合成形状”，没有验证生产垂直链路

- `crates/agentdash-agent/tests/dash_history.rs:149-199` 检查 revision、summary 和 active cleared，但没有断言生产 frame 的 typed `CompactionSummary` section、retained boundary 或 provenance。
- `packages/app-web/src/features/session/ui/ContextFrameCard.test.tsx:169-193` 与 `:489-518` 使用手工构造的 rich compaction section，验证了前端 renderer 能显示 message/token/projection/source range；生产 producer 实际只发 `SystemNotice`。
- `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx:203-232` 已区分 `SystemNotice` 和 `CompactionSummary`；`:812-819` 的 SystemNotice 只显示 body，而 `:836` 之后的 CompactionSummary renderer 才显示丰富元数据。
- `packages/app-web/src/features/session/ui/contextFrame/ContextFrameStream.tsx:185-190` 也依赖 `compaction_summary` section 才能显示压缩消息数。
- `packages/app-web/src/features/session/ui/SessionProjectionView.test.tsx:28-43`、`:277-360` 使用手工 `summary_chunk`、strategy、phase 和 range；当前 Rust projector 对 ContextCompaction 返回 no segment，因此这个测试形状不是生产 API 实际生成的形状。

测试状态：**存在明显 contract drift，P2；它会掩盖上述 P1 问题**。

### 风险分级

| 优先级 | 问题 | 用户影响 |
| --- | --- | --- |
| P1 | projection 以 compaction event 位置截断，遗漏仍发送给 Native provider 的 retained suffix | 用户看到的“当前上下文”与模型真实输入不一致 |
| P1 | failed/lost 被投影为 successful completed，并错误推进 UI context boundary | UI 声称压缩成功、隐藏仍有效历史 |
| P1 | 缺少 AgentContextSnapshot / ModelInputRecipe 契约 | 无法端到端证明输入成员、顺序、revision 与 fidelity |
| P1 | 生产 summary frame 只用 SystemNotice | 压缩 provenance、边界和统计对用户隐藏 |
| P1 | 前端 pending 只等 HTTP response，不等 durable terminal | 压缩期间交互未可靠 gate，reload 后状态丢失 |
| P1 | 产品投影丢失 authority/fidelity | Codex observed history 被误解为 exact context |
| P2 | `active_compaction_id` 命名和语义错误 | API 消费者会把历史边界误当进行中 operation |
| P2 | 前端/投影测试使用生产端不会生成的合成数据 | 绿色测试不能证明垂直契约成立 |

没有发现直接导致 Native provider 使用错误上下文的 P0：Native durable state 与 provider materializer 当前能正确使用 retained suffix 和 completed summary。高风险集中在用户可见状态、控制门禁和“上下文视图的真实性”。

### 建议的目标契约

#### Complete Agent / Service API

新增权威查询值（名称可在实现设计时收敛）：

```text
AgentContextSnapshot {
  source
  source_revision?
  context_revision?
  captured_at_ms
  authority
  fidelity
  active_compaction_operation?
  last_compaction_outcome?
  recipe_digest?
  contributions: [
    ContextFrameContribution { frame }
    ConversationRangeContribution {
      record_ids / start_seq / end_seq
      records
      retained_by_compaction_id?
    }
    ProviderToolsContribution { definitions or digest }
    OpaqueProviderContextContribution { checkpoint/evidence }
  ]
}
```

关键 invariant：

- 贡献项顺序等于下一 provider round 的语义输入顺序。
- Native recipe 必须可由 provider capture 精确比对。
- Codex recipe 必须标为 Observed；opaque 状态不得伪装成文本。
- 只有成功 Applied/Completed 才生成新 context revision。
- frame `rendered_text` 是 Agent 输入与用户可见 frame 的同一值。

#### Compaction lifecycle

```text
Idle
  -> Started(operation_id, base_context_revision)
  -> Applied(new_context_revision, summary_frame, retained_range, digest)
  -> Completed

Started / Applied
  -> Failed(error)
  -> Lost(reason)
```

- Started 后 command availability 统一禁止不可并发的 context mutations。
- 用户输入是否延迟、拒绝或进入新 revision，必须成为明确策略并由 provider capture 验证。
- Failed/Lost 不改变 active recipe。
- reload/reconnect 从 snapshot 恢复 operation 状态，不能依赖页面本地 promise。

#### Product projection / frontend

- context popup 直接渲染 `AgentContextSnapshot`，不再从 canonical timeline event position 推导。
- 同时提供两个视图：
  - “当前模型上下文”：只显示 active recipe contributions；
  - “会话审计历史”：显示完整 canonical conversation，并标记哪些消息已被 compaction supersede、哪些仍 retained。
- 明确显示 `Exact / Observed`，而不是把 token estimate 或公开 turns 包装成权威真值。
- summary card 使用真实 typed `CompactionSummary` section。
- Started/Applied/Completed/Failed/Lost 共享一个 operation reducer；按钮、输入门禁、timeline 卡片和 popup refresh 从同一状态读取。

### 垂直验证矩阵

以下验证应比较 ID、revision、digest、顺序、fidelity 与实际 provider capture，不能只匹配摘要字符串。

| 场景 | Agent durable source | Provider round capture | Service API snapshot | Runtime snapshot/change | Product projection API | Frontend |
| --- | --- | --- | --- | --- | --- | --- |
| Native 手动压缩成功 | Started/Applied/Completed 顺序正确；Applied 带 rich frame、retained range、digest | 下一轮输入严格为 summary frame + retained suffix + post messages；tools 与 active frames 正确 | Exact recipe、context revision 与 source 一致 | operation terminal 后 recipe 原样收敛 | 不按 event position猜测；返回完整 frame/records | 压缩期间 gate；成功后自动显示新 Exact recipe |
| Native 自动 overflow A/B/C | B 轮生成一次 compaction；C 轮前持久 recipe 已完成 | C 的第一次请求即使用新 recipe，无旧 head 泄漏 | capture revision 与 snapshot revision 相同 | change 不丢 Applied/Completed | retained membership 可解释 | 无需手动刷新 |
| Native failure / lost | source 记录 terminal error，active recipe/revision 不变 | 后续 round 继续使用旧 recipe | outcome 正确，recipe 不变 | terminal 类型保持 failed/lost | 不产生成功边界 | 卡片显示失败/lost；输入恢复；旧上下文仍可见 |
| 压缩期间并发用户输入 | 输入被拒绝/延迟/归入新 revision 的策略确定 | capture 中该输入只出现一次且位置确定 | membership 可查询 | operation 与 input change 次序稳定 | UI 可解释输入归属 | 禁用或延迟提示与后端策略一致 |
| 每个 phase 后 reload/reconnect | durable fold 可恢复 active state | 不因 reconnect 重复 apply | snapshot 是单一真值 | subscription 与 snapshot 收敛 | revision/digest 不倒退 | Started 仍显示处理中；terminal 后自动更新 |
| Codex 压缩成功 | provider command 与公开 thread events 可审计 | 无法取得内部 prompt 时不伪造 capture | authority=AgentObserved、fidelity=Observed、opaque evidence | authority/fidelity 不丢失 | 明确区分 observed turns 与 unknown private context | 显示“观察到的上下文”，不声称 Exact |
| Surface/append frame 与压缩交错 | current stable frame、append/revoke 与 compaction revision 次序确定 | provider 输入只有当前活跃 frames | recipe 中完整 frame 与 delivery metadata 一致 | frame change 与 recipe revision 收敛 | revoked frame 不再 active，仍留 audit | current context 与审计历史分开显示 |

### Files Found

- `.trellis/spec/backend/agent-runtime-context.md` — Complete Agent、ContextFrame、compaction lifecycle 与 canonical presentation 的主规范。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` — 跨层 source/snapshot/change 与前端消费边界。
- `.trellis/spec/frontend/state-management.md` — canonical feed、terminal convergence 与 reload 规则。
- `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/` — compaction ownership、Agent seam 和 Codex opaque boundary 的既有研究。
- `.trellis/tasks/07-23-contextframe-input-authority-restoration/` — ContextFrame 输入权威恢复与 provider round refresh 的实现记录。
- `.trellis/tasks/07-24-native-session-live-state-audit/` — Native live state、context projection route 与页面刷新链路的实现记录。
- `crates/agentdash-agent-protocol/src/backbone/context_frame.rs` — ContextFrame 与 typed CompactionSummary section 契约。
- `crates/agentdash-agent/src/dash/history.rs` — Dash durable compaction lifecycle、fold 与 summary frame producer。
- `crates/agentdash-agent/src/dash/store.rs` — Applied/Completed 原子持久化和 retained boundary。
- `crates/agentdash-agent/src/dash/service.rs` — Native provider history、ContextFrame 和 tools 的逐轮物化。
- `crates/agentdash-integration-native-agent/src/bridge_execution.rs` — Native compactor 的 retained suffix 与 digest 生成。
- `crates/agentdash-integration-native-agent/src/canonical_projection.rs` — durable compaction 到 canonical event 的映射。
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs` — 产品 context projection 的边界推断与 usage/segment 生成。
- `crates/agentdash-contracts/src/runtime/session.rs` — SessionProjectionView API 数据结构。
- `crates/agentdash-integration-codex/src/complete_agent.rs` — Codex compact command、thread/read 和 Observed snapshot。
- `crates/agentdash-agent-service-api/src/snapshot.rs` — AgentSnapshot 的当前可表达范围。
- `crates/agentdash-agent-runtime-contract/src/managed_projection.rs` — Runtime snapshot 中已存在的 authority/fidelity。
- `packages/app-web/src/features/session/ui/SessionProjectionView.tsx` — context popup、压缩 action 和 refresh 行为。
- `packages/app-web/src/features/session/model/types.ts` — canonical item 到前端 status 的映射。
- `packages/app-web/src/features/session/ui/toolCardRegistry.ts` — compaction 卡片状态约束。
- `packages/app-web/src/features/session/ui/bodies/ContextCompactionCardBody.tsx` — compaction 卡片固定成功文案。
- `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx` — SystemNotice 与 rich CompactionSummary renderer。
- `packages/app-web/src/features/session/ui/contextFrame/ContextFrameStream.tsx` — compaction section 统计展示。
- `packages/app-web/src/features/session/ui/ContextFrameCard.test.tsx` — rich compaction frame 的合成前端测试。
- `packages/app-web/src/features/session/ui/SessionProjectionView.test.tsx` — 与当前生产 projector 不一致的合成 projection 测试。

### Related Specs

- `.trellis/spec/backend/agent-runtime-context.md`：规定 Complete Agent 拥有上下文/compaction，Accepted ContextFrame 的 `rendered_text` 同时用于 provider 与 canonical presentation。
- `.trellis/spec/backend/persistence.md`：durable source、revision 与 replay 是 reload/reconnect 后恢复状态的依据。
- `.trellis/spec/backend/kernel.md`：跨层语义应通过稳定 contract 传递，Product 不应反推 Agent 内部状态。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`：Runtime projection 应保留 source authority/fidelity，前端通过 snapshot/change 收敛。
- `.trellis/spec/frontend/state-management.md`：ContextFrame change 进入 canonical feed，terminal state 需要通过 snapshot reload 收敛。

### External References

无。本评估针对仓库内部契约和当前接入的 provider 能力；未引入外部文档假设。

## Caveats / Not Found

- 本研究是静态代码与既有任务记录审计，没有运行 `pnpm dev` 或真实 provider 会话。07-23、07-24 任务记录本身也注明真实页面/手工链路复验尚未完成。
- 没有发现 Codex `thread/read` 能返回 provider 私有压缩 checkpoint、完整 system prompt 或精确下一轮 request recipe 的内部契约；在该能力出现前只能诚实表达 `Observed`。
- 当前 Native provider 的实际执行上下文看起来正确；主要错误在产品投影、生命周期展示和可验证性。因此未将问题定为 P0。
- 本研究没有修改业务代码、spec 或其它任务文件。
