import type { ConversationWaitingItemView } from "../../../generated/workflow-contracts";
import { buildInteractiveTerminalUri } from "../../workspace-panel/tab-types/terminal-uri";

export function terminalUriForWaitingItem(item: ConversationWaitingItemView): string | null {
  if (item.kind !== "exec") return null;
  const terminalId = item.source_ref?.trim();
  if (!terminalId) return null;
  return buildInteractiveTerminalUri(terminalId);
}

