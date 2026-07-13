# 收束 Canonical Runtime 会话展示链路

## Goal

在不恢复 Backbone 双事实、旧 transport 或兼容回退的前提下，让 AgentRun workspace 以 canonical Runtime snapshot/events 为唯一事实源，并重新获得 `features/session` 已有的完整会话 presentation 能力。事实源迁移不得被实现为简化消息 UI 或另起一套平行会话 renderer。

## Background

- `07-10-agent-runtime-architecture-convergence` 将 `SessionChatView` 的生产消息流从 `SessionChatStream/useSessionFeed` 切换到 `AgentRuntimeFeed/useAgentRuntimeFeed`。
- 当前 `RuntimeItemContent` 只保留 `user_message/agent_message/tool_call/tool_result/reasoning/plan/context_*` 等归一化内容；命令、文件变更、MCP、Companion、typed error、usage 和分通道 delta 等 presentation 语义没有形成完整的 AgentDash-owned typed contract。
- `AgentRuntimeFeed` 将 canonical item 压缩为 `role + text + status`，工具输入输出主要通过 `JSON.stringify` 展示；原有 `SessionEntry -> ToolCallCardShell -> toolCardRegistry`、轮次分段、reasoning、context、Companion 等绘制子树失去生产入口。
- 原重构设计要求 vendor DTO 在 adapter 终止，同时允许 Backbone 作为 canonical Runtime events 的 presentation projection；它没有要求替换既有 session presentation。
- 重构前的 `agentdash-agent-protocol` 直接包裹 `codex_app_server_protocol::ThreadItem`，并通过 `AgentDashNativeThreadItem` 做加法扩展；其 `BackboneEvent/BackboneEnvelope` 仍是 AgentDash 自定义事件与 NDJSON envelope，不等同于标准 Codex App Server JSON-RPC wire。
- 重构后的 `RuntimeEventEnvelope` 是包含 canonical thread identity、durable sequence 与 revision 的内部领域 envelope；它比 Codex wire 更适合作为持久化和恢复事实，但当前 Codex adapter 将丰富 `ThreadItem` 压缩为少数 `RuntimeItemContent` 变体，丢失了 presentation 保真度。
- 当前 `agentdash-integration-codex` 实现的是 AgentDash 作为 client/driver 连接 Codex App Server；未发现 AgentDash 对外提供标准 Codex App Server `thread/*`、`turn/*` JSON-RPC server façade。原始“任意兼容 Codex App Server 的前端可直接连接”目标尚未形成完整生产边界。
- 本项目处于预研期，不保留旧 Backbone runtime 作为兼容路径，也不建立 dual-read、fallback 或平行 feed。

## Requirements

