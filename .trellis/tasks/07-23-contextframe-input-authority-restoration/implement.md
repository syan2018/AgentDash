# ContextFrame 模型输入单一权威与热更新链路实施记录

## Phase 0: Contract And Audit

- [x] 冻结“上游 typed facts / ContextFrame readable delivery / provider-native structured tools”三层边界。
- [x] 对照 main-reference 与历史任务，定位本分支两次删除 context projection、随后在 Dash system prompt 新增独立 ToolSchema 摘要的回归时间线。
- [x] 枚举主 Agent provider request constructor、平台 PromptText renderer、initial context、compaction、canonical projection 与前端展示路径。
- [x] 排除用户输入、assistant/tool history、命名与 compaction summarizer 等并非平台上下文旁路的 lane。

## Phase 1: Accepted ContextFrame Model

- [x] Native adapter 在 surface/initial-context 接纳边界生成完整 ContextFrame，Dash history 无损保存 exact frame。
- [x] accepted surface 同时保留 machine tool definitions、typed provenance 与 readable ContextFrame。
- [x] frame id、cache key/revision 与 surface/context revision 稳定关联。
- [x] canonical projection 直接发布已接纳 frame，不再重渲染 instruction、ToolSchema、initial context 或 compaction 文本。
- [x] Dash按accepted delivery metadata消费模型输入：stable frame直接物化，capability delta通过
  `SystemAppend` ledger物化；presentation/audit事件不再伪装成另一份输入事实。

## Phase 2: Context Delivery Materializer

- [x] Dash 从已接纳 frame 按 delivery phase/order/revision identity 确定性物化 provider-agnostic context preamble。
- [x] identity、guidelines、environment、user context、assignment、memory 与 capability context 全部消费 exact `rendered_text`。
- [x] initial context 的标题包装与正文只在 ContextFrame 接纳时渲染一次。
- [x] 当前 Dash 生产路径不存在独立 pending/hook/auto-resume PromptText producer；审计确认无需制造第二套注入接口。
- [x] surface/initial/compaction 通过 native history fold 恢复，不增加跨 owner ledger。

## Phase 3: ToolSchema Authority

- [x] `RuntimeToolDefinition`、Complete Agent surface payload、static platform registry 与 dynamic MCP discovery 全链路保留 capability/source/tool_path/context_usage_kind provenance。
- [x] 实现仓库唯一的确定性、nested-schema-aware ContextFrame ToolSchema readable renderer。
- [x] initial 使用 empty→current delta；hot update 生成 added/removed/changed delta。
- [x] tool identity 使用 capability/source/tool_path/name，不以 name 作为唯一身份。
- [x] 删除 Dash 与 platform-spi 中的替代 readable formatter。
- [x] provider `tools[]` 从同一 accepted definitions 原样派生，provider 不追加任何工具 PromptText。

## Phase 4: Provider-Round Refresh

- [x] Core 从 immutable turn context 改为每个 provider round 重新读取 accepted context/tools。
- [x] tool result 或 surface/context mutation 提交后，下一 provider request 立即刷新。
- [x] 同一 round retry 固定 snapshot，不混入中途变更。
- [x] 已接纳调用使用发起 round 的 projector/callback 完成；后续新调用使用新 tool set 与新 binding。
- [x] 覆盖 apply/revoke、changed/removed tool 与同一 turn 内热更新。

## Phase 5: Compaction And Usage

- [x] runtime compaction summary 作为 exact `CompactionSummary` ContextFrame 保存并驱动后续 provider input。
- [x] provider usage 基于最终 request 返回，因而包含已物化 ContextFrame；structured tools 继续以独立 machine contract 计入 provider request。
- [x] readable ToolSchema 与 structured tools 共享 typed identity/provenance，但不互相重渲染。
- [x] ContextOverflow 触发的 compaction 使用包含最新 frame/tool surface 的真实 provider request。
- [x] 验证 compaction summary 在 history/canonical/provider 三条路径保持同一文本。

## Phase 6: Frontend And Canonical Presentation

- [x] ToolSchema item 恢复完整 `parameters_schema` 可展开 JSON tree。
- [x] “Agent 实际原文”展示 exact accepted `rendered_text`。
- [x] frame id、cache revision、delivery metadata 与 surface revision 保留在 canonical payload 中；provider round snapshot 由 Dash 执行测试验证。
- [x] 更新 ContextFrame render tests 与 canonical fixtures。
- [x] 删除“只展示工具数量与描述”的浅投影合同。

## Phase 7: Specs, Migration And Validation

- [x] 更新 Native Adapter、Runtime Context、Tool Capability 与 Backbone specs。
- [x] provider adapter 守卫覆盖 Anthropic、OpenAI Responses、OpenAI Completions 与 Codex Responses。
- [x] focused Rust suites、schema generators、runtime wire/service API tests 与 Product tracer 通过。
- [x] focused frontend type/lint/tests 通过。
- [x] 新增 pre-release migration 清理旧 Dash owner documents/effects；没有兼容 reader 或 fallback 分支。
- [x] 仓库级前端 `typecheck` 通过；全量 ESLint 仍被 33 个本任务外既有 React effect 规则错误阻断，按工作区约束未修改无关文件。

## Phase 8: Capability Append Regression Closure

- [x] 将capability manifest维度与ToolSchema delta合并为单一CAP ContextFrame。
- [x] Dash CAP delivery固定为`context/system_append`，移除该路径的connector-native/consume语义。
- [x] update frame过滤未变化section，未变化工具schema不再全量重放，no-op revision不生成空CAP。
- [x] added/changed工具由平台renderer输出字段说明与无损完整JSON Schema。
- [x] provider round按native history累积当前active surface的append frames，surface revoke清空链路。
- [x] 前端Dash CAP fixtures同步为`system_append`并继续直接展示accepted `rendered_text`。
- [x] 完成focused Rust/frontend验证与重复缺陷复盘；任务随本次修复提交完成后归档。

## Phase 9: Stable Snapshot / Canonical Transition Separation

- [x] 红测复现tool-only revision把8条stable/CAP frames全量发布的问题。
- [x] canonical projector只接收history fold中的previous surface与current entry，过滤语义未变化的
  stable frames，不复制完整turn/item state。
- [x] read、changes与durable live三条路径共用同一previous/current projection。
- [x] provider继续从current surface读取完整stable snapshot，不因timeline去重丢失模型上下文。
- [x] Complete Agent focused regression与完整native integration crate测试通过。
- [ ] 等待真实页面复验后再决定任务归档与finish journal。

## Primary Files

- `crates/agentdash-agent/src/dash/history.rs`
- `crates/agentdash-agent/src/dash/service.rs`
- `crates/agentdash-agent/src/dash/core_execution.rs`
- `crates/agentdash-agent-core/src/loop_engine.rs`
- `crates/agentdash-integration-native-agent/src/service.rs`
- `crates/agentdash-integration-native-agent/src/canonical_projection.rs`
- `crates/agentdash-integration-native-agent/src/bridge_execution.rs`
- `crates/agentdash-infrastructure/src/complete_agent_product_provisioning.rs`
- `crates/agentdash-agent-protocol/src/backbone/context_frame.rs`
- `crates/agentdash-llm-provider/src/*`
- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/features/session/ui/ContextFrameBody.tsx`
- `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx`

## Validation Focus

- 同一 frame text 进入 provider request与canonical presentation。
- active tool更新后的下一 provider round可见新context/tools。
- retry/idempotent apply不重复注入frame。
- schema provenance与nested parameters无丢失。
- initial context与runtime compaction无旁路renderer。
- frontend、usage与compaction使用实际投递事实。
