# Research: Compaction 前端状态、归约与 UI 交互完整性

- Query: Compaction 是否在前端形成明确、可恢复的运行状态；压缩期间哪些交互没有受控；完成、失败、取消、并发输入和断线重连后上下文是否正确刷新；压缩后的当前模型上下文是否以完整 `ContextFrame` 暴露。
- Scope: mixed（前端为主，追踪到直接决定前端语义的 canonical projection 与 context projector）
- Date: 2026-07-28

## Findings

### 1. 结论摘要

三个用户问题的直接答案如下。

1. **压缩中状态不明确。**
   - 手动点击时，只有 Context popup 内部按钮在 HTTP Promise 未结束前显示“提交中”；该状态不进入 Session store、Managed Runtime feed、workspace command state 或全局 composer。
   - 自动压缩没有这层本地状态。
   - canonical `item_started(contextCompaction)` 已经到达前端，但 UI 把该 item 固定判为 `completed`，正文同时写成“上下文已压缩”。
   - Session 的 `isReceiving` 和运行按钮只看普通 `TurnStarted/TurnCompleted` 与 Product `executionStatus`；compaction 没有形成这两个前端所需的 activity 边界。

2. **存在未受控且会与压缩冲突的交互。**
   - composer 文本编辑、附件、文件选择、Enter/Ctrl+Enter 提交仍由旧 Product conversation snapshot 控制；压缩开始不会刷新该 snapshot，也不会让 `isSending/isCancelling` 生效。
   - 压缩按钮仅在本次手动请求 Promise 内防重复；请求结束或自动压缩期间没有 canonical gate。
   - 已完成轮次的 Fork 只按轮次稳定性判断，完全不知道当前正在 compaction。
   - stop/cancel 不会出现，因为运行态检测不识别 compaction；用户既看不到正在压缩，也没有明确的取消能力。
   - Context popup 的只读刷新可以保留；Workspace/VFS 资源浏览不应因 compaction 被整体锁死。需要门禁的是会改变 Agent conversation/context head 的命令，而不是全部页面交互。

3. **完成后的刷新是“部分存在但没有闭环”，不能简单判定为完全未刷新。**
   - 成功路径中，`executor_context_compacted` 或 `CompactionSummary ContextFrame` 会改变 popup 的 `refreshKey`，手动命令 Promise 结束也会调用 `onRefresh`；timeline 也会收到 ContextFrame。
   - 但 live reducer 只把 canonical record 合进 `conversation_history`，不会同步 Managed Runtime snapshot 的 operation/availability/interactions；control-plane planner 也不因 compaction start/applied/completed 刷新 workspace conversation snapshot。
   - 右侧固定“上下文”Tab 在 AgentRun 页面收到的 `contextSnapshot` 明确是 `null`，展示的是 resource surface 等概览，不是当前模型上下文。
   - context projection 只返回 segment `preview`，没有完整内容或完整 active ContextFrame 集合；完整 `rendered_text` 只能在历史 timeline 中逐张展开，无法作为“当前模型输入快照”整体检查。
   - 失败路径更严重：Native canonical projector 把 `CompactionFailed` 也映射成 `ItemCompleted(contextCompaction)`，context projector 又把任何 completed compaction 当作消息边界。刷新/重载后 UI 可能错误隐藏压缩前消息并声称“已压缩”，即使模型上下文实际上没有成功替换。

综合严重度：

| 严重度 | 问题 |
| --- | --- |
| P0 | 失败的 compaction 被投影为成功 completed item，并被 context projection 当成有效压缩边界，用户可见上下文与真实模型上下文相反 |
| P1 | 缺少 canonical active-compaction activity，`item_started` 在 UI 被固定显示为 completed，Session/composer 没有压缩中状态 |
| P1 | Product `compact_context` 与 Runtime `request_compaction` availability 语义冲突：active turn 时前者 enabled、后者必定 unavailable |
| P1 | compaction live 事件不刷新 workspace conversation command state，输入、重复压缩、Fork 等命令继续使用旧状态 |
| P1 | “当前上下文”只暴露 preview 和历史 ContextFrame，缺少可整体恢复/检查的 active ContextFrame snapshot |
| P1 | projection 请求没有 target/version commit fence；旧请求可覆盖新 target 或新 projection |
| P2 | 手动 action 只显示“提交中/请求已接受/请求失败”，不关联 compaction identity/phase，且错误诊断丢失 |
| P2 | 现有前端测试以旧 compaction response shape mock 新 Runtime receipt，未验证真实状态变化 |

### 2. 文件与职责

