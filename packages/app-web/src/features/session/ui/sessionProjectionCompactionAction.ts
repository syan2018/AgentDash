import type {
  AgentRunCommandPreconditionView,
  AgentRunContextCompactionCommandResponse,
} from "../../../generated/agent-run-interaction-contracts";
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
  response: AgentRunContextCompactionCommandResponse,
): string {
  switch (response.outcome) {
    case "scheduled_next_turn":
      return "已排队";
    case "launched_compaction_turn":
      return "已启动";
    case "completed":
      return response.message ?? "压缩完成";
    case "no_eligible_messages":
      return response.message ?? "暂无可压缩内容";
    case "blocked":
      return response.message ?? "当前无法压缩";
    case "failed":
      return response.message ?? "压缩失败";
  }
}
