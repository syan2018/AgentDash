# Workspace Module Agent Surface 深模块评估

## Goal

评估并设计 `WorkspaceModuleAgentSurface` 深模块，用一个小 interface 收束 Workspace Module Agent tool 的 visibility、operation catalog、RuntimeGateway / Extension channel、Canvas host operation 与 presentation notification 规则，让现有 AgentTool 退回到参数反序列化和结果投影 adapter。

本任务先做高密度评估与切分设计，不启动实现。用户明确倾向“极端派”收束：不要为了保守过渡继续保留浅 interface、兼容路径或双实现。目标是判断正确的 `WorkspaceModuleAgentSurface` interface 应该长什么样，并制定一次性迁移旧散装入口的方案；如果现有 tool schema 阻碍正确 interface，可以纳入破坏式调整讨论，而不是默认保留。

## Evidence

- 架构 review 报告：`C:\Users\yihao.liao\AppData\Local\Temp\architecture-review-20260630-123422.html`。
- 后端 explorer 结论：`workspace_module/tools.rs` 把 AgentTool adapter、visibility resolution、operation catalog、Canvas runtime update、RuntimeGateway catalog/channel 和 presentation event 混在一个浅 module 中。
- 用户决策：显著接受“删除旧处理面”作为验收目标。新增 facade 不算完成；旧散装 ownership 必须迁移或删除，不保留双路径。
- 用户决策：保留 `workspace_module_list` / `workspace_module_describe` / `workspace_module_operate` / `workspace_module_invoke` / `workspace_module_present` 五个 Agent-facing tool 名作为稳定操作语言；它们的 implementation 必须退化为 thin adapter，真正的处理面归 `WorkspaceModuleAgentSurface::resolve/execute`。
- 相关文件：
  - `crates/agentdash-workspace-module/src/workspace_module/tools.rs`
  - `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs`
  - `crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs`
  - `crates/agentdash-api/src/bootstrap/session.rs`
  - `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs`

## Requirements

- R1. 定义候选 deep module 的目标 interface，例如 `resolve(context) -> WorkspaceModuleSurface` 与 `execute(command) -> WorkspaceModuleOperationOutcome`，并明确哪些事实藏在 implementation 后面。
- R2. 第一阶段保留现有五个 Agent-facing tool 名作为调用语言，原因是 Canvas skill、跨层 contract 与 Agent 操作语义已经围绕这些名称形成；tool implementation 必须只承担 schema、参数解析和 result projection。
- R3. 明确 `WorkspaceModuleVisibilitySource`、`WorkspaceModuleOperationRuntimeSource`、Canvas host operation、Extension RuntimeGateway action/channel、presentation notification 分别归 deep module 还是 adapter。
- R4. 明确测试策略：业务规则应能通过 typed surface/use case 测试，AgentTool 测试只保留 schema、invalid args 和 result projection。
- R5. 明确第一刀文件范围和回滚点，避免与二三档任务或其它并行会话交叉写入。
- R6. 不创建子任务；如果后续执行范围过大，先在本任务内拆阶段，不通过 child task 隐含依赖。
- R7. 验收以旧处理面删除为准：tool 构造器、source helper、runtime bridge adapter 不能继续拥有 visibility / operation / presentation 规则。

## Acceptance Criteria

- [ ] PRD 明确评估目标、证据、第一阶段约束和 out of scope。
- [ ] `design.md` 记录候选 interface、before/after module ownership、data flow、测试 seam 和风险。
- [ ] `implement.md` 记录可执行阶段：只读 mapping、facade 提取、tool adapter 变薄、测试迁移、验证命令。
- [ ] 评估结论能回答：五个现有 tool 如何映射到 `resolve/execute`，是否一次性覆盖 read/write/present surface，Canvas/Extension channel 是否进入同一个 surface。
- [ ] 实现验收必须证明旧散装 ownership 已删除或降级为 thin adapter；仅新增 facade 不满足完成标准。
- [ ] 未经用户确认，不执行代码修改或 `task.py start`。

## Out Of Scope

- 为兼容旧调用保留双 tool、双 schema 或旧入口。
- 重塑 Canvas HTTP / WorkspacePanel 前端 presentation contract。
- 重塑 Extension RuntimeGateway public interface。
- 数据库 migration。

## Resolved Decisions

- 保留五个 Agent-facing tool 名；删除的是旧散装处理面 ownership。
- 按完整 surface 设计，`resolve` 覆盖 list/describe/readiness，`execute` 覆盖 operate/invoke/present。若实现分阶段，阶段边界必须服务验证矩阵，而不是形成第二套长期 interface。
