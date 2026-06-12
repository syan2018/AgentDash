import type { SessionChatControlState } from "../features/session";
import type { SessionRuntimeStateStatus } from "../features/workspace-runtime";
import type { AgentRunWorkspaceView, ProjectAgentSummary } from "../types";

export function readonlyChatControlState(reason: string): SessionChatControlState {
  return {
    mode: "runtime",
    controlPlaneStatus: "unavailable",
    primaryAction: {
      kind: "none",
      enabled: false,
      label: "发送",
      placeholder: "当前 AgentRun 只能查看 runtime trace。",
      unavailableReason: reason,
    },
    cancelAction: {
      enabled: false,
      label: "取消",
      unavailableReason: "当前 AgentRun 没有正在执行的 turn。",
    },
    helperText: reason,
  };
}

export interface AgentRunWorkspaceChatControlInput {
  isProjectAgentDraft: boolean;
  draftProjectIdValue: string | null;
  draftProjectAgentKey: string | null;
  draftProjectAgent: ProjectAgentSummary | null;
  currentRunId: string | null;
  currentAgentId: string | null;
  projectionStatus: SessionRuntimeStateStatus;
  projectionError: string | null;
  workspace: AgentRunWorkspaceView | null;
}

export function deriveAgentRunWorkspaceChatControlState({
  isProjectAgentDraft,
  draftProjectIdValue,
  draftProjectAgentKey,
  draftProjectAgent,
  currentRunId,
  currentAgentId,
  projectionStatus,
  projectionError,
  workspace,
}: AgentRunWorkspaceChatControlInput): SessionChatControlState {
  if (isProjectAgentDraft) {
    const enabled = Boolean(draftProjectIdValue && draftProjectAgentKey && draftProjectAgent);
    const unavailableReason = !draftProjectIdValue || !draftProjectAgentKey
      ? "Draft AgentRun 缺少 ProjectAgent 参数。"
      : !draftProjectAgent
        ? "正在加载 ProjectAgent 配置。"
        : undefined;
    return {
      mode: "draft",
      controlPlaneStatus: "draft",
      primaryAction: {
        kind: "start_draft",
        enabled,
        label: "开始",
        placeholder: "输入首条消息，Ctrl+Enter 开始…",
        unavailableReason,
      },
      cancelAction: {
        enabled: false,
        label: "取消",
        unavailableReason: "Draft AgentRun 尚未启动。",
      },
      helperText: enabled ? undefined : unavailableReason,
    };
  }

  if (!currentRunId || !currentAgentId) {
    return readonlyChatControlState("当前没有可控制的 AgentRun。");
  }
  if (projectionStatus === "loading" || projectionStatus === "refreshing") {
    return readonlyChatControlState("正在解析当前 AgentRun 的工作台状态。");
  }
  if (projectionStatus === "error") {
    return readonlyChatControlState(projectionError ?? "AgentRun 工作台投影加载失败。");
  }
  if (projectionStatus !== "ready") {
    return readonlyChatControlState("当前 AgentRun 工作台投影尚未就绪。");
  }
  if (!workspace) {
    return readonlyChatControlState("当前 AgentRun 工作台状态尚未返回。");
  }

  const actions = workspace.actions;
  const isRunning = workspace.control_plane.status === "running";

  if (isRunning && actions.enqueue.enabled) {
    const steerSecondary: SessionChatControlState["secondaryAction"] = actions.steer.enabled
      ? {
          kind: "steer",
          enabled: true,
          label: "Steer",
          placeholder: "Ctrl+Enter 立即注入 steer 指令…",
          unavailableReason: undefined,
        }
      : undefined;

    return {
      mode: "runtime",
      controlPlaneStatus: workspace.control_plane.status,
      primaryAction: {
        kind: "enqueue",
        enabled: true,
        label: "排队",
        placeholder: steerSecondary
          ? "Enter 排队，Ctrl+Enter steer，@ 引用文件…"
          : "Enter 排队发送，@ 引用文件…",
        unavailableReason: undefined,
      },
      secondaryAction: steerSecondary,
      cancelAction: {
        enabled: actions.cancel.enabled,
        label: "取消",
        unavailableReason: actions.cancel.unavailable_reason,
      },
      helperText: workspace.control_plane.reason ?? undefined,
    };
  }

  const primary = actions.send_next.enabled
    ? {
        kind: "send_next" as const,
        enabled: true,
        label: "发送",
        placeholder: "继续对话，@ 引用文件，Ctrl+Enter 发送…",
        unavailableReason: undefined,
      }
    : {
        kind: "none" as const,
        enabled: false,
        label: "发送",
        placeholder: isRunning
          ? "当前 AgentRun 正在执行，等待可用或取消。"
          : "当前 AgentRun 只能查看 runtime trace。",
        unavailableReason: isRunning
          ? actions.steer.unavailable_reason ?? workspace.control_plane.reason
          : actions.send_next.unavailable_reason ?? workspace.control_plane.reason,
      };

  return {
    mode: "runtime",
    controlPlaneStatus: workspace.control_plane.status,
    primaryAction: primary,
    cancelAction: {
      enabled: actions.cancel.enabled,
      label: "取消",
      unavailableReason: actions.cancel.unavailable_reason,
    },
    helperText: primary.enabled
      ? workspace.control_plane.reason ?? undefined
      : primary.unavailableReason,
  };
}
