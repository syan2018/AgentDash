export type AgentRunDeliveryStatus =
  | "idle"
  | "running"
  | "cancelling"
  | "completed"
  | "failed"
  | "interrupted"
  | "lost";

export const AGENT_RUN_DELIVERY_STATUS_LABEL: Record<AgentRunDeliveryStatus, string> = {
  idle: "就绪",
  running: "执行中",
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
