# Companion 与 Channel message 语义收束设计

## Recommended Direction

采用“消息一律 channel/source aware，模型通道独立决定”的方案。

短期不把 runtime-session 主链路替换成完整 channel broker，而是在 Backbone user-input 事件和 session feed read model 上补齐 channel/source provenance。这样既能修复 Companion 被塞进 system frame 的问题，又能复用现有 `ChannelAddress -> MailboxSourceIdentity` 语义，为后续外部 channel、IM、integration 接入留下统一视角。

核心规则：

```text
Channel/source identity 描述消息从哪里来、谁发的、属于哪个 route。
Model delivery role 描述模型应该如何消费这条消息。
UI presentation 描述前端如何展示这条消息。
```

这三个维度不能再互相推断。`origin != user` 不等于 `model_role = system`。

## First Principles

这次问题可以化简为一句话：Companion 协作内容是需要 Agent 响应的输入，却被当前实现提升成了 system authority。

基本事实：

- 模型 system channel 应承载平台规则、身份、环境、能力和运行期控制事实。
- Companion 请求、结果和 parent/human response 会改变 Agent 本轮要处理的任务内容，因此更接近 user-role message。
- 来源不是人类，只说明 provenance，不说明模型 authority。
- 前端展示需要区分“用户本人说的”和“Companion/channel 注入的输入”，但这不是 system role 的理由。
- 已有 `ChannelAddress` 和 `MailboxSourceIdentity` 正好承载 provenance，不需要再发明一组分散字段。

## Existing Evidence

### Channel / Mailbox 底座

`crates/agentdash-domain/src/channel/mod.rs` 已定义：

- `ChannelAddress { namespace, kind, source_ref, correlation_ref, actor, route, display_label_key, metadata }`
- `ChannelMessage { channel_id, sender, audience, correlation_ref, address, payload, content_refs, provider_event_ref }`
- `ChannelDeliveryIntent { message, target, policy }`
- `ChannelDeliveryTarget::Mailbox { run_id, agent_id }`

`channel_address_to_mailbox_source_identity` 已把 `ChannelAddress` 投影为 `MailboxSourceIdentity`，说明 channel 语义与 mailbox source identity 已经同构。

`crates/agentdash-application/src/channel.rs` 的 `materialize_delivery_to_mailbox` 已把 channel delivery 转成 `NewAgentRunMailboxMessage`，并使用 `origin = mailbox_origin_from_channel_address(address)`。这说明 mailbox 是 channel delivery 的一个 materialization target，而不是 channel 的替代模型。

### 当前偏差

`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs`：

- `build_system_delivery_context_frame` 会把 `CompanionDispatch | CompanionParentResume` 判为 `companion_delivery`。
- 该 frame 的 `message_role` 是 `system`。
- 一旦生成该 frame，`resolved_payload.prompt_payload` 被替换成固定继续文本。

`crates/agentdash-application-runtime-session/src/session/launch/commit.rs`：

- `should_persist_as_human_user_input` 只允许 `HttpPrompt | LifecycleAgentUserMessage | LocalRelayPrompt`。
- Companion 会落到 `build_system_delivery_envelope`，生成 `PlatformEvent::SessionMetaUpdate(key="system_message")`。

`crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs`：

- user origin steering 会调用 `emit_user_input_submitted`。
- non-user origin 会调用 `emit_non_user_mailbox_delivery_projection`，生成 `system_message`。
- Companion origin 因此仍可能绕过 runtime-session commit 的修正。

## Target Semantics

### Message Dimensions

推荐在 Backbone user input payload 中新增一个 channel/source 字段，语义接近：

```rust
pub struct UserInputSource {
    pub namespace: String,
    pub kind: String,
    pub source_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub actor: String,
    pub route: Option<String>,
    pub display_label_key: String,
    pub metadata: Option<serde_json::Value>,
}
```

可选增强字段：

```rust
pub enum UserInputPresentation {
    User,
    Companion,
    Channel,
    SystemActor,
}
```

但 presentation 可以先由前端从 `source.namespace/kind/actor/route` 派生，不一定要作为协议字段。

`UserInputSubmittedNotification` 的目标含义改为：

```text
这是一条进入模型 user-role 通道的输入。
submission_kind 表示 prompt / steer。
source 表示 channel/source provenance。
```

`UserInputSubmitted` 不再等价于“人类用户本人输入”。人类用户只是 `source.namespace=core, kind=composer, actor=user` 的一种。

### Companion Mapping

推荐映射：