| 文件 | 职责 |
| --- | --- |
| `packages/app-web/src/generated/agent-run-interaction-contracts.ts` | Product conversation command、stale guard 和已遗留的 compaction outcome DTO |
| `packages/app-web/src/generated/agent-runtime-contracts.ts` | Managed Runtime snapshot、command availability、operation 与 interaction DTO |
| `packages/app-web/src/generated/workflow-contracts.ts` | Workspace conversation snapshot、execution status、command set |
| `packages/app-web/src/generated/session-contracts.ts` | context projection；当前 segment 只有 `preview` |
| `packages/app-web/src/services/agentRunRuntime.ts` | Runtime snapshot、live command、context projection 和 compaction service |
| `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts` | snapshot + live overlay + terminal/reconnect reload |
| `packages/app-web/src/features/agent-run-runtime/model/agentLiveProjection.ts` | live canonical record fold 与普通 turn liveness |
| `packages/app-web/src/features/session/model/useSessionStream.ts` | Managed Runtime feed 到 Session events/reducer 的适配 |
| `packages/app-web/src/features/session/model/sessionStreamReducer.ts` | item/context frame/token usage 的前端归约 |
| `packages/app-web/src/features/session/model/types.ts` | ThreadItem 标题和展示状态 |
| `packages/app-web/src/features/session/ui/SessionChatViewModel.ts` | Product live side effect 与 context projection refresh key |
| `packages/app-web/src/features/session/ui/SessionChatView.tsx` | Session feed、发送/取消本地状态、composer wiring |
| `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx` | composer、context ring、Fork/Copy 等交互 |
| `packages/app-web/src/features/session/ui/SessionProjectionView.tsx` | context popup、手动 compaction、本地 loading/error 与 segments |
| `packages/app-web/src/features/session/model/contextFrame.ts` | ContextFrame/CompactionSummary 前端解析 |
| `packages/app-web/src/features/session/ui/ContextFrameBody.tsx` | 完整 `rendered_text` 与 raw frame debug 展示 |
| `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx` | CompactionSummary structured section 展示 |
| `packages/app-web/src/features/session/ui/bodies/ContextCompactionCardBody.tsx` | compaction item 的固定成功文案 |
| `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts` | live/project event 到 workspace/list 刷新的 effect planner |
| `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts` | workspace snapshot 的 loading/refresh/error/target fence |
| `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` | AgentRun 页面组合；当前把 `sessionContextSnapshot` 固定为 null |
| `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx` | 右侧“上下文”Tab；消费旧 context snapshot/resource surface 概览 |
| `crates/agentdash-integration-native-agent/src/canonical_projection.rs` | Native history 到 canonical conversation record 的 compaction 映射 |
| `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs` | 当前 context projection 的消息边界与 segment 构造 |
| `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs` | Product conversation execution/command availability |
| `crates/agentdash-agent-runtime/src/agent_snapshot_projection.rs` | Runtime command availability |
| `crates/agentdash-agent/src/dash/history.rs` | `active_compaction` 的真实 Agent history 状态 |

### 3. 当前前端数据流

#### 3.1 手动命令

```text
ContextUsageRing
  -> SessionProjectionViewPanel.handleCompactContext
  -> compactAgentRunContext
     -> GET /runtime/snapshot
     -> 检查 request_compaction availability
     -> POST /runtime/commands { request_compaction }
  -> ManagedRuntimeOperationReceipt
  -> popup local compactAction
  -> onRefresh(context projection)
```

证据：

- popup 只持有局部 `compactAction`，并只把 `compactPending` 用于禁用压缩按钮：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:251-265`。
- 点击后显示“提交中”，等待 `compactAgentRunContext`，再按 operation status 显示通用成功/失败并刷新：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:271-300`。
- service 不使用传入 UI 的 `ConversationCommandView.stale_guard`，而是再次 GET Runtime snapshot，
  检查 `request_compaction` 后调用通用 `/runtime/commands`：
  `packages/app-web/src/services/agentRunRuntime.ts:61-72`。
- Product facade 是同步 handoff，HTTP 会等待 concrete Agent `execute(...).await`：
  `crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:66-70`,
  `:178-207`。因此 popup 的“提交中”在当前 Native 路径实际上可能覆盖完整压缩执行，不只是网络提交。

#### 3.2 live 与 timeline

```text
GET /runtime/snapshot -> baseline conversation_history
GET /runtime/live     -> AgentLiveEvent(record)
  -> applyAgentLiveEvent
  -> useSessionStream 把 record 数组重新编号为 SessionEventEnvelope
  -> sessionStreamReducer
  -> SessionEntry/toolCardRegistry/ContextFrameCard
```

证据：

