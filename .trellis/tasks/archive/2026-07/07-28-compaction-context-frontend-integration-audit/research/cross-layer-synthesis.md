# Compaction 跨层完整性审计结论

## 结论

用户提出的三个问题全部成立，但根因不只是前端漏了 loading：

1. `Compaction` 没有成为贯穿 Agent owner、Runtime snapshot、Product command view 与前端的权威 activity。
2. Product context projection 没有复用 Native provider 的真实 context recipe，而是按 presentation event 位置猜测边界。
3. 生产 `CompactionSummary` frame 只生成了 `SystemNotice` 文本，没有使用协议中已有的 typed provenance。
4. Native compactor 的摘要输入漏掉 `ToolCall/ToolResult`；位于 retained tail 之前的工具事实可能从后续模型输入真实消失。

因此当前系统同时存在：

- 模型上下文正确性风险；
- 用户可见上下文与真实模型输入不一致；
- 压缩状态、命令门禁和失败终态不一致；
- refresh/reload 后稳定恢复错误状态；
- 测试以合成事件证明局部行为，却没有证明真实生产纵向链路。

本轮为静态源码审计，没有运行真实 provider 会话或浏览器 E2E。除 provider 实际响应内容外，以下问题均有确定代码路径支撑。

## 与 Runtime 状态链任务的关系

参考 `.trellis/tasks/07-28-runtime-session-state-chain/prd.md` 后，本任务的前端结论进一步收敛：

- 当前并非单纯缺少一次Compaction refresh，而是Runtime feed与Product Workspace HTTP
  conversation同时持有会话状态；
- control plane又通过Conversation presentation event触发Workspace refresh，在两条状态链之间
  做时序协调；
- Compaction不是Turn，所以没有触发既有planner；为planner增加compaction特判只能缓解症状；
- 正确方案是让同一个Agent Runtime connection/view消费Complete Agent `read/changes`，并让
  Compaction state与presentation由同一次`SourceObservation`原子更新；
- Composer、Compact、Fork等控制命令直接消费统一view的availability；Product Workspace退出
  Runtime execution/commands owner角色；
- Context inspector按统一view发布的context revision发起query，不从timeline反推。

详细证据见 `research/control-state-plane-convergence.md`。

## 当前端到端数据流

### Native 手动压缩

```text
Context popup
  -> 读取 Product compact_context availability
  -> 执行 Managed Runtime request_compaction
  -> Dash CompactionStarted durable commit
  -> 同一 HTTP/service future 同步等待 compactor
  -> CompactionApplied + ContextFrameChanged
  -> CompactionCompleted 或 CompactionFailed
  -> Runtime receipt terminal
```

关键断裂：

- UI 展示 authority 与命令执行 authority 不是同一份；
- `CompactionStarted` 只成为 canonical `ItemStarted`，没有进入 snapshot activity；
- UI 把 started item 固定渲染成 completed；
- 本地 pending 只覆盖 HTTP promise，不可由 reload 恢复；
- terminal 不作为 Managed Runtime authoritative reload boundary；
- failed/lost 被 canonicalize 成普通 `ItemCompleted`。

### Native 自动 overflow

```text
Turn A -> ContextOverflow
  -> A Failed
  -> Compaction B Started
  -> B Applied/Completed
  -> continuation C promoted
```

Dash owner 在 B 期间保存 `active_compaction`，repository root 也仍占用原 execution；但公共 snapshot 只看 active Turn。结果是 UI 显示 idle/commands available，新输入却在 owner 层被 conflict，而既有规范所要求的 durable deferred input 没有接通。

### 压缩后模型真实输入

```text
active ContextFrames
+ CompactionSummary frame
+ retained_from 开始的 canonical conversation suffix
+ 压缩后新增 conversation messages
+ structured provider tools
```

Product context projection 当前却使用：

```text
最后一个 ItemCompleted(ContextCompaction) 之后的 records
+ category/preview/token estimate
```