| Companion 场景 | source namespace | kind | actor | route | model role |
| --- | --- | --- | --- | --- | --- |
| child initial dispatch | companion | dispatch | agent/system | sub | user |
| child result to parent | companion | result | agent | parent | user |
| parent request | companion | parent_request | agent | parent | user |
| parent response to child | companion | parent_response | agent | child | user |
| human response | companion | human_response | human | human | user |
| parent resume / legacy continuation | companion | parent_resume | agent | parent | user |

这些输入可以带 gate id / request id / dispatch id：

- `source_ref`: gate id、dispatch id 或 durable fact id。
- `correlation_ref`: request id、dispatch id 或 channel delivery id。
- `metadata`: bounded result refs、status、timed_out、review/adoption hint 等结构化事实。

### System Delivery Scope

`system_delivery` context frame 继续服务平台/运行时控制事实，例如真正的 runtime context update、某些 hook auto-resume 控制投影、context compaction 触发等。

判断依据不是 origin，而是这条内容是否应该拥有 system authority。Companion 协作内容不满足这个条件。

## Backend Design

### Backbone Protocol

修改 `crates/agentdash-agent-protocol/src/backbone/user_input.rs`：

- 新增 `UserInputSource` 或 `UserInputChannelSource`。
- `UserInputSubmittedNotification` 增加 `source` 字段。
- `new(...)` 构造器接收 source。
- 增加 helper：
  - `UserInputSource::core_composer()`
  - `UserInputSource::local_relay_prompt()`
  - `UserInputSource::from_launch_source(...)` 作为非 mailbox launch 的兜底来源。
  - 若 crate 依赖不允许直接依赖 domain，则在 application 层提供 `MailboxSourceIdentity -> UserInputSource` mapper。

项目未上线，因此生成的 TypeScript 可以直接更新，不需要保留旧字段兼容。

### Launch Command / PreparedTurn

需要让 runtime-session commit 拿到 input source。推荐在 `agentdash-application-ports::launch` 增加中立类型：

```rust
pub struct LaunchInputSource {
    pub namespace: String,
    pub kind: String,
    pub source_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub actor: String,
    pub route: Option<String>,
    pub display_label_key: String,
    pub metadata: Option<serde_json::Value>,
}
```

`LaunchCommand` 增加 `input_source: Option<LaunchInputSource>` 和 `with_input_source(...)`。

来源设置：

- HTTP / lifecycle user message: `core/composer/user` 或 route 指定来源。
- local relay: `core/local_relay_prompt/user`。
- AgentRun mailbox delivery: 从 `AgentRunMailboxMessage.source` 映射。
- Companion parent resume legacy command: `companion/parent_resume/agent/parent`。
- Workflow / hook / system 保留现有 system projection，除非调用方显式设置为 user-role input source。

`PreparedTurn` 增加 `input_source`，`TurnCommitter` 直接使用它构建 `UserInputSubmitted`。

### Turn Preparation

把当前 `should_remain_human_prompt` 改为更准确的策略函数，例如：

```text
launch_input_delivery = UserInput | SystemDeliveryContext
```

推荐首轮规则：

- `HttpPrompt | LifecycleAgentUserMessage | LocalRelayPrompt`: UserInput。
- `CompanionDispatch | CompanionParentResume`: UserInput。
- `SystemDelivery | HookAutoResume | WorkflowOrchestrator | RoutineExecutor | ContextCompaction`: 保持现有 system/control projection，除非后续任务明确重分级。
- `<subagent_notification>` marker 保持当前专门处理，后续可在 subagent/channel 任务里迁移为 channel user input 或 typed platform event。

当 delivery 为 UserInput 时：

- 不创建 `system_delivery` context frame。
- 不替换 `resolved_payload.prompt_payload`。
- 让 connector 直接接收原始 `PromptPayload::Input`。

### Turn Commit

`commit_accepted_launch_events` 使用新的策略：

```text
UserInput delivery -> BackboneEvent::UserInputSubmitted { source, content, ... }
System/control delivery -> PlatformEvent::SessionMetaUpdate(system_message)
```

命名上不要继续使用 `should_persist_as_human_user_input`，建议改为 `should_emit_user_input_submitted` 或 `input_delivery_event_kind`，原因是 human 与 user-role 是不同概念。

### Mailbox Scheduler

`AgentRunMailboxService` 的 launch 与 steer 投影都要同步：

- 对 `MailboxMessageOrigin::User | Companion` 且消息语义为 Agent-facing input 的 delivery，发 `UserInputSubmitted`，source 来自 `message.source`。
- 对 hook/system/workflow 中真正的控制投影，保留 `system_message`。
- `emit_user_input_submitted` port 需要接收 source；所有调用点提供明确来源。