- live projector只替换/追加 `conversation_history`，除 `thread_name` 外不更新 snapshot 的其他字段：
  `packages/app-web/src/features/agent-run-runtime/model/agentLiveProjection.ts:21-43`。
- Session liveness只扫描 `turn_started/turn_completed`：
  `packages/app-web/src/features/agent-run-runtime/model/agentLiveProjection.ts:5-18`。
- `useSessionStream` 只以 `hasActiveCanonicalTurn` 输出 `isReceiving`：
  `packages/app-web/src/features/session/model/useSessionStream.ts:148-163`。
- Native 已发布 `CompactionStarted -> ItemStarted(contextCompaction)`，
  `CompactionApplied -> executor_context_compacted + ContextFrameChanged`，
  `CompactionCompleted/Failed -> ItemCompleted(contextCompaction)`：
  `crates/agentdash-integration-native-agent/src/canonical_projection.rs:173-207`。

#### 3.3 current context popup

```text
raw canonical events
  -> computeProjectionRefreshKey
     terminal turn | context_compacted | compaction_summary
  -> GET /runtime/context/projection
  -> projection previews/categories
```

证据：

- refresh key 识别 terminal turn、`context_compacted` 和 CompactionSummary ContextFrame：
  `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:196-237`。
- SessionChatView 把该 key 传给 context ring：
  `packages/app-web/src/features/session/ui/SessionChatView.tsx:291-294`,
  `packages/app-web/src/features/session/ui/SessionChatView.tsx:651-654`。
- projection component 在 target/key 变化后请求 context projection：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:482-509`。

### 4. P0：失败压缩被伪装成成功并成为错误的 context boundary

#### 现象

`CompactionFailed` 和 `CompactionCompleted` 在 canonical presentation 层被合并成相同
`ItemCompleted(contextCompaction)`；ThreadItem 本身又不携带 status/error。
前端因此显示“上下文已压缩”，context projector 还会把该 failed item 当作最新压缩边界，
从当前 projection 中移除压缩前消息。

#### 证据

- Native projector 对 `CompactionCompleted | CompactionFailed` 使用同一 `ItemCompleted` 分支，
  error/lost 完全没有进入 payload：
  `crates/agentdash-integration-native-agent/src/canonical_projection.rs:197-207`。
- Agent history 本身其实区分 Failed/Lost，并清空 `active_compaction`：
  `crates/agentdash-agent/src/dash/history.rs:960-975`。
- context projector 反向查找任何 `ItemCompleted` 的 contextCompaction，并把其后一位作为
  `message_boundary`：
  `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:53-67`。
- response 的 `active_compaction_id` 也是这个“最近 completed item”，并非真正 active state：
  `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:214-220`。
- 前端 ContextCompaction body 固定写“上下文已压缩”：
  `packages/app-web/src/features/session/ui/bodies/ContextCompactionCardBody.tsx:7-11`。
- 前端对所有 contextCompaction item 固定返回 `"completed"`：
  `packages/app-web/src/features/session/model/types.ts:616-633`。

#### 影响

- 手动失败：HTTP receipt 可显示“压缩请求失败”，但随后的 `onRefresh` 会读取错误 boundary；
  页面可能同时显示失败提示与已被截断的“当前上下文”。
- 自动失败：没有 popup 本地错误；timeline 直接显示成功卡。popup 不会因普通 item_completed 自动刷新，
  但页面重载/手工刷新后会读取错误 boundary。
- 断线重连：authoritative snapshot 恢复的是同一个错误 presentation，错误会稳定重现而非自愈。
- 这是 current-model-context authority 错误，不只是文案问题。

#### 建议边界

- canonical contract必须保留 compaction terminal outcome，或遵循
  `started -> error + failed terminal`，不能把 failed 映射成成功 `ItemCompleted`。
- context projector只接受有成功 `CompactionApplied`/CompactionSummary frame 证据的 boundary；
  `ItemCompleted` 本身不够。
- 前端 card 状态从 event lifecycle/typed outcome读取，不再从无 status 的 ThreadItem 猜测。

#### 验证

- production fixture：`CompactionStarted -> CompactionFailed` 后，旧消息仍在 context projection，
  timeline 显示失败且不存在 CompactionSummary frame。
- reload/reconnect 后同样成立。

### 5. P1：没有明确的 active-compaction 前端状态

#### 现象

前端收到 `item_started(contextCompaction)` 后，卡片立即显示 completed；Session 状态栏、
composer 和 stop button 均不知道压缩正在占用 Agent history。

#### 证据

- reducer会把 `item_started/item_updated/item_completed` 合并到同一个 item，
  并正确保存 `isStreaming` 与 freshness：
  `packages/app-web/src/features/session/model/sessionStreamReducer.ts:394-426`。
- 但 `SessionEntry` 调用 `renderToolCallCard(threadItem)` 时没有把 event type /
  `entry.isStreaming` 传给 card status：
  `packages/app-web/src/features/session/ui/SessionEntry.tsx:152-191`。
- registry取得的 status来自 `getThreadItemStatus`，contextCompaction 最终仍固定 completed：
  `packages/app-web/src/features/session/ui/toolCardRegistry.ts:157-163`,
  `packages/app-web/src/features/session/model/types.ts:629-630`。
- `isAgentRunWorkspaceActionRunning` 只接受
  `starting_claimed | running_active | cancelling`：
  `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:82-88`。
- composer据此决定是否显示 stop：
  `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:843-851`,
  `packages/app-web/src/features/session/ui/composer/ComposerSendButton.tsx:42-57`。
- Agent 的权威 history 已有独立 `active_compaction`，但 Managed Runtime snapshot DTO没有对应
  activity/compaction phase字段；snapshot只有 lifecycle、interactions、operations、
  command availability和conversation history：
  `packages/app-web/src/generated/agent-runtime-contracts.ts:79`。

#### 影响

- 自动压缩完全没有进行中反馈。
- 手动压缩只在用户保持 popup 打开时看到局部 spinner；主会话仍显示 idle/ready。
- 用户看到 `item_started` 时反而被告知“已经压缩”，误判可以继续操作。
- 无 canonical compaction ID/phase，前端不能把手动请求、live start、terminal 和 projection version
  关联为同一次 operation。

#### 建议边界

- Managed Runtime/conversation view投影一个 typed active activity，例如
  `{ kind: "context_compaction", compaction_id, phase, cancellable }`。
- Session reducer可继续复用同一 canonical item，但 card status必须读取 event lifecycle；
  全局 gate读取 activity snapshot，不扫描 UI item。
- 手动 local pending只作为“命令正在提交”的短态；收到 canonical activity 后切换到权威状态。

### 6. P1：Product 与 Runtime 的 compaction availability 自相矛盾

#### 现象

Product conversation snapshot明确允许 active turn 时 `compact_context`，但 Runtime snapshot
只允许没有 active turn时 `request_compaction`。UI用前者显示按钮，点击后service用后者二次检查，
所以 active turn中会展示可点击按钮，随后必然报“Managed Runtime 当前不接受 context compaction”。

#### 证据

- Product `compact_context` 在 `Ready || RunningActive` 时 enabled：
  `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:646-659`。
- 对应测试明确断言 running active 的 compact command enabled，并携带 active turn stale guard：
  `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:1168-1188`。
- Runtime `SubmitInput | RequestCompaction | Fork` 只在 `active && !has_active_turn` 时 available：
  `crates/agentdash-agent-runtime/src/agent_snapshot_projection.rs:202-223`。
- frontend service再次读取 Runtime availability，拒绝后才决定是否 POST：
  `packages/app-web/src/services/agentRunRuntime.ts:61-72`。
- popup接收 Product `ConversationCommandView` 来决定 enabled：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:257-270`。

