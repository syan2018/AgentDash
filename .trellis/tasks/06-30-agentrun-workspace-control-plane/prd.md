# AgentRun Workspace Control Plane 深模块评估

## Goal

评估并设计 `AgentRunWorkspaceControlPlane` 深模块，用一个窄的前端 interface 收束 AgentRun workspace 页面中的 command projection、mailbox、stale guard、executor override、workspace runtime data、hook runtime refresh 与 `SessionChatView` props fanout，让页面回到 layout/navigation，让 ChatView 回到 renderer/composer UI。

本任务先做高密度评估与切分设计，不启动实现。用户明确倾向“极端派”收束：不要为了保守过渡继续保留浅 props、双 command model 或 page 级散装处理面。目标是定义正确的 `AgentRunWorkspaceControlPlane` interface，并制定删除旧散装组装方式的迁移方案；用户可见交互语义应保持正确，但内部 interface 可以破坏式调整。

## Evidence

- 架构 review 报告：`C:\Users\yihao.liao\AppData\Local\Temp\architecture-review-20260630-123422.html`。
- 前端 explorer 结论：`AgentRunWorkspacePage` 同时装配 project/store/lifecycle/workspace-module/extension-runtime/task-plan/story/workspace binding，并把 runtime command、mailbox、executor config、refresh strategy 等 implementation 细节传给 `useAgentRunWorkspaceCommands` 和 `SessionChatView`。
- 用户决策：显著接受“删除旧处理面”作为验收目标。新增 adapter 不算完成；旧散装 props、command model、mailbox lookup 和 refresh ownership 必须迁移或删除，不保留双路径。
- 用户决策：第一刀直接缩窄 `SessionChatView` public interface；`SessionChatView` 消费 control-plane 产出的 chat/composer/mailbox view model 与 intent handlers，后端 generated command snapshot 仍是 command authority，但不作为 ChatView 的 public props 事实源。
- 相关文件：
  - `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
  - `packages/app-web/src/pages/AgentRunWorkspacePage.conversationCommandState.ts`
  - `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts`
  - `packages/app-web/src/features/session/ui/SessionChatView.tsx`
  - `packages/app-web/src/features/session/ui/SessionChatViewTypes.ts`
  - `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts`

## Requirements

- R1. 定义候选 deep module 的目标 interface，例如 `useAgentRunWorkspaceControlPlane(...) -> { chatModel, workspaceRuntimeData, ownerBarModel, actions, status }`。
- R2. 用户可见交互语义必须稳定，包括 Enter / Ctrl+Enter、cancel、mailbox promote/delete/resume、draft start、running steer、model selector、workspace panel tab opening；内部 props/interface 不默认稳定。
- R3. 明确 `AgentRunWorkspacePage`、`useAgentRunWorkspaceControlPlane`、`SessionChatView`、workspace module presentation helper 各自保留的职责；ChatView public interface 第一刀收敛到 UI model + intent handlers。
- R4. 明确测试策略：command policy、mailbox action、refresh effects 应通过 control-plane model 测试；React page tests 只验证 layout wiring；ChatView tests 只验证给定 view model 的渲染与 intent 触发。
- R5. 明确第一刀文件范围和回滚点，避免一次性重写 ChatView 或 stream/feed。
- R6. 不创建子任务；如果后续执行范围过大，先在本任务内拆阶段，不通过 child task 隐含依赖。
- R7. 验收以旧处理面删除为准：page、command hook、ChatView props 不能继续共同拥有同一套 command/mailbox/refresh 规则。

## Acceptance Criteria

- [ ] PRD 明确评估目标、证据、第一阶段约束和 out of scope。
- [ ] `design.md` 记录候选 interface、before/after module ownership、state/data flow、测试 seam 和用户可见风险。
- [ ] `implement.md` 记录可执行阶段：只读 mapping、control-plane adapter 提取、page props 收口、ChatView props 缩窄、测试迁移、验证命令。
- [ ] 评估结论能回答：如何破坏式缩窄 `SessionChatView` public props，以及如何把 mailbox command lookup / refresh effects / executor override 纳入同一 control-plane model。
- [ ] 实现验收必须证明旧散装 ownership 已删除或降级为 thin view adapter；仅新增 adapter 不满足完成标准。
- [ ] 未经用户确认，不执行代码修改或 `task.py start`。

## Out Of Scope

- 重写 `SessionChatView` 的 stream/feed 渲染体系。
- 改变 AgentRun backend command DTO 或 mailbox contract。
- 为兼容旧 props 或旧 command model 保留双路径。
- 大规模视觉改版。

## Resolved Decisions

- 第一刀直接缩窄 `SessionChatView` public interface。后端 generated command snapshot 保持 command authority；control-plane 把它投影为 ChatView 消费的 composer/mailbox view model。
- Mailbox command lookup、stale refresh、hook runtime refresh、executor override policy 与 workspace module presentation opening policy 进入 control-plane；page 只提供 layout、navigation 和必要 UI adapter。
