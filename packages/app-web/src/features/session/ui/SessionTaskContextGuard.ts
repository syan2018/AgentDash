import type { ContentBlock } from "../model/types";

export function isAgentDashTaskContextBlock(block: ContentBlock | undefined): boolean {
  if (!block || block.type !== "resource") return false;
  return block.resource.uri.startsWith("agentdash://task-context/");
}
