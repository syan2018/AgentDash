import type { MailboxMessageView } from "../../../generated/agent-run-mailbox-contracts";
import type { AgentRunChatMailboxModel } from "../model/conversationCommandState";

/** mailbox 是否有可展示内容（消息 / 关注 / 暂停）。供综合状态栏判断是否渲染。 */
export function mailboxHasContent(
  messages: MailboxMessageView[],
  mailbox?: AgentRunChatMailboxModel,
): boolean {
  const steer = messages.filter(
    (m) => m.delivery.kind === "steer_active_turn" &&
      (!mailbox?.hide_system_steer_messages || m.origin === "user"),
  );
  const pending = messages.filter((m) => m.delivery.kind !== "steer_active_turn");
  const waitingCount = mailbox?.waiting_items.length ?? 0;
  return Boolean(
    steer.length > 0 ||
      pending.length > 0 ||
      waitingCount > 0 ||
      mailbox?.user_attention ||
      mailbox?.paused,
  );
}
