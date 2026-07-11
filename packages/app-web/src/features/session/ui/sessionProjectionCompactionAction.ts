import type {
  AgentRunCommandPreconditionView,
} from "../../../generated/agent-run-mailbox-contracts";
import type { OperationReceipt } from "../../../generated/agent-runtime-contracts";
import type { ConversationCommandView } from "../../../generated/workflow-contracts";

export function commandPrecondition(command: ConversationCommandView): AgentRunCommandPreconditionView {
  return {
    command_id: command.command_id,
    command_kind: command.kind,
    stale_guard: command.stale_guard,
  };
}

export function newClientCommandId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `cmd-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function contextCompactionOutcomeMessage(
  response: OperationReceipt,
): string {
  return response.duplicate
    ? `压缩操作已存在 · ${response.operation_id}`
    : `压缩操作已接受 · ${response.operation_id}`;
}