- **R1 — 唯一事实源**：AgentRun 会话读取只使用 canonical Runtime snapshot、durable events 与 typed product projection；不得恢复旧 session stream 或 Backbone runtime 事实源。
- **R2 — Schema-generated owned 同构协议**：canonical conversation contract 必须从固定版本 Codex App Server 官方导出的 JSON Schema/TypeScript fixtures 机械生成 dependency-light 的 AgentDash-owned 标准类型，完整保存 session/item/event 标准子集及产品展示语义；AgentDash 扩展通过手写 typed union/profile 做加法表达。禁止人工手抄标准协议、vendor DTO alias 穿透 Runtime、裸 `JsonValue` 文本化或 UI 猜测。
- **R2a — 跨层收束**：本任务必须覆盖 Rust canonical contracts、adapter/runtime 映射、generated TypeScript、frontend projection 与 renderer；不得把当前 canonical item 的信息损失当作前端固定上限。
- **R2b — Codex App Server 兼容空间**：本任务不实现对外 Codex-compatible server endpoint，但恢复后的协议必须保持标准 Codex App Server protocol 的方法/item/event语义可无损投影，并为未来 AgentDash typed extension 与 capability negotiation 留出明确边界。
- **R2c — Envelope 与 payload 分层**：内部 durable Runtime envelope 可以增加 canonical thread identity、sequence、revision、operation、binding 与 recovery 信息，但不得替代、压扁或重新解释 Codex-shaped + AgentDash extension 的会话 payload。
- **R2d — 原协议正确恢复**：以 `af21f9d7c^` 的生产 session presentation 行为和协议表达能力为恢复基线，将原 `codex_app_server_protocol::ThreadItem` 加 AgentDash 扩展的语义迁移到妥善的 owned/versioned contract；允许调整 backend envelope 解析，不允许借机改变老前端 presentation、交互或视觉行为。
- **R2e — Vendor 终止与 conformance**：只有协议生成工具与 `agentdash-integration-codex` 依赖 Codex App Server vendor crate。W1先将workspace内Codex Rust/npm/schema/fixture基线统一升级到官方`rust-v0.144.1`，标准 payload再通过严格schema/serde transcode进入generated owned type，双向可验证且无generic fallback；method/variant admission必须穷举。后续升级时重新生成并以fixture/schema diff强制暴露漂移。
- **R3 — Presentation ownership**：`features/session` 拥有会话 presentation model 与 renderer；AgentRun workspace 负责提供目标、snapshot/events、command availability 与 product projection，不反向拥有第二套会话 UI。
- **R4 — 单一投影链**：建立 canonical Runtime item/event 到 session presentation model 的单一 projection，覆盖 snapshot baseline、durable cursor、具有稳定identity的live transient delta、terminal、interaction 与 target isolation。有限durable replay不得冒充token live stream；重连和active turn replay不能重复追加delta。
- **R5 — 行为完全保持**：消息、reasoning、plan、工具调用/结果、命令执行、文件变更、MCP、Companion、context change/compaction、typed error、usage、轮次状态、fork/round actions 与交互请求必须恢复 `af21f9d7c^` 的生产行为。除读取 backend envelope 等价物的 adapter 边界外，不重写组件结构、视觉语言或产品交互。
- **R5a — 直接恢复组件图**：以 `af21f9d7c^` 为基线直接恢复 `SessionChatStream -> SessionEntry -> ToolCallCardShell/toolCardRegistry`、turn segmentation、thinking/context aggregation 与相关 view model/test。禁止新建“行为等价”renderer；必要改动仅限 generated type 引用、canonical envelope projection 与 transport adapter。
- **R6 — 删除错误分叉**：正确链路接通后删除 `AgentRuntimeFeed` 这套简化 renderer 及无用途的平行 view model；同时清理真正失效的旧 Backbone transport/model，不保留悬空导出和仅由测试维持的生产死代码。
- **R7 — Subagent 上下文**：implement/check manifests 必须注入前端 architecture、state management、component/design language、cross-layer contracts、原 session presentation 基线、canonical Runtime contract与本任务设计；不得只以“删除旧 consumer”作为检查目标。
- **R8 — 行为验收**：测试必须验证产品 presentation parity 与 canonical data flow，不得只验证文本出现、类型检查或旧符号删除。
- **R9 — 可复现协议工具链**：项目内提供无需全局安装的protocol codegen write/check流程，固定Codex revision、schema hash、root type清单与生成器版本；生成Rust/TypeScript可审查、可重建、可在CI检测drift。标准协议生成失败必须回到设计评审，不得改成人工手抄。
- **R10 — Connector 全量桥接**：Codex、Native Agent、Remote Runtime/Relay及未来Integration driver必须把各自source item/event/interaction无损投影到owned conversation protocol。Codex标准事件strict transcode；Native/企业driver显式构造owned类型；Remote只转发完整typed envelope。任何connector不得将unknown item转成AgentMessage、字符串或generic JSON。
- **R11 — Tool owner 投影**：每个进入最终Business Surface/Tool Catalog的工具必须由tool owner声明protocol presentation projector，覆盖call、progress/update、terminal result、error、approval与必要identity。Shell/command、file change/apply patch、fs read/grep/glob、MCP、dynamic tool、Workspace Module/Canvas、Companion、Task/Wait及其它注册工具均纳入inventory；禁止中央mapper按工具名猜测，禁止未声明projector时默认退化为generic tool。