#### 影响

- “active turn中排队压缩”的旧 UI/DTO 意图已失效。
- 用户无法区分“按钮状态过期”与“运行时不支持排队”。
- Product stale guard没有被当前 compaction service发送；展示 authority和执行 authority不是同一份。

#### 遗留合同证据

- `AgentRunContextCompactionCommandOutcome` 仍声明
  `scheduled_next_turn | launched_compaction_turn | completed | ...`：
  `packages/app-web/src/generated/agent-run-interaction-contracts.ts:30-32`。
- 当前 service实际返回 `ManagedRuntimeOperationReceipt`：
  `packages/app-web/src/services/agentRunRuntime.ts:61-72`。
- helper仍按旧 outcome映射状态：
  `packages/app-web/src/features/session/ui/sessionProjectionCompactionAction.ts:19-35`，
  但 production component已不调用它。

#### 建议边界

- 删除 Product/Runtime 双命令视图；Context popup直接消费与 POST 同源的 generated Runtime command view，
  或让 Product command projection无损转发Runtime availability和同一 stale identity。
- 明确产品选择：active turn期间“拒绝”或“durable deferred”。当前实现不能一边显示 enabled，
  一边由 Runtime立即拒绝。

### 7. P1：压缩期间 command state 不刷新，冲突交互继续开放

#### 现象

compaction start/applied/completed live records只更新 timeline。control-plane effect planner
只在普通 turn start/end、thread name、workspace module和部分project invalidation上刷新 workspace；
因此 Product conversation commands保持压缩前状态。

#### 证据