生产顺序中 retained messages 位于 `CompactionCompleted` 之前，因此它们仍发给模型，却从用户“当前上下文”中消失。

## 严重度总表

| 优先级 | 确认问题 | 直接影响 |
| --- | --- | --- |
| P0 | compactor 摘要输入忽略 `ToolCall/ToolResult`，cut 区间内工具事实既不进 summary，也不在 retained suffix | 后续模型上下文真实、不可逆丢失工具事实 |
| P0 | `CompactionFailed/Lost` 被投影为成功 completed，并被 Product 当成新 context boundary | UI 声称成功、稳定隐藏仍有效历史，reload/reconnect 后继续错误 |
| P1 | `active_compaction` 没进入 AgentSnapshot/Runtime activity/command availability | 压缩中没有权威状态，命令显示可用但后端拒绝 |
| P1 | Product projection 忽略 `retained_from` | “当前上下文”与 Native provider 下一轮输入不一致 |
| P1 | Product 与 Runtime 的 compact availability 相互冲突 | active turn 时按钮可点，执行时必然被另一层拒绝 |
| P1 | 自动压缩期间 deferred input/Steer 合同未实现 | 新输入变为 conflict，无法解释其归属 |
| P1 | `CompactionStarted` 后进程退出没有 durable worker/recovery | source 可永久停在 active compaction/Accepted |
| P1 | Native inner effect 与 Complete Agent outer effect 是双 durable record | 两个 owner 通过事后 reconciliation 收敛，存在非原子 gap |
| P1 | manual compaction 实际不 queue、不可 cancel，新输入不 deferred | 实现、规范和 UI 三者语义不同 |
| P1 | summary frame 使用 `SystemNotice`，未使用 typed `CompactionSummary` section | boundary、digest、strategy、statistics、revision 对用户隐藏 |
| P1 | 缺少一等 `AgentContextSnapshot/ModelInputRecipe` | 无法证明输入成员、顺序、revision、authority 与 fidelity |
| P1 | compaction live event 不刷新 workspace command state | composer、重复压缩、Fork 继续使用旧门禁 |
| P1 | context projection 请求没有 target/version commit fence | 旧请求可覆盖新 target 或压缩后的新 projection |
| P1 | Product context API 丢失 `authority/fidelity` | Codex observed history 被误解为 exact provider context |
| P2 | `active_compaction_id` 实际表示最后 completed item | API 名称与语义相反 |
| P2 | 压缩后的 provider usage 没有 revision/stale 状态 | 旧“当前 token”与新 projection 并排展示 |
| P2 | 前端测试仍 mock 旧 outcome shape或生产端不会生成的 frame | 绿色测试无法证明纵向契约 |

## 关键问题证据与根因

### P0：工具事实可能从真实模型上下文消失

- `crates/agentdash-integration-native-agent/src/bridge_execution.rs:297-362`
  的 `effective_conversation` 只物化 `InputAccepted` 与 `AgentOutput`。
- `ToolCall`、`ToolResult`、interaction 和 structured outcome 没进入 summary provider request。
- `crates/agentdash-agent/src/dash/service.rs:2070-2191` 在压缩后只恢复 summary frame 与
  `retained_from` suffix。

所以 cut 区间内的工具调用结果同时不在 summary 和 retained suffix。prompt 即使要求“保留 tool outcomes”，模型也没有收到这些事实。

修复边界：Compaction必须作为原Session上的正式Turn，复用正常Turn的精确context
materialization并只追加一次synthetic instruction；完整tool call/result pairing与structured
outcome必须处于同一prefix，不能另建summary transcript或只修改prompt。

### P0：失败被当作成功边界