## Acceptance Criteria

- [ ] AgentRun workspace 中只有一条 canonical Runtime session feed 数据链，没有 Backbone runtime dual-read、fallback 或第二套 renderer。
- [ ] `features/session` 的生产入口能够绘制用户/Agent 消息、流式 reasoning、plan、typed tool call/result、命令执行、文件变更、MCP、Companion、context 与 interaction。
- [ ] `SessionChatView` 的生产消息入口重新使用恢复后的原 session presentation 组件图，而不是 `AgentRuntimeFeed` 或另一套替代 renderer。
- [ ] typed presentation 所需字段由 generated AgentDash-owned contracts 提供；UI 不从工具名、字符串或无 schema JSON 猜测 item 类型和状态。
- [ ] `agentdash-agent-runtime-contract` 及 Managed Runtime 不依赖 `codex-app-server-protocol`；Codex vendor dependency只存在于protocol codegen工具与Codex Integration adapter。
- [ ] 标准 Codex session/item/event 代表性 fixtures 在 vendor type 与 owned type 间保持 JSON 结构等价；所有受支持 variant 使用穷举转换，无 catch-all 文本化。
- [ ] Workspace内Codex Rust crates、npm依赖、protocol revision检查、schema与fixture基线全部对齐`rust-v0.144.1`，不存在残留`0.140.0` pin。
- [ ] 标准协议Rust/TypeScript产物可由单一命令从pinned Codex exporter重建，check mode能够检测手工修改、漏生成和上游schema漂移。
- [ ] Rust contract、runtime/adapter、generated TypeScript 与 frontend projection 对同一 typed item/event 语义形成端到端测试。
- [ ] 标准 Codex App Server protocol 的未来投影边界与 AgentDash extension seam 已明确；本任务不要求提供可连接的外部 server endpoint。
- [ ] snapshot baseline 与 live durable events 在重连、重复事件、cursor gap、target 切换和 terminal 场景下保持顺序、去重与状态一致。
- [ ] live transient delta具有明确的stream generation与transient identity/sequence；重连不会重复追加，final durable item仍是authoritative projection。
- [ ] 以 `af21f9d7c^` 为基线的会话 presentation 关键交互、组件结构与视觉信息层级有行为回归测试；不得用通用文本卡片替代专有 renderer。
- [ ] `AgentRuntimeFeed` 简化绘制路径被删除，原 `features/session` 悬空生产子树被接回或完成 canonical 化后有依据地清理。
- [ ] implement agent 与 check agent 使用不同且完整的上下文清单；check 明确执行 UI parity、跨层数据流和死代码审计。
- [ ] 前端 typecheck、相关 Vitest、generated contract check 与代表性 AgentRun workspace E2E 通过。
- [ ] 新开发环境只需workspace标准Rust/Node依赖即可运行protocol codegen；不要求手工安装全局CLI。
- [ ] 最终Host inventory中的每个Agent runtime driver通过conversation projection conformance；Codex、Native、Remote代表性E2E均保留typed item/delta/interaction语义。
- [ ] 最终Tool Catalog中的每个tool contribution都有显式projector与代表性call/update/result测试；缺失projector在surface compile/admission阶段失败，不进入运行时。
- [ ] Shell、file change、fs、MCP、dynamic、Workspace Module、Companion与Task工具在Runtime snapshot/events和原frontend renderer中落入正确typed variant/card，无工具名猜测或generic fallback。

## Out of Scope

- 恢复已删除的旧 RuntimeSession、Connector 或 Backbone authoritative runtime。
- 提供旧 schema、旧 API 或旧 feed 的兼容层与回退路径。
- 与 canonical session presentation 无关的 AgentRun workspace 视觉重设计。
- 直接暴露 Codex vendor DTO 作为 Application 或 frontend canonical contract。
- 在本任务中实现或发布对外 Codex App Server JSON-RPC server endpoint。

