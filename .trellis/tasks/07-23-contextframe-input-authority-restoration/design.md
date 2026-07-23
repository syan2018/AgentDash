# ContextFrame 模型输入单一权威与热更新链路设计

## 1. Architecture Boundary

ContextFrame 是平台事实进入模型的标准投递协议，不是能力、workspace、assignment 或 history 的原始事实源。

```text
Upstream facts
  AgentFrame / CapabilityState / accepted surface / compaction history
        |
        v
ContextFrame materializer
  structured sections + one rendered_text + delivery metadata
        |
        +----------------------+-----------------------+
        |                      |                       |
        v                      v                       v
Dash provider context   canonical history/live   frontend/usage/debug
        |
        v
Provider wire adapter
  native messages + native structured tools[]
```

provider structured `tools[]` 与 ContextFrame PromptText 是同一 accepted tool surface 的两个不同投影：

- structured tools 服务函数调用；
- ContextFrame 服务平台拥有的可读能力上下文、热更新、审计和展示。

二者共享 typed source，但只有 ContextFrame 拥有可读渲染。

## 2. Current Broken Flow

```text
BoundAgentSurface
  -> DashSurface.instructions/tools
      -> DashSurface::render_system_prompt()       # 可读工具说明与其它 instruction
      -> immutable DashCoreContext                 # turn 开始时只物化一次
      -> provider rounds clone static prompt/tools

Dash SurfaceApplied history
  -> Native Adapter canonical_projection
      -> rebuild ContextFrame                      # 事后再次渲染
      -> shallow Tool Surface Delta
      -> frontend "Agent 实际原文"
```

问题不是 structured tools 存在，而是可读上下文拥有两个 renderer，且 ContextFrame 没有参与 provider input。

## 3. Target Accepted Context Model

Native adapter 在接纳 bound surface/initial context/runtime context update 时，生成一组 typed accepted ContextFrame。精确类型命名可在实现中调整，但必须表达以下结构：

```rust
struct AcceptedDashContext {
    surface_revision: u64,
    surface_digest: String,
    frames: Vec<ContextFrame>,
    tools: Vec<DashToolDefinition>,
}
```

约束：

- `frames` 保存最终 `rendered_text`、sections 与 delivery metadata。
- `tools` 只保存 provider/tool registry 所需的机器定义和 protocol projector。
- Dash history 保存能够无损恢复这两部分的 accepted entry。
- canonical projection 直接发布 accepted `frames`，不调用第二套自然语言 formatter。
- ContextFrame id/cache revision 与 accepted surface/context revision 建立稳定关联。

`DashSurface` 是否直接包含 frames，或拆成独立 `AcceptedContextApplied` history payload，由实现设计决定；不得让 canonical projector 和 provider materializer分别重建文本。

## 4. ContextFrame Materialization

### 4.1 Stable/system domains

下列 frame 按 `delivery_phase + delivery_order + created_at + frame_id` 排序并物化到 provider-agnostic context preamble；vendor bridge只接收物化结果，不解释 ContextFrame：

- identity；
- user context；
- environment；
- system guidelines；
- 稳定 capability/tool context。

实际 provider request 中出现的每段平台上下文必须直接来自 frame `rendered_text`。`message_role` 与 `model_channel` 保留为 frame 语义和后续 provider-independent materialization依据，不变成各 vendor adapter 的分叉规则。

### 4.2 Context domains

下列 frame 与 stable frame 共用同一 accepted-context materializer，并在 native history 中拥有可恢复的投递边界：

- assignment；
- memory；
- initial context；
- compaction summary；
- capability/tool delta；
- hook add-context、pending action、auto resume（仅在未来存在对应平台 PromptText producer时）。

首次投递与运行中 delta 共享同一 delivery mechanism。empty→current 只是普通 before/after delta。

### 4.3 Cache 与 exactly-once

- Stable/static frame 由当前 cache key/revision 构成每轮 system materialization，可重复物化但不形成重复 conversation message。
- Assignment/discovery/runtime delta 按 frame id 与 cache revision成为一次 accepted history事实；每轮可从当前 fold 重建 request，但不重复提交 history。
- provider retry 对同一 provider round 使用同一 materialized snapshot。
- 下一 provider round 才观察新的 accepted surface/context revision，避免在一个已经发出的 wire request 中途改变语义。
- context consumption evidence 属于 concrete Agent history fold，不建立 Product/Runtime 第二账本。

### 4.4 Dash capability append ledger

Dash把一次surface transition的capability manifest section与`ToolSchemaDelta`合并为一个
`CapabilityStateDelta` frame。该frame的模型通道固定为`context`、消费模式固定为
`system_append`，因为平台需要独立决定工具上下文何时初始注入、何时追加delta以及何时撤销，
connector只负责消费已物化的provider request。

```text
SurfaceApplied revision 1
  -> CAP append(empty -> current, full added schemas)
SurfaceApplied revision 2
  -> CAP append(previous -> current, changed sections only)
SurfaceApplied revision 3 without capability changes
  -> no CAP frame
SurfaceRevoked
  -> clear active surface append ledger
```

每个provider round从native history按提交顺序恢复当前active surface链的append frames，并与当前
stable frames一起物化。最新surface不重放完整工具快照；这既保持delta语义，也让canonical timeline
中的同一frame序列逐字对应模型输入。

## 5. ToolSchema Projection

### 5.1 One typed source

```text
RuntimeToolDefinition / dynamic MCP discovery
  -> accepted DashToolDefinition + provenance
      -> provider ToolDefinition
      -> RuntimeToolSchemaEntry
          -> ToolSchemaDelta section
          -> one readable renderer
```

当前 `DashToolDefinition` 缺少 capability/source/tool_path 等 provenance。目标结构需要在 accepted surface 中保留这些 typed 字段，不能仅靠 `name.starts_with("mcp_")` 猜测。