- `crates/agentdash-integration-native-agent/src/canonical_projection.rs:197-207`
  把 `CompactionCompleted | CompactionFailed` 都映射为 `ItemCompleted(ContextCompaction)`。
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:53-67`
  把任意 completed compaction item 当作 message boundary。
- `packages/app-web/src/features/session/model/types.ts:616-633`
  对 context compaction 固定返回 `completed`。
- `packages/app-web/src/features/session/ui/bodies/ContextCompactionCardBody.tsx:7-11`
  固定显示“上下文已压缩”。

修复边界：terminal outcome 必须 typed；只有成功 `Applied + Completed` 才推进 context revision，failed/lost/cancelled 只结束 operation，不改变 active recipe。

### P1：压缩不是权威 activity

- Dash fold 已在 `crates/agentdash-agent/src/dash/history.rs:531-548` 保存
  `active_turn` 与 `active_compaction`。
- `ensure_idle` 同时检查二者，但公共 `AgentSnapshot` 没有 active activity：
  `crates/agentdash-agent-service-api/src/snapshot.rs:97-120`。
- Runtime availability 只从 canonical active Turn 推导：
  `crates/agentdash-agent-runtime/src/agent_snapshot_projection.rs:119-132,202-249`。
- canonical `ItemStarted(ContextCompaction)` 不能建立 active Turn：
  `crates/agentdash-agent-protocol/src/presentation.rs:80-94`。

修复边界：在 Agent owner query 中暴露 typed activity；Runtime、Product 和 frontend 只投影它，不从 UI item 或 HTTP promise猜测。

### P1：当前上下文视图使用错误边界

- Native materializer 使用最新成功 compaction 的 `retained_from`：
  `crates/agentdash-agent/src/dash/service.rs:2070-2191`。
- Product projector 使用 `ItemCompleted` 的 history index：
  `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:53-90`。
- 现有测试构造“old user -> completed -> new user”，没有构造生产中的
  “retained A/B -> started/applied/completed”：
  `session_context_projection.rs:503-547`。

修复边界：Product 不再从 presentation position 重建模型上下文，直接消费 Agent owner 物化的 exact/observed recipe。

### P1：ContextFrame 只暴露了 Summary 文本

- 协议已有 `ContextFrameSection::CompactionSummary`，可携带 token、message count、
  compaction identity、strategy、trigger、phase、source range 和 first-kept coordinate：
  `crates/agentdash-agent-protocol/src/backbone/context_frame.rs:530-558`。
- producer 却创建普通 `SystemNotice`：
  `crates/agentdash-agent/src/dash/history.rs:224-268`。
- 前端完整 frame renderer 已能显示 sections、`rendered_text` 与 raw JSON：
  `packages/app-web/src/features/session/ui/contextFrame/ContextFrameBody.tsx:22-95`。

修复边界在 producer 与 query contract，不是重新做 renderer。Conversation messages 仍应保持 canonical records；不能为了“全部 ContextFrame 化”而复制一份消息。

### P1：前端状态和刷新没有闭环

- reducer 已保存 item freshness，但 `SessionEntry` 没把 lifecycle 传给 card：
  `packages/app-web/src/features/session/model/sessionStreamReducer.ts:394-426`，
  `packages/app-web/src/features/session/ui/SessionEntry.tsx:152-191`。
- popup pending 只是本地 promise：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:251-307`。
- control-plane planner 不因 compaction item/frame 刷新 commands：
  `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:157-188`。
- success path 已存在间接 refresh key，但 authoritative reload 只认普通 turn terminal：
  `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:217-237`，
  `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts:36-38,130-146`。
