import type { AgentRunListRuntimeThreadStatus } from "../../generated/workflow-contracts";

export type AgentRunDeliveryStatus =
  | "idle"
  | "running"
  | "suspended"
  | "cancelling"
  | "completed"
  | "failed"
  | "interrupted"
  | "lost";

export const AGENT_RUN_DELIVERY_STATUS_LABEL: Record<AgentRunDeliveryStatus, string> = {
  idle: "就绪",
  running: "执行中",
  suspended: "已暂停",
  cancelling: "取消中",
  completed: "已完成",
  failed: "失败",
  interrupted: "已中断",
  lost: "已丢失",
};

export function normalizeAgentRunDeliveryStatus(status: string): AgentRunDeliveryStatus {
  if (
    status === "idle"
    || status === "running"
    || status === "suspended"
    || status === "cancelling"
    || status === "completed"
    || status === "failed"
    || status === "interrupted"
    || status === "lost"
  ) {
    return status;
  }
  return "idle";
}

export function agentRunListPresentationStatus(
  runtimeStatus: AgentRunListRuntimeThreadStatus | undefined,
  activeTurnId: string | undefined,
  lifecycleStatus: string,
): AgentRunDeliveryStatus {
  if (runtimeStatus === "active") return activeTurnId ? "running" : "idle";
  if (runtimeStatus === "suspended") return "suspended";
  if (runtimeStatus === "desynchronized") return "lost";
  if (runtimeStatus === "lost") return "lost";
  if (lifecycleStatus === "completed") return "completed";
  if (lifecycleStatus === "failed") return "failed";
  if (lifecycleStatus === "cancelled") return "interrupted";
  return "idle";
}