### 5.2 Readable renderer

renderer 由 ContextFrame/context delivery owner 持有，输出确定性文本：

- name；
- description；
- capability key；
- source；
- tool path；
- required/optional；
- scalar/object/array type；
- enum/const；
- nested fields 与 array items；
- schema 限制或显式省略原因。

structured `parameters_schema` 原样保留在 section。若为了模型上下文设置最大深度/字段数，超限必须在 schema admission 阶段拒绝，或在文本中输出明确的 truncation marker；不能像当前浅 renderer 一样悄悄只保留参数数量。

### 5.3 Delta semantics

- initial: `before = empty`, `after = current`；
- hot update: accepted previous surface 与 accepted current surface 做稳定 key diff；
- key 至少包含 owner/source/tool path/name，不能只按 runtime name；
- changed 判断覆盖 description、schema 与 provenance；
- removed tool 输出稳定 identity；
- same revision/idempotent apply 不产生重复 frame。
- added/changed schema的可读投影同时包含字段级说明与canonical完整JSON Schema，确保任何未被摘要
  renderer识别的JSON Schema关键字仍由平台无损注入。

## 6. Provider-Round Refresh

当前 `run_agent_loop(input, CoreContext, ...)` 把 context 按值冻结。目标 Core 边界改为每轮获取 snapshot：

```rust
#[async_trait]
trait CoreContextSource {
    async fn materialize_for_provider_round(
        &self,
        turn_id: &AgentTurnId,
        round: u32,
        transcript: &[CoreMessage],
    ) -> Result<CoreProviderContext, CoreError>;
}
```

等价接口也可接受，但执行顺序必须固定：

```text
tool result committed
  -> accepted surface/context mutations committed
  -> materialize current ContextFrames + structured tools
  -> append exactly-once context delta
  -> build provider request
  -> BeforeProviderRequest observation
  -> provider stream
```

已 accepted 的 tool call 继续使用调用时绑定；refresh 只影响下一 provider request 与下一次 tool admission。

## 7. Similar Model-Input Lanes

| Lane | Current state | Target |
| --- | --- | --- |
| intrinsic identity | Dash instruction 直拼，ContextFrame 事后投影 | accepted Identity frame 直接物化 |
| system guidelines | Product string 直拼，ContextFrame 事后投影 | accepted SystemGuidelines frame |
| environment/workspace | Native adapter临时格式化 `## Workspace` | context materializer唯一格式化 |
| assignment/user/memory | instruction string 直拼 | 对应 accepted frame驱动 |
| initial context | Dash header renderer与Frame payload不一致 | 一份 accepted frame text |
| ToolSchema | Dash system renderer + shallow frame renderer | 一份完整 ContextFrame renderer |
| runtime compaction | `<compacted_context>` 直拼 | CompactionSummary frame 驱动恢复 |
| pending/hook/auto-resume | 当前 Dash 无独立 PromptText producer | 未来生产者必须先提交 accepted frame |
| user input/steer | native conversation lane | 保持，不转为ContextFrame |
| tool result | native conversation/tool lane | 保持，不转为ContextFrame |
| naming/summarizer job | internal provider job | 保持独立，不冒充主Agent上下文 |

## 8. Canonical Projection And Frontend

- `canonical_projection` 不再实现 instruction/tool/initial-context readable renderer。
- 投影层只把 Dash history 中 accepted ContextFrame 包装成 `Platform(ContextFrameChanged)`。
- 前端按 structured section 展示摘要，并允许展开完整 `parameters_schema`。
- “Agent 实际原文”只展示确实被 context materializer消费的 `rendered_text`。
- canonical payload保留 frame id、cache revision、surface revision与delivery metadata；provider round消费关系由Dash round snapshot和纵向测试验证。

## 9. Context Usage And Compaction

- provider-visible usage以最终 materialized request 为准。
- ContextFrame 使用 frame id/cache revision 与 accepted surface revision建立关联。
- structured tools 与可读 ToolSchema PromptText 分开归类，但不得用两份独立工具事实重建；UI须能解释两者分别是 machine contract 与 readable context。
- provider返回的实际input usage和ContextOverflow覆盖已物化preamble、structured tools与conversation/tool results。
- compaction 后保留的 summary 本身以 CompactionSummary frame进入后续 request；旧 delta 的折叠由同一 context materializer完成。

## 10. Cross-Adapter Boundary

- Native Dash 是本任务的完整修复目标。
- provider bridges统一增加“无平台 PromptText renderer”守卫。
- Codex/Remote Complete Agent 只审计平台 surface/context 的交付合同；其自身 native prompt/history 仍由该 Agent owner 管理。
- 只有 adapter 声明消费平台 ContextFrame 时，才要求其证明 accepted frame 与实际输入一致；不把外部 Agent 的私有 system prompt复制为平台 ContextFrame。

## 11. Migration

项目未上线。若 Dash Agent-owned document 需要保存新的 accepted frames：

- 使用新的 forward migration 或 owner document schema version；
- 清理开发态旧 document；
- 不实现旧 `DashSurface` 文本 renderer fallback；
- 不 dual-write旧 surface投影与新 ContextFrame。

## 12. Validation Strategy

- unit：ToolSchema renderer、delta key、ordering/cache、model channel materialization；
- Core：每 provider round refresh 与 retry snapshot稳定性；
- Native integration：surface apply、active tool hot update、initial context、compaction；
- provider adapter：structured tools原样映射且不追加可读说明；
- canonical projection：history frame与live/read frame相同；
- frontend：schema展开与实际原文；
- tracer：Product surface → Dash accepted history → provider request → tool callback surface update → next provider round → canonical live/read → UI。
