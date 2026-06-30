# AgentRun Workspace Control Plane 深模块评估 - Design

## Problem

`AgentRunWorkspacePage` 当前同时承担 layout、navigation、store refresh、command policy、mailbox actions、executor override、workspace runtime data、hook runtime refresh 和 workspace module presentation。`SessionChatView` 直接接收 backend command DTO 与大量 command handler，因此它的 interface 接近 implementation。

## Candidate Interface

候选 deep module：

```text
useAgentRunWorkspaceControlPlane(input) -> {
  chatModel,
  workspaceRuntimeData,
  ownerBarModel,
  actions,
  status,
}
```

`input` 包含 run/agent/draft ids、route state、workspace projection、stores 和必要 adapters。implementation 内部解析 conversation snapshot、mailbox command、keyboard command、executor override、refresh effects 与 workspace module presentation action。后端 generated command snapshot 仍是 command authority；control-plane 负责把它投影成 ChatView 消费的 composer/mailbox view model。

本任务采用“正确 interface 优先”的设计口径：第一刀直接缩窄 `SessionChatView` public interface，让 ChatView 消费 UI model + intent handlers。generated DTO 可以作为 control-plane 输入事实源，但不再作为 ChatView public props 的事实源。

用户已显著接受“删除旧处理面”作为完成标准。设计必须把旧 ownership 的去向写清楚：迁移到 control-plane module、降级为 thin view adapter，或删除。新增 adapter 但 page / ChatView / command hook 继续共同拥有规则不算完成。

## Ownership Sketch

- `AgentRunWorkspacePage`：layout、route/navigation、panel sizing、把 control-plane model 接给 UI。
- `useAgentRunWorkspaceControlPlane`：command projection、mailbox actions、draft/runtime distinction、refresh effects、executor override policy、workspace module presentation opening policy。
- `SessionChatView`：消费更窄 `ConversationInputModel` / `SessionComposerControl` / `MailboxViewModel`，只触发 UI intent。

## Completion Standard

- `AgentRunWorkspacePage` 不再拥有 command/mailbox/refresh 规则，只负责 layout/navigation/wiring。
- `SessionChatView` 不再直接消费 backend command DTO 作为控制面事实源；它消费 chat/input view model。
- `useAgentRunWorkspaceCommands` 若保留，只能作为 thin effect adapter，不拥有 command projection policy。
- 测试必须覆盖 control-plane model，而不是只通过 page render 触发旧散装处理。

## Evaluation Focus

1. 第一刀如何直接缩窄 ChatView public interface，并保持 stream/feed 渲染体系稳定。
2. mailbox command lookup 是否进入 control-plane model。
3. hook runtime refresh timer 是否归 control-plane。
4. workspace module presentation event opening 是否归 control-plane or page action。
5. 如何保持 draft start / runtime submit / running steer 用户行为完全稳定。

## Risk

- 用户可见交互分支多，尤其 Enter / Ctrl+Enter、cancel、model_required、mailbox promote/delete/resume。
- 直接缩窄 ChatView props 需要更大测试迁移，但可以避免留下第二套 command model。
- Refresh effects 散在多个 store，control-plane module 需要明确 effect ownership，避免变成新“万能 page”。

## Validation Shape

- Control-plane model tests：command availability、mailbox actions、refresh effects。
- Page tests：layout wiring、workspace panel opening。
- ChatView tests：在第一阶段尽量不大动；第二阶段只测 view model 渲染和 intent。
