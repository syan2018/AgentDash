# ContextFrame 模型输入单一权威与热更新链路收复

## Goal

恢复 ContextFrame 作为平台拥有的模型可见上下文的唯一可读渲染与投递协议，使工具说明、身份与规则、任务上下文、初始上下文、能力变化和压缩摘要都由同一份已接纳 ContextFrame 同时驱动：

- Dash 的真实模型输入；
- 运行中上下文与工具热更新；
- Complete Agent native history；
- canonical ContextFrame timeline；
- 前端“Agent 实际原文”；
- context usage 与 compaction 评估。

provider 原生 `tools[]` 继续承载函数调用所需的机器契约，但 provider bridge、Dash Core 和各 provider adapter 都不拥有可读 ToolSchema PromptText 的渲染职责。

## User Value

- 平台能够精确回答 Agent 实际看到了什么，而不是用事后投影近似猜测。
- 工具 schema、MCP provenance 与能力热更不再受具体 provider 的隐式上下文策略锁定。
- active tool call 引发 surface 更新后，下一次 provider round 就能看到新的 ContextFrame 与工具集合。
- 前端展示、token 统计、compaction 决策与实际 provider request 不再分别维护不同版本的上下文。

## Confirmed Architecture Decisions

- ContextFrame 是事实域的标准模型投递协议，不取代 `AgentFrame`、CapabilityState、accepted Agent surface、compaction history 等上游事实源。
- 平台拥有的可读上下文只渲染一次。ContextFrame 中被接纳的 `rendered_text` 是模型投递、历史展示和调试展示共用的精确文本。
- provider 原生 `tools[]` 是机器可调用契约，不是可读工具说明的事实源；它必须保留完整 name、description 和 input schema，且不得额外生成 system/developer/user PromptText。
- 初始工具面是从 empty 到 current visible ToolSchema set 的普通 delta，不建立另一套初始 snapshot 渲染分支。
- PiAgent/Main 已验证过的边界继续适用：Application/context delivery owner 负责 ToolSchema PromptText 与 ContextFrame；Agent/provider adapter 只负责工具注册、函数调用和 wire adaptation。
- 项目尚未上线，直接收敛到目标合同，不保留旧 renderer、dual delivery、兼容 reader 或 fallback。

## Confirmed Regressions

### 1. ToolSchema 可读说明绕过 ContextFrame

- Product surface 已提供完整 `name/description/input_schema`。
- Native adapter 将其复制为 `DashToolDefinition`。
- `DashSurface::render_system_prompt()` 又在 Dash 内部生成 `## Runtime Tool Schema`。
- canonical ContextFrame 之后才从 `SurfaceApplied` history 反向生成 `tool_schema_delta`，且 `rendered_text` 只有工具名和 description。

结果是 system prompt 和 ContextFrame 有两个不同 renderer；ContextFrame 并未传入 Dash，只是下游展示投影。

### 2. ContextFrame 热更新失效

Dash 在用户 turn 开始时只调用一次 `materialize_context()`，形成固定的 `DashCoreContext { system_prompt, history, tools }`。Core 的每个 provider round 都 clone 这份静态值。运行中 `apply_surface()` 虽可提交新 surface，下一 provider round 却不会重新物化上下文和工具。

### 3. Surface instructions 同类旁路

identity、system guidelines、environment、user context、assignment、memory 与 capability manifest instruction 先由 Product/Native surface 形成字符串，再由 Dash 直接拼进 system prompt。Native adapter 之后才把同一 instruction 重新包装为 ContextFrame。协议中已有 delivery phase、order、cache policy、model channel 和 consumption mode，但这些字段目前不驱动 Dash 模型输入。

### 4. Initial context 文本不一致

Dash 实际 system prompt 使用 `InitialContextContribution::render_for_prompt()`，额外生成 `## AgentDash Initial Context: ...` 标题；canonical ContextFrame 的 `rendered_text` 却只保存原始 payload。前端“Agent 实际原文”因此不是 Agent 实际收到的原文。

### 5. Runtime compaction summary 同类旁路

完成 compaction 后，Dash 直接把 summary 包装为 `<compacted_context>` 并追加到 system prompt。runtime compaction presentation 没有复用同一份 CompactionSummary ContextFrame，导致模型输入、timeline 与 usage 无法由同一投递事实闭环。

### 6. 前端与测试固化了浅投影