- projection request 直接提交返回值，没有 target/revision fence：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:471-509`。

修复边界：Started/Applied/terminal通过同一source observation更新统一Runtime view；context
revision变化后invalidates recipe query。Workspace不再保存或刷新第二份command state；提交
projection时校验target与revision。

## ContextFrame 与“完整上下文”的正确语义

“完整暴露”应包含两个不同对象：

1. `CompactionSummary ContextFrame`
   - 精确 `rendered_text`；
   - compaction/checkpoint identity；
   - source revision/digest；
   - compacted 与 retained 边界；
   - strategy/trigger/phase；
   - token/message/tool statistics；
   - usage evidence 与确认状态。
2. 当前模型输入配方
   - 有序 active ContextFrames；
   - retained canonical conversation range；
   - structured provider tools；
   - provider private/opaque context evidence；
   - authority、fidelity、context revision、recipe digest。

Native 可提供 `Exact` recipe；Codex 当前只能基于公开 thread history 提供 `Observed`，并显式列出 opaque provider state。不能本地猜测或伪造“完整 Codex ContextFrame”。

## 场景矩阵

| 场景 | 当前状态 | 目标 invariant |
| --- | --- | --- |
| Native 手动成功 | popup 局部 pending；timeline started 立即显示 completed；成功后间接刷新 | durable activity 可恢复；commands 统一 gate；terminal 后 recipe revision 单调收敛 |
| Native 自动 overflow | UI 无压缩中状态；新输入 conflict | B activity 可见；输入按明确策略 deferred 或 unavailable；C 首轮只用新 recipe |
| Failed/Lost | 被显示成成功并推进错误 boundary | typed terminal；active recipe/revision 不变；旧上下文仍可见 |
| active Turn 中请求手动压缩 | Product 显示 enabled，Runtime 拒绝 | 单一 availability authority；queue 或 unavailable 二选一 |
| 压缩期间提交输入 | UI 可能可点，owner 拒绝 | durable deferred 或明确 disabled，不允许 UI/owner 分叉 |
| 压缩期间二次压缩/Fork/Close | UI 与 owner 规则不一致 | command matrix 从 activity 派生 |
| Started 后进程退出 | active source 无 worker 恢复 | durable claim/lease worker，或 inspect 时幂等恢复/typed Lost |
| 页面 reload/reconnect | timeline 可恢复，但错误 outcome/boundary 也稳定恢复 | snapshot 重建 active activity、recipe 与 terminal outcome |
| projection 并发请求/target 切换 | 旧返回可覆盖新状态 | target key + monotonic revision commit fence |
| Surface/append frame 与 compaction 交错 | provider 可重放，Product 没有 active frame set | 同一 materializer 输出有序 recipe，revoked frame 不再 active |
| Codex compaction | operation/context internals opaque | 明确 `AgentObserved/Observed`，不声称 Exact |

## 既有任务覆盖

| 任务 | 已确立内容 | 本次发现的未闭环部分 |
| --- | --- | --- |
| `07-17-agent-runtime-compaction-state-protocol-review` | summary + retained suffix + provenance；queued/deferred；typed lifecycle | 当前实现仍缺 active activity、queue/deferred、typed failure 和前端门禁 |
| `07-23-contextframe-input-authority-restoration` | ContextFrame 是 Agent-visible platform fact；provider/presentation 应同源 | summary producer 仍用 `SystemNotice`；Product 仍没有 active recipe |
| `07-24-native-session-live-state-audit` | live overlay、terminal snapshot convergence、owner invariant | compaction terminal 未进入同一 authoritative refresh/reconciliation 机制 |

不是重新设计第二套架构，而是把这三项任务已经定义的 contract 收敛到同一个 Agent-owned activity 与 context recipe。

## 推荐实施顺序

1. 先修 P0 context correctness：summary recipe 纳入工具事实；failed/lost 不再生成成功 boundary。
2. 建立 Agent-owned `ActivitySnapshot`、typed compaction terminal 与单一 command matrix，并以
   `SourceObservation`原子发布state与presentation。
3. 建立 `AgentContextSnapshot/ModelInputRecipe`，Native 输出 Exact，Codex 输出 Observed。
4. 让 typed `CompactionSummary` frame 携带完整 checkpoint/provenance。
5. 复用相邻任务的单一Agent Runtime connection/view投影activity、commands、authority/fidelity
   与monotonic context revision；不再经Product Workspace复制Runtime control state。
6. 前端用统一view selector完成门禁，用revision-bound query完成Context inspector；不增加
   Compaction presentation event的Workspace refresh特判。
7. 用真实生产 event order、provider capture、reload/reconnect 和 crash injection 做纵向验证。

详细目标结构见 `design.md`，实施拆分见 `implement.md`。