- live projector不更新 `command_availability`、operations或interactions：
  `packages/app-web/src/features/agent-run-runtime/model/agentLiveProjection.ts:21-43`。
- control-plane live event planner只对 `turn_started`、`turn_completed`、thread name、
  task_write和workspace-module presentation产生 effect：
  `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:157-188`。
- ContextFrame测试还明确要求其留在 canonical feed lane，不刷新control plane：
  `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.test.ts:258-298`。
- composer `sendDisabled` 只看 submit command、`isCancelling` 和本地 `isSending`：
  `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:670-691`。
- RichInput没有 disabled prop；附件菜单只在 `isSending` 时禁用：
  `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:793-808`。
- `handleSubmit` 也只检查 command enabled和本地 `isSending`：
  `packages/app-web/src/features/session/ui/SessionChatView.tsx:351-397`。
- Fork availability只看历史轮次是否完成/有message ref：
  `packages/app-web/src/features/session/model/roundActions.ts:50-90`；
  toolbar只防本次fork Promise重入：
  `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:537-595`。

#### 未受控交互清单

| 交互 | 当前压缩期间行为 | 风险/判断 |
| --- | --- | --- |
| Enter提交新消息 | 旧 submit command仍可能enabled | 会与 Agent `active_compaction` 冲突并在后端失败；P1 |
| Ctrl/Cmd+Enter steer | workspace仍可能保留旧 active turn command | active turn与Runtime compaction availability本就冲突；状态易误导；P1 |
| 再次点击压缩 | 仅手动请求Promise期间禁用；自动压缩/请求结束后无gate | 重复请求或后端拒绝；P1 |
| stop/cancel | compaction不算running，因此按钮不出现 | 用户无法取消或理解不可取消阶段；P1 |
| Fork已完成轮次 | 仍可点击 | 可能在context head mutation期间启动fork；应由Runtime命令availability裁决；P1 |
| 文本编辑 | 可继续 | 可以保留草稿，不应与“提交”一起禁用 |
| 文件/图片选择 | 仍可操作 | 作为草稿准备可保留；真正提交必须gate |
| Context refresh / 展开历史 | 可操作 | 只读，应保留 |
| Workspace/VFS资源浏览 | 可操作 | 资源视图不应因conversation activity整体锁死 |
| approval/interaction response | 当前Session只展示静态 approval event，未见当前AgentRun页面的响应控件 | 没有可证明的前端compaction gate；不能声称已覆盖 |

### 8. P1：完成后只刷新部分状态，且 refresh 有竞态

#### 已有正确部分

- 成功时 `CompactionApplied` 发布 `executor_context_compacted` 后再发布 accepted ContextFrame：
  `crates/agentdash-integration-native-agent/src/canonical_projection.rs:184-196`。
- reducer虽然不渲染 `executor_context_compacted`，仍把durable事件保存在 `rawEvents`，因为
  `rawEvents` 在display过滤前追加：
  `packages/app-web/src/features/session/model/sessionStreamReducer.ts:604-615`；
  display过滤在 `:546-548`。
- `computeProjectionRefreshKey` 会因此触发 context projection refetch：
  `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:217-237`。