- ToolSchema ContextFrame 仍保留完整 structured `parameters_schema`，但前端只展示参数数量和 description，不提供 schema 展开。
- integration test 明确断言可读 tool delta 不包含 `properties`。
- Dash history test 明确要求独立 system prompt renderer 包含工具参数摘要。

这些断言需要改为证明“只有 ContextFrame renderer 生成可读说明，并且同一文本确实进入 provider request”。

## Similar-Case Audit Boundary

本任务纳入所有由平台拥有、会进入主 Agent provider request、却未由 accepted ContextFrame 驱动的上下文：

- intrinsic identity；
- Project/Agent system guidelines；
- environment / workspace；
- user context；
- assignment / workflow / constraints；
- memory context；
- capability manifest、ToolSchema 与 MCP/tool surface delta；
- initial context package；
- runtime compaction summary；
- pending action / auto resume / hook add-context 等运行时上下文（若生产路径存在）。

以下通道语义不同，不改造成 ContextFrame：

- 用户输入和明确的 `steer`：它们属于 native conversation input；
- assistant message、tool call 与 tool result：它们属于 native conversation history；
- conversation naming、compaction summarizer 自身的内部 provider job：它们不是主 Agent turn 的上下文；
- provider 原生 structured `tools[]`：它是函数调用机器合同。

Codex/Remote 等外部 Complete Agent adapter 纳入合同审计和守卫测试；只有平台实际拥有其可读上下文投递时才使用同一 ContextFrame 合同，不把外部 Agent 自有的 native prompt/history 强行复制到 Dash 模型中。

## Requirements

### R1. ContextFrame 成为可执行投递合同

- ContextFrame 的 delivery phase/order 决定 provider-agnostic preamble 顺序，cache identity 决定 revision 身份，consumption mode 决定是否消费；model channel/message role 作为 frame 语义保留，不能下放给 vendor provider 重新解释。
- accepted surface/history 中保存或可无损折叠出“实际被消费的同一份 ContextFrame”；canonical projection 直接发布该 frame，不再次渲染自然语言。
- `rendered_text` 与真实 provider-visible text 必须字节级一致；展示层不得再使用“Agent 实际原文”描述未被 Agent 消费的浅摘要。

### R2. ToolSchema 单一 renderer

- 删除 Dash/provider 侧独立 ToolSchema PromptText renderer。
- 从同一 accepted tool surface 同时派生：
  - provider structured `tools[]`；
  - ContextFrame `ToolSchemaDelta` structured section；
  - ContextFrame 唯一可读 ToolSchema `rendered_text`。
- renderer 至少表达 tool name、description、capability/source/tool path、required、type、enum、array/object nested fields；完整 structured schema 保留在 section 中。
- schema 超出支持边界时必须在 admission 或 rendered text 中显式表达，不能静默省略关键字段。
- built-in、platform MCP 与 project/dynamic MCP 使用同一 renderer，并保留真实 provenance。

### R3. 每个 provider round 刷新上下文

- 在每次主 Agent provider request 前重新读取当前 accepted surface/context。
- surface/tool/context 在 active tool call 中更新后，下一 provider round 必须使用新 revision。
- 当前已接纳 tool call 继续按其原 binding/generation 完成；新工具集合只约束后续 provider request 与新 tool call。
- ContextFrame delta exactly-once 进入应消费的上下文历史，不因多个 provider round 重复注入。

### R4. 收束所有平台拥有的模型上下文

- identity/guidelines/environment/user-context/assignment/memory 不再由 Dash 直接拼字符串后再投影 ContextFrame。
- initial context 的标题、正文与 channel 由 ContextFrame renderer 一次确定。
- runtime compaction summary 由同一 CompactionSummary ContextFrame 同时驱动模型恢复、timeline 和 usage。
- hook/pending/auto-resume 若产生平台拥有的可读上下文，必须先接纳为 ContextFrame 再进入同一 materializer；当前 Dash 生产路径审计未发现独立 PromptText producer。

### R5. provider 与 Agent Core 保持纯适配

- provider bridge 只把结构化 system/messages/tools 映射为 vendor wire request。
- Anthropic、OpenAI Responses、OpenAI Completions、Codex Responses 等 provider adapter 不生成可读 ToolSchema 或平台上下文说明。
- Dash Core 不持有面向某类 ContextFrame 的 renderer；它只在 provider round 边界消费已物化的 context snapshot。