这一步是关键，否则 Companion launch 可能修正了，running turn steering 仍会作为 system projection 出现。

### Transcript Restore

`transcript_restore.rs` 当前已经把 `BackboneEvent::UserInputSubmitted` 恢复为 `AgentMessage::User`。添加 source 字段后无需改变模型恢复主逻辑，但要补测试确认：

- companion source 的 `UserInputSubmitted` 被恢复为 user-role message。
- source metadata 不进入模型正文，除非 payload 本身包含 bounded projection 文本。

## Frontend Design

### Read Model

前端为 `user_input_submitted` 增加轻量 view model：

```ts
type SessionInputSourceView = {
  namespace: string;
  kind: string;
  actor: string;
  route?: string | null;
  label: string;
  presentation: "user" | "companion" | "channel";
};
```

label 规则复用 workspace mailbox 已有 `SOURCE_LABELS` 思路：

- `mailbox.source.core.composer` -> 用户输入
- `mailbox.source.companion.dispatch` -> Companion 派发
- `mailbox.source.companion.result` -> Companion 结果
- `mailbox.source.companion.parent_request` -> Parent 请求
- `mailbox.source.companion.parent_response` -> Parent 回应
- `mailbox.source.companion.human_response` -> 用户回应
- unknown namespace/kind -> format fallback

### Rendering

`SessionEntry.tsx` 对 `user_input_submitted`：

- `source.namespace === "companion"` 时，渲染 `SessionMessageCard` 的 companion/channel variant，带来源 badge。
- `source.actor === "user" && source.namespace === "core"` 时，沿用普通 user 样式。
- 其他 channel source 可使用中性 channel 样式。

`systemEventPolicy.ts` 不再需要把 `companion_delivery` 作为 `system_message` 渲染目标；实现后这类事件不应再产生。

`useSessionFeed.ts` 的 turn segmentation 可以继续把 `user_input_submitted` 作为 turn 前硬边界。Companion input 也是开启/续跑 Agent 的输入，应独立成段，但视觉上不说成“你说”。

## Spec Updates

需要更新：

- `.trellis/spec/cross-layer/backbone-protocol.md`
  - `UserInputSubmitted` 表达 user-role input，不等价于 human-only。
  - source/channel provenance 是前后端共同消费字段。
- `.trellis/spec/backend/session/agentrun-mailbox.md`
  - 修订 non-user mailbox delivery 一律 system projection 的旧结论。
  - 说明 Companion Agent-facing delivery 进入 `UserInputSubmitted`，source identity 提供 provenance。
- `.trellis/spec/backend/session/execution-context-frames.md`
  - 说明 context frame 的 system channel 只用于规则/事实投影，不承载 Companion 协作输入。
- `.trellis/spec/frontend/type-safety.md` 或相关前端规范
  - 说明 generated Backbone source 字段为 UI 差分事实源，不手写并行 DTO。

## Trade-offs

### 为什么不先引入完整 ChannelMessage Backbone 事件

完整 `ChannelMessageSubmitted` 可以更彻底，但它会同时牵动外部 binding、channel registry、消息持久化、provider outbox 和前端 timeline。当前 bug 的最小正确修复是：进入模型 user-role 的内容仍使用 `UserInputSubmitted`，并补齐 channel/source provenance。

这能立即修复模型 authority，同时不阻塞未来把更多消息映射到完整 channel event。

### 为什么不只改前端

只改前端仍会让模型看到 system frame，核心问题没有解决。后端必须停止把 Companion 内容写入 system model channel。

### 为什么不把所有 non-user 都改成 user input

Hook、workflow、system delivery 中存在真正的运行期控制事实。它们是否进入 user-role 需要逐类判断。本任务先修 Companion 这类明确要求 Agent 响应的协作输入，并建立 source-aware 机制。

## Validation Strategy

后续实现至少覆盖：

- Rust protocol serialization test：`UserInputSubmittedNotification` 含 source 字段并生成 TS。
- Runtime-session preparation test：Companion launch 不产生 `system_delivery` context frame，不替换 prompt payload。
- Runtime-session commit test：Companion parent resume emit `UserInputSubmitted`，source 为 companion。
- Mailbox scheduler test：Companion launch / steer emit source-aware `UserInputSubmitted`；system/hook 控制投影仍保留 system path。
- Transcript restore test：companion source user input 恢复为 `AgentMessage::User`。
- Frontend reducer/render test：companion `user_input_submitted` 显示为 Companion/channel 输入，不走 `SessionSystemEventCard`。
- Generated binding check：`cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts` 后无 drift。
