# ContextFrame 事实域收束重构

## Goal

将会话运行时上下文收束为清晰的事实域与标准 ContextFrame 投递面，使模型可见上下文、前端调试视图、上下文用量统计和执行侧工具消费都从同一事实源派生。

本任务跟踪一次完整架构重构，不直接以当前局部补丁为边界。重构完成后，ContextFrame 不再作为旧 Bundle / HookInjection / bootstrap_context 路径的混合容器，而是按事实域表达运行时上下文。

## User Value

- 前端 CAP 卡片能准确表达当前能力事实，而不是把 sparse delta 误认为完整状态。
- companion roster 只从 `CapabilityState.companion.agents` 派生，执行侧、模型上下文和 UI 展示一致。
- ProcedureContract 的不同字段按语义进入对应 frame：能力配置进入 CAP，任务指导进入 assignment，hook rules 进入 hook/pending/trace，端口合同进入 workflow/task delivery 面。
- 上下文用量统计覆盖实际进入模型的内容，避免 rendered_text 已投递但 usage 漏算。

## Confirmed Facts

- `CapabilityState.companion.agents` 已是 runtime companion roster 的真实事实源；`companion_request` 工具也消费这份 roster。
- `capability_state_update` 当前是 sparse delta frame；前端 CAP 卡只展示后端当次发出的 section，不会凭空展示未发出的 MCP、ToolPath、Companion 等维度。
- `assignment_context` 当前汇聚 `ContextFragment` / `HookInjection` 任务语义，包含 workflow guidance、context bindings、requirements、constraints 等 ProcedureContract 派生内容。
- `system_guidelines` 已承载用户偏好与项目指引。
- `ContextFrame.rendered_text` 既用于模型可见投递，也与 `sections` 一起用于前端展示和统计；这要求每个 section 的事实域边界必须稳定。
- `ContextFrameSection::HookInjection`、`ToolSchema` full section、`RUNTIME_AGENT_CONTEXT_SLOTS`、`bootstrap_context` 命名和 `companion_agents` assignment slot 都需要在本重构中重新定性。

## Requirements

- 定义 ContextFrame 事实域标准：
  - Capability frame 承载能力事实：capability keys、tool paths、MCP、VFS、tool schema、skills、companion roster。
  - Assignment frame 承载任务语义：task/story/project/workflow guidance、context bindings、requirements、constraints、instruction。
  - System guidelines frame 承载用户偏好与项目规则。
  - Pending/action/trace frame 承载运行时控制、待处理动作和审计信息。
- 将 companion roster 的生产、持久化、模型投递、UI 展示、工具消费统一到 `CapabilityState.companion.agents` 派生链路。
- 拆清 CAP snapshot 与 CAP delta 的产品语义，令 initial/bootstrap 与 live transition 的 frame kind 或 section contract 明确表达全量状态或增量变化。
- 收束 ProcedureContract 投影：
  - `capability_config` 进入 capability resolver / `CapabilityState`。
  - `injection.guidance` 与 `context_bindings` 进入 assignment frame。
  - `hook_rules` 进入 hook runtime / pending action / trace。
  - `input_ports` / `output_ports` 进入明确的 workflow/task delivery 表达。
- 清理协议、测试、前端 parser/renderer 和 spec 中已经失去生产者或语义归属的 section / slot。
- 补齐 context usage 统计，使每类进入模型的 ContextFrame section 都有对应 usage item。
- 同步更新 Trellis spec，记录最终事实域契约和数据流。

## Acceptance Criteria

- [ ] 后端协议中每个 `ContextFrameSection` 都有明确生产者、事实源、模型投递规则和前端展示语义。
- [ ] companion roster 不再通过 assignment slot / hook fragment / owner bootstrap 文本片段表达；CAP frame 是唯一结构化投递面。
- [ ] CAP snapshot / delta 语义明确，前端 CAP 卡能区分完整状态与本次变化。
- [ ] `assignment_context` 只包含任务语义和 ProcedureContract 的 assignment 投影，不包含能力事实或系统指引事实。
- [ ] `system_guidelines` 是用户偏好与项目指引的标准投递面。
- [ ] `ContextFrameSection::HookInjection`、`ToolSchema` full section、`RUNTIME_AGENT_CONTEXT_SLOTS` 等残留协议已完成保留/删除/重定义决策并落地。
- [ ] `context_usage_items_from_context_frame` 覆盖所有模型可见 ContextFrame section。
- [ ] 前端 parser/renderer 覆盖后端有效 section，并为未知 section 提供可诊断 fallback。
- [ ] 后端、前端和 Trellis spec 对同一事实域使用一致命名。
- [ ] 关键路径有测试覆盖：owner bootstrap、runtime transition、ProcedureContract projection、frontend ContextFrame rendering、context usage 统计。

## Scope Notes

- 当前任务是单一重构 tracking task，不拆 Trellis child task。
- 具体工作项由本任务下的 `work-items.md` 与 `work-items/` 文档管理和跟踪；这种粒度让 Trellis task 承载完整架构主题，工作项文档承载可独立推进的实施切片。
- 实现前需要先审阅并冻结 `design.md` 中的目标协议。
- 现有未提交局部改动需要在进入实现前重新评估是否纳入本任务或拆成单独预备提交。