- 手动操作不论 success/failure receipt都会调用 `onRefresh`：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:286-300`。
- reconnect时 Managed Runtime feed会重新读取 authoritative snapshot：
  `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts:100-122`。

#### 缺失部分

- authoritative reload的自动边界只识别 `turn_completed`，不识别 compaction terminal：
  `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts:36-38`,
  `:130-146`。compaction成功主要依赖 durable live overlay，直到手工reload/重连才替换完整snapshot。
- workspace conversation snapshot不因compaction刷新，见上一节 planner证据。
- right Context tab没有 current Session context owner。AgentRun page显式：
  `const sessionContextSnapshot = null`：
  `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:202-205`，
  随后把它传入 Workspace data：
  `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:530-557`。
- ContextOverviewTab只消费该nullable snapshot和resource/lifecycle概览：
  `packages/app-web/src/features/workspace-panel/ContextOverviewTab.tsx:112-187`。

#### 请求竞态

`SessionProjectionView.refresh` 没有 target key、request generation或
`projection_version` monotonic commit fence：

- 每次请求完成直接 `setProjection(next)`：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:482-496`。
- effect cleanup只阻止尚未执行的microtask，并不会阻止已发请求提交结果：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:499-509`。

因此存在两个已确认的代码级竞态：

1. compaction前已发出的旧 projection 请求晚于完成后的刷新返回，覆盖新 projection；
2. AgentRun target切换后，旧 target请求晚到并写入新页面。

后端已提供 `projection_version`，前端却没有用它拒绝同target的旧版本，也没有以target key隔离。

#### 建议边界

- compaction terminal和ContextFrame accepted应驱动同一次 typed effect：
  reload Runtime snapshot + invalidate context projection + refresh workspace conversation commands。
- context projection state使用 `{targetKey, status, projection}`，请求提交时同时检查targetKey和
  `projection_version >= current`。
- error refresh保留最后一份同target projection可以接受，但必须显式标注 stale/error；
  target不匹配时绝不能暴露旧数据。

### 9. P1：完整 ContextFrame 可见性仍不等于“当前完整模型上下文”

#### 已解决部分

ContextFrame timeline链已经能展示同一 accepted frame：

- parser要求并保留 `rendered_text` 和全部可识别 sections：
  `packages/app-web/src/features/session/model/contextFrame.ts:327-359`。
- CompactionSummary section保留summary、token、范围、strategy、trigger、phase等字段：
  `packages/app-web/src/features/session/model/contextFrame.ts:633-650`。
- ContextFrame body逐section渲染，并提供“Agent 实际原文”和完整 JSON debug：
  `packages/app-web/src/features/session/ui/ContextFrameBody.tsx:22-30`,
  `:34-55`, `:60-95`。

这说明“只给 Summary、连实际原文也完全看不到”已不符合当前 timeline 实现：
`rendered_text` 可以展开查看。

#### 仍未解决的语义缺口

1. `SessionProjectionSegmentViewResponse` 只有 `preview`，没有完整content或typed ContextFrame：
   `packages/app-web/src/generated/session-contracts.ts:22-30`。
2. popup最多显示三行preview：
   `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:177-205`。
3. projection categories虽有 `context_usage.items`，UI没有渲染 `items`；只渲染categories、
   message breakdown、top tools/attachments：
   `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:133-174`,
   `:389-451`。
4. 右侧Context tab没有AgentRun current context snapshot，见上一节。
5. CompactionSummary structured body没有渲染 `section.summary`；它只显示messages/tokens/range等metadata：
   `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx:836-860`。
   summary正文只能从折叠的“Agent 实际原文”读取。

#### 语义判断

- “压缩摘要”作为一个 `CompactionSummary ContextFrame` 是正确的：它是旧消息折叠后继续进入模型的
  可读恢复事实。
- “当前完整模型上下文”不应被伪装成一个巨大新 frame；它应是一个有版本的 active context snapshot，
  明确列出：
  - 当前仍active的stable ContextFrames；
  - compaction summary frame；
  - compaction后保留/新增的conversation消息与工具结果；
  - 每一项的完整typed内容或可无损读取引用。
- 当前 popup只提供preview和估算，timeline只提供历史frame；两者都不能单独回答
  “此刻下一次provider request会消费哪些完整内容”。

### 10. loading、error 与 terminal 展示

#### Loading

- 初始feed只在没有任何display item时显示“正在连接”：
  `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:261-270`。
- compaction loading仅存在于popup按钮，主timeline的 started card仍显示completed。
- context projection refresh会保留旧projection并显示刷新按钮 loading；这是合理的
  stale-while-refresh形态，但没有stale/version标志。

#### Error

- popup只用 generic receipt status映射“压缩请求失败”，没有 operation id、compaction id、
  backend diagnostic：
  `packages/app-web/src/features/session/ui/SessionProjectionView.tsx:286-299`。
- canonical failure error被后端 projector丢弃，前端timeline无法补救。
- Session stream错误横幅标题固定为“发送失败”，即使实际是live stream/reconnect错误：
  `packages/app-web/src/features/session/ui/SessionChatView.tsx:591-604`。

#### Terminal

- contextCompaction started/completed在reducer中共享identity是正确方向，
  但terminal状态被ThreadItem固定completed覆盖。
- `CompactionApplied`与`CompactionCompleted`是两个durable边界；popup在前者的
  `context_compacted/summary`即可刷新内容，但全局 activity必须等真正terminal再解除命令gate。

### 11. 场景矩阵

| 场景 | 当前行为 | 结论 |
| --- | --- | --- |
| 手动成功 | popup显示“提交中”，HTTP完成后显示“压缩请求已接受”并刷新；live timeline先显示错误的“已压缩”started card，随后summary frame到达 | 部分收敛；缺全局状态与准确phase |
| 自动成功 | 无popup pending；started card立即显示completed；summary/context_compacted触发popup refetch | 用户无法知道正在压缩，完成后popup通常会更新 |
| 手动失败 | receipt显示通用失败并刷新；canonical又把failed映射completed，projection可能错误截断 | P0 |
| 自动失败 | timeline显示成功卡；无summary refresh；reload后projection把failed completed当boundary | P0 |
| 取消/中断 | UI不把compaction视为running，stop不出现；协议未给前端cancellable phase | 未实现明确合同 |
| 压缩中并发输入 | composer继续使用旧Product命令；Agent history `ensure_idle`会拒绝active_compaction期间的新activity | 前端应gate/defer，当前会制造可预期错误 |
| active turn点击压缩 | Product按钮enabled，Runtime precheck拒绝 | 双availability合同冲突 |
| 断线中发生压缩 | 当前overlay保留；connected后reload snapshot，durable history可恢复started/terminal/frame | 数据可恢复，但错误success/failure语义也会恢复 |
| 页面重新加载 | baseline包含durable compaction item/frame，context popup初始请求projection | 能恢复历史；不能恢复明确“active compaction phase”，failed boundary仍错 |
| target切换时projection在途 | 旧请求可写入新target组件state | 缺target commit fence |

并发输入后端事实：

- Agent history `ensure_idle` 在 `active_turn` 或 `active_compaction` 任一存在时拒绝新activity：
  `crates/agentdash-agent/src/dash/history.rs:1019-1024`。
- 前端没有读取该 `active_compaction`。

### 12. Hydrate 与 reconnect 评估

#### 已有保证

- baseline presentation IDs在首次snapshot加载时冻结：
  `packages/app-web/src/features/agent-run-runtime/model/useManagedRuntimeFeed.ts:65-75`。
- reconnect时 connection重读authoritative snapshot：
  `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts:100-122`。
- feed测试覆盖普通turn reconnect与terminal overlay收敛：
  `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.test.ts:199-235`,
  `:262-300`, `:302-331`, `:379-438`。

#### 缺失保证

- 没有 compaction started/applied/completed/failed 专项 reconnect测试。
- reload boundary只看 `turn_completed`，而当前compaction不产生普通turn terminal。
- Runtime snapshot没有active compaction字段，若断线发生在 started 与terminal之间，reload只能从
  history item推断；当前前端既不推断全局activity，也把started card显示completed。
- page reload的baseline历史不会驱动命令式副作用是正确的，但 active maintenance activity属于
  snapshot状态，不应依赖“重放started副作用”；当前缺少该snapshot状态。

### 13. 现有测试覆盖与缺口

#### 已覆盖

- Native success integration证明manual compaction在history/changes中产生一次 started item：
  `crates/agentdash-integration-native-agent/tests/complete_agent_service.rs:3161-3244`。
- ContextFrame card测试证明CompactionSummary结构字段可以parse/display：
  `packages/app-web/src/features/session/ui/ContextFrameCard.test.tsx:169-193`。
- Runtime feed测试覆盖普通turn terminal reload和通用reconnect：
  `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.test.ts:199-438`。
- projection panel测试覆盖preview渲染和按钮disabled文案：
  `packages/app-web/src/features/session/ui/SessionProjectionView.test.tsx:28-99`。

#### 测试自身漂移

- 点击压缩测试mock的是旧
  `{ command_receipt, outcome: "launched_compaction_turn" }`：
  `packages/app-web/src/features/session/ui/SessionProjectionView.test.tsx:101-134`。
- production `compactAgentRunContext`返回的是 `ManagedRuntimeOperationReceipt`：
  `packages/app-web/src/services/agentRunRuntime.ts:61-72`。
- 测试只断言service被调用，没有断言 pending、success/error状态、onRefresh顺序或receipt status，
  因而旧mock仍能通过。
- `contextCompactionOutcomeMessage`测试覆盖的是已不被production component调用的旧helper：
  `packages/app-web/src/features/session/ui/SessionProjectionView.test.tsx:137-172`。

#### 必须新增

1. reducer/render：
   - started显示in progress；
   - completed显示success；
   - failed/lost显示对应terminal与diagnostic；
   - 同ID start→terminal原位合并。
2. command gate：
   - active compaction时submit/steer/repeat compact/fork按统一command snapshot禁用或defer；
   -草稿编辑与只读浏览保持可用；
   - terminal后commands恢复。
3. projection refresh：
   - applied刷新内容但不提前解除activity gate；
   - failed不移动message boundary；
   - overlapping old/new version只提交新版本；
   - target A慢请求不能写入target B。
4. reconnect/hydrate：
   - started时断线，snapshot恢复active compaction；
   - success/failure terminal后重连；
   - baseline历史不触发命令式UI，但snapshot activity正确。
5. production browser tracer：
   - 手动与自动各一条；
   - 点击压缩后主页面出现“正在压缩”；
   -冲突命令不可提交；
   -成功后Context popup和完整active frames同revision；
   -失败后旧消息仍属于current context。

### 14. 建议实施切片（按顺序）

#### Slice A：先修 canonical failure/boundary（P0）

- 区分 compaction success/failed/lost presentation。
- context projector以成功applied evidence建立boundary。
- 修复timeline card terminal outcome。
- 这是所有前端修复的前置条件，否则刷新越及时，错误投影暴露越快。

#### Slice B：建立 active activity snapshot（P1）

- Runtime snapshot与Product conversation view共享typed activity。
- compaction started/phase/terminal成为可hydrate事实。
- command availability从该activity一次投影，不再由Product和Runtime各自定义。

#### Slice C：前端统一 gate 与展示（P1）

- `SessionChatView`接收activity/commands，主状态栏显示Compacting。
- event lifecycle驱动card，snapshot activity驱动command gate。
- 明确保留草稿编辑/只读资源操作，限制conversation head mutation。

#### Slice D：current context snapshot（P1）

- context projection返回完整可恢复的active items/frames或typed读取引用，不只preview。
- AgentRun右侧Context tab接入该projection；popup和tab共享target/version-scoped store。
- 明确compaction summary、stable frames和post-boundary conversation的组合关系。

#### Slice E：收敛刷新与测试（P1/P2）

- compaction lifecycle进入typed effect planner。
- projection请求增加target/version fence。
- 删除旧 outcome DTO/helper与漂移测试。
- 增加真实 production composition/browser tracer。

## Code Patterns

### Pattern A：canonical history与current snapshot混合使用，但只更新history字段

`applyAgentLiveEvent`适合conversation overlay，不适合更新availability/operation/activity。
需要明确两类行为：

```text
canonical presentation append/replace -> conversation_history overlay
owner state invalidation               -> authoritative snapshot reload
```

当前只实现了第一行，普通turn terminal才触发第二行。

### Pattern B：UI局部pending不能替代canonical activity

`compactAction.kind === "pending"`只能防按钮Promise重入。自动compaction、页面重载、断线恢复、
其他入口均无法恢复它。正确状态owner必须在Agent/Runtime snapshot。

### Pattern C：ContextFrame history与active context snapshot是不同读模型

- timeline回答“哪些frame曾被accepted”；
- active context snapshot回答“下一次provider request会消费什么”。

两者应来自同一 accepted facts，但不能让用户通过翻历史自行重建currentness。

## External References

- 未使用网络外部资料；本轮以当前仓库代码为权威。
- 复用既有本地研究
  `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/research/current-compaction-state-and-codex-reference.md`：
  该文已指出前端把所有 contextCompaction 固定为 completed，并以本地 pinned Codex reference
  说明 canonical success应有 started/completed、failure不应伪造completed。
- 当前代码相较07-17研究已前进：Native现在确实发布started/applied/completed；
  但前端activity、failure语义和command gate仍未收敛。

## Related Specs

- `.trellis/spec/frontend/architecture.md`
  - conversation history/live必须同schema；
  - `TurnStarted/TurnCompleted`是普通turn liveness边界；
  - terminal snapshot期间live overlay必须收敛。
- `.trellis/spec/frontend/type-safety.md`
  - generated DTO是wire事实源；
  - Runtime command enabled状态应来自Runtime snapshot。
- `.trellis/spec/frontend/state-management.md`
  - target-scoped state、snapshot/live/reconnect边界。
- `.trellis/spec/frontend/hook-guidelines.md`
  - NDJSON feed、baseline和reconnect。
- `.trellis/spec/cross-layer/architecture.md`
  - Managed Runtime history/current projection、ContextFrame input authority。
- `.trellis/spec/cross-layer/backbone-protocol.md`
  - canonical ContextCompaction item与ContextFrame展示合同。
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
  - frontend不得从间接信号猜状态；
  - dispatcher迁移必须同时处理reducer、effect planner、store和renderer。

## Caveats / Not Found

- 本轮没有修改或运行业务代码，只做静态审计；没有启动 `pnpm dev` 做浏览器生产 tracer。
- 当前AgentRun Session UI中只发现静态 `approval_request` 展示，未发现与该页面直接接线的
  typed approve/deny/user-input response控件；因此无法证明approval响应在compaction期间有任何前端gate。
- 外部Codex/Remote Agent可能拥有不同的native compaction lifecycle；本文件只对当前
  AgentRun前端消费合同与Native直接证据下结论。
- `ContextFrame.rendered_text`已能在timeline展开，这是当前已解决能力；问题是缺少
  current active context的整体可读投影，不应误报为“ContextFrame原文完全不可见”。
- `active_compaction_id`当前实际表示“最近一个completed contextCompaction item”，命名与语义不一致；
  UI不能把它当作正在运行的状态。
