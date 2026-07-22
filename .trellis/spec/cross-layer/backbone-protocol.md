# Canonical Conversation and Backbone Protocol

## 1. Scope / Trigger

本规范适用于 Complete Agent history/live、Agent Runtime read adapter、AgentRun 会话 API 与前端
Session renderer。修改 turn/item/message/tool/compaction 的生产、传输、恢复或渲染时必须复核。

`CanonicalConversationRecord` 是 Agent 会话 presentation 的唯一跨层表示。Backbone 产品通知仍可
承载 AgentFrame、Workspace Module、PTY terminal 与平台诊断，但不能形成第二套会话 turn/item
协议。

## 2. Signatures

```rust
pub struct CanonicalConversationRecord {
    pub presentation_id: String,
    pub presentation: CanonicalConversationPresentation,
}

pub struct AgentSnapshot {
    pub source: AgentSourceCoordinate,
    pub revision: AgentSnapshotRevision,
    pub conversation_history: Vec<CanonicalConversationRecord>,
    // lifecycle / interaction / surface evidence omitted
}

pub struct AgentLiveEvent {
    pub source: AgentSourceCoordinate,
    pub sequence: AgentServiceU64,
    pub record: CanonicalConversationRecord,
}
```

```http
GET /agent-runs/{run_id}/agents/{agent_id}/runtime
GET /agent-runs/{run_id}/agents/{agent_id}/runtime/live
```

```ts
type AgentLiveEvent = {
  source: AgentSourceCoordinate;
  sequence: AgentServiceU64;
  record: CanonicalConversationRecord;
};
```

## 3. Contracts

- concrete Complete Agent 独占 native history，并在 `read` 中返回完整
  `conversation_history`。Runtime/Product 不保存 normalized turn/item/message/tool 镜像。
- durable history 与 process-local live 使用同一个 `CanonicalConversationRecord` schema；区别只由
  `presentation.durability` 表达。durable live record必须来自已成功提交的native history，ephemeral
  live record只表达尚未提交的Core partial。
- input live顺序是 durable `UserInputSubmitted` → durable `TurnStarted` → ephemeral output →
  durable terminal。snapshot与durable live必须调用同一个canonical projector，不能在execute
  返回后由Product或前端补造用户消息。
- `AgentLiveEvent` 只包含 source、source-local sequence 与 canonical record。transport 不接受
  provider round、`payload.kind`、独立 `turn_id/item_id` 等平行 telemetry 形态。
- `presentation_id` 是同一 presentation 在 baseline/live 合并时的稳定 identity；收到相同 id 时
  替换记录，收到新 id 时追加。不得派生 `agent-turn:`、`agent-item:` 或 renderer-local tool id。
- `TurnStarted`/`TurnCompleted` 是运行状态的唯一边界。第一个 message/tool/item 输出不结束 turn；
  只有对应 `TurnCompleted` 才移除 active turn。
- `ItemStarted`/`ItemUpdated`/`ItemCompleted` 的 `AgentDashThreadItem` discriminant 决定 UI 形态。
  `agentMessage` 与 `reasoning` 进入消息卡，其余 item 进入对应工具/资源卡；未知 discriminant 是
  协议错误，不降级为“TOOL 未知”。
- 工具结果正文以typed content parts保存在concrete Agent history；canonical projector根据ToolCall
  固定的owner projector生成`fsRead`、`fsApplyPatch`、`commandExecution`等ThreadItem。前端Card只
  消费对应`contentItems/details`，不解析callback或provider原始JSON。
- provider/Core 事件只能在 concrete Agent 内部用于构造 canonical records；provider round 完成
  不是 Agent turn terminal，也不能触发前端终态 reload。
- Agent实际保存的surface/context history通过
  `Platform(ContextFrameChanged { frame, message })` 进入canonical stream。前端直接消费该variant；
  `SessionMetaUpdate` 只处理自身metadata语义，不承担ContextFrame旁路编码。
- `ContextFrame`是平台对Agent已接纳context/surface事实的typed presentation，不是Dash输入领域对象。
  `tool_schema_delta`携带`added_tools/removed_tools/changed_tools`；前端按变化类型渲染工具名、来源、
  参数数量与description，不把`parameters_schema`作为默认JSON树展示。frame排序依次使用delivery
  phase、delivery order、created_at与稳定frame id，禁止依赖JavaScript sort的隐式稳定性。
- 断线或进程重启后丢弃 ephemeral lane，并从 Complete Agent `read` 重新获取 durable history。
  Snapshot-only Agent 不需要平台 durable change journal。
- PTY terminal、Canvas、Workspace Module 与 AgentFrame 是独立资源事实；即使它们引用 Agent
  coordinate，也不能完成、恢复或改写 Agent conversation。
- RuntimeWire 透明承载 Driver command/response 与 reverse HostPort frame，不转换为 Product
  Backbone event 或第二套 conversation DTO。