### R6. 可观察性与 context management 对齐

- 前端恢复 ToolSchema structured schema 展开，并继续展示唯一 `rendered_text`。
- provider-visible usage 与 compaction pressure 基于最终 materialized request；structured tools 与 readable ToolSchema 分责并共享 typed identity，不建立第二份上下文事实。
- canonical payload 保留 surface revision、ContextFrame id/cache revision 与 delivery metadata；Dash round snapshot 测试证明实际 provider round 的消费 revision。

### R7. 规格与回归保护

- 修正把 ContextFrame 定义为“仅 downstream presentation”的 specs。
- 删除把 Dash system prompt tool renderer 视为正确架构的断言。
- 建立从 Product accepted surface 到 provider request、native history、canonical ContextFrame、frontend renderer 的纵向测试。
- 复用 main-reference 中已经验证的 ToolSchema formatter、turn-start/transform-context 与 per-provider-round refresh 语义；按当前 Complete Agent authority 重组，不机械恢复已删除模块。

### R8. Dash Capability Append 合同

- 同一次surface transition的capability manifest维度与`ToolSchemaDelta`必须合并为一个
  `CapabilityStateDelta` ContextFrame，不能拆成两个CAP投递事实。
- Dash capability frame固定使用`model_channel=context`与`consumption=system_append`；connector
  不拥有工具上下文的native注入策略。
- initial surface按empty→current生成完整工具schema；后续surface只渲染真实变化的section和
  added/changed schema，未变化工具不能重放，语义无变化不能生成空CAP frame。
- 每个provider round从native history恢复当前active surface链的initial append与后续delta；
  surface revoke清空该链，避免已失效schema继续进入模型。

## Acceptance Criteria

- [x] 仓库中只有一个平台可读 ToolSchema renderer，且不位于 provider、bridge、Dash Core 或前端。
- [x] 初始 tools 从 empty→current 生成包含完整 structured schema 与可读参数说明的 ContextFrame。
- [x] 同一 accepted tool definition 原样进入 provider `tools[]`，但 provider system/developer/user PromptText 不额外内嵌工具说明。
- [x] provider 实际收到的 ToolSchema PromptText 片段与对应 ContextFrame `rendered_text` 字节级一致。
- [x] active tool 触发 surface/tool update 后，下一 provider round 同时看到新 ContextFrame 和新 structured tool set。
- [x] removed/changed tool delta exactly-once 接纳，不在 native history 中重复，也不等待下一次用户输入。
- [x] identity、guidelines、environment、assignment、memory、initial context 与 runtime compaction summary 不再存在 ContextFrame 之外的独立可读 renderer。
- [x] native history/read/changes 与 live canonical stream 发布的 ContextFrame 是 Agent 实际接纳的同一对象或无损序列化结果。
- [x] 前端 ToolSchema item 可展开查看 parameters schema，且“Agent 实际原文”确实对应模型输入。
- [x] provider usage 与 compaction trigger 基于包含全部 accepted frame 的最终 request；structured tools 与 PromptText 分责并共享 typed identity。
- [x] provider adapter 守卫测试覆盖 Anthropic、OpenAI Responses、OpenAI Completions 与 Codex Responses，证明它们只做结构映射。
- [x] focused Rust、frontend tests 与真实 Product→Dash→provider tracer 通过。
- [x] 相关 Trellis specs 使用最终的 ContextFrame input authority、tool machine contract 与 hot-update 边界。
- [x] capability manifest与ToolSchema在Dash中只形成一个`context/system_append` CAP frame。
- [x] 后续CAP frame只含真实变化的section；added/changed工具由平台渲染无损完整JSON Schema。
- [x] provider round累积当前active surface的append历史，surface revoke后不再保留旧工具说明。

## Scope Notes

- 本任务是跨 Agent service API、Native Agent、Dash Core、provider bridge、canonical projection、frontend 与 specs 的架构收敛任务，实施前必须完成 `design.md` 与 `implement.md` 审阅。
- 本任务不与 `07-20-agent-runtime-persistence-authority-convergence` 争夺持久化 owner；concrete Agent 仍拥有 native history/context。这里只修正该 owner document 内“实际模型输入”和 ContextFrame 的关系。
- 预计不需要数据库 schema 变更；若 accepted ContextFrame 必须进入现有 Dash owner document 的新字段，则使用当前 forward migration 清理开发态数据，不提供旧 shape 兼容读取。