## 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| live payload 缺少 `record.presentation.envelope.event.type` | transport 拒绝该 payload 并报告流解析错误 |
| live payload 使用旧 `turn_id/item_id/payload.kind` | 拒绝；producer/consumer 必须升级到 canonical record |
| 同一 `presentation_id` 再次出现 | 原位替换 presentation，不重复渲染 |
| ephemeral output早于输入/TurnStarted | producer顺序错误；不得靠前端延迟插入修正 |
| 首个 delta/item 到达但没有 `TurnCompleted` | 会话保持 receiving/active |
| `TurnCompleted` 到达 | turn 进入唯一终态；消息、工具和错误按 canonical history 渲染 |
| live 断开或 sequence gap | 丢弃 partial lane并重新 `read` authoritative snapshot |
| terminal turn 没有 assistant item | 保留 terminal-only segment，并展示 `turn.error` |
| PTY terminal 退出 | 只更新 terminal resource，不改变 Agent turn |
| vendor 发送 executor-specific conversation DTO | adapter 边界拒绝或映射为 owned canonical record |
| tool surface同名定义改变 | `changed_tools`渲染↻；不渲染为第二次新增 |
| 多个ContextFrame拥有相同phase/order/time | 以稳定frame id确定顺序，snapshot与live merge后顺序一致 |
| tool delta带完整parameters schema | schema保留为structured contract；默认UI只呈现参数字段计数，不展开原始JSON |
| fsRead结果同时包含typed正文与details | Read Card显示路径、行数和逐行正文；重载后使用同一内容，不展示executor envelope |

## 5. Good / Base / Bad Cases

- Good：Dash先提交并live发布用户输入与`TurnStarted`，Core随后产生tool partial，Dash将完成的tool
  写入native history并live发布canonical `ItemStarted/Updated/Completed`；UI使用相同item id渲染，
  重载后由durable history恢复同一张卡。
- Good：消息首个 delta 到达后 composer 保持运行态，工具继续执行，最终仅由
  `TurnCompleted` 结束。
- Good：相邻surface增加write、修改read、删除search，timeline只显示`+ write / ↻ read / − search`，
  重载authoritative history得到相同顺序与内容。
- Base：live 中途断开，临时 delta 消失；重新 `read` 后完整 assistant/tool history 恢复。
- Bad：transport 校验 `{turn_id,item_id,payload.kind}`，后端发送 `{record}`；所有合法输出会被
  静默吞掉。
- Bad：Managed Runtime 再维护 `turns[]/items[]/active_turn_id`，然后与 canonical history 比较
  currentness；这会把纯视图变成第二事实源。

## 6. Tests Required

- Rust adapter test 覆盖 finalized provider response 中完整 tool calls，并断言生成 canonical tool
  items。
- Complete Agent integration test 覆盖 source-scoped live canonical records，且 live lane 不持久化
  为第二份 tail。
- transport test 断言当前 `{source,sequence,record}` 通过、旧 telemetry payload 被拒绝。
- frontend projection test 断言相同 `presentation_id` 替换、新 id 追加。
- liveness test 断言 `TurnStarted + first output` 仍 active，加入 `TurnCompleted` 后才 inactive。
- ordering test断言用户输入与`TurnStarted`先于第一个ephemeral output；ContextFrame test断言直接
  消费`Platform(ContextFrameChanged)`并保留typed frame。
- ContextFrame frontend tests覆盖added/removed/changed tool渲染、不展示schema JSON，以及相同
  phase/order/time时按frame id稳定排序。
- production tracer 覆盖 Product input → tool live items → final assistant → reload durable history，
  并断言页面没有未知工具卡或悬空会话。
- schema generation 与 TypeScript typecheck 必须证明 Agent service/Runtime wrapper 只引用
  `CanonicalConversationRecord`，不再导出平行 item presentation vocabulary。

## 7. Wrong vs Correct

```ts
// Wrong: provider telemetry becomes a second frontend protocol.
if (event.payload.kind === "tool_call_completed") {
  snapshot.items.push(makeRuntimeToolItem(event));
}

// Correct: merge the one canonical record by its stable presentation identity.
const index = snapshot.conversation_history.findIndex(
  (record) => record.presentation_id === event.record.presentation_id,
);
index >= 0
  ? snapshot.conversation_history.splice(index, 1, event.record)
  : snapshot.conversation_history.push(event.record);
```

```ts
// Wrong: any output implies the Agent stopped receiving.
const isReceiving = lastEvent.type === "agent_message_delta";

// Correct: only canonical turn boundaries define execution liveness.
const isReceiving = hasActiveCanonicalTurn(snapshot.conversation_history);
```

```rust
// Wrong: normalize source ids into another identity namespace.
let runtime_item_id = format!("agent-item:{}", source_item_id);

// Correct: preserve the concrete Agent coordinate in the request-scoped view.
let runtime_item_id = RuntimeItemId::new(source_item_id.into_inner())?;
```
