import type { ReactNode } from "react";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { ConversationEffectiveExecutorConfigView } from "../../../generated/project-agent-contracts";
import type {
  BackendSelectionRequestDto,
  MailboxMessageView,
  MailboxStateView,
  SessionMessageRefDto,
} from "../../../generated/agent-run-mailbox-contracts";
import type { ExecutorConfig } from "../../../services/executor";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import type { TaskSessionExecutorSummary } from "../../../types/context";
import type { ProjectAgentExecutor } from "../../../types";
import type { ImageAttachment } from "./composer/useImageAttachments";

export interface PromptTemplate {
  id: string;
  label: string;
  content: string;
}

export interface SessionChatModelConfig {
  status: "resolved" | "model_required";
  effective_executor_config?: ConversationEffectiveExecutorConfigView;
  missing_fields: string[];
  message?: string;
}

export interface SessionChatCommandModel {
  command_id: string;
  kind: string;
  enabled: boolean;
  unavailable_reason?: string;
  disabled_code?: string;
  requires_input: boolean;
  executor_config_policy: "required" | "optional" | "forbidden";
  shortcut?: "enter" | "ctrl_enter";
}

export interface SessionChatCommandState {
  mode: "draft" | "runtime";
  executionStatus: string;
  commands: SessionChatCommandModel[];
  keyboard: {
    enter?: string;
    ctrl_enter?: string;
  };
  primaryCommandId?: string;
  cancelCommand?: SessionChatCommandModel;
  modelConfig: SessionChatModelConfig;
  helperText?: string;
}

export interface SessionChatMailboxModel {
  messages: MailboxMessageView[];
  state?: MailboxStateView;
  paused: boolean;
  user_attention: boolean;
  hide_system_steer_messages: boolean;
  can_resume: boolean;
  resumeAction?: SessionChatCommandModel;
  promoteAction?: SessionChatCommandModel;
  deleteAction?: SessionChatCommandModel;
}

export interface SessionChatModel {
  sessionId: string | null;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  workspaceId?: string | null;
  executorHint?: string | null;
  agentDefaults?: ProjectAgentExecutor | TaskSessionExecutorSummary | ConversationEffectiveExecutorConfigView | null;
  executorStateKey?: string | null;
  showExecutorSelector?: boolean;
  commandState: SessionChatCommandState;
  mailbox: SessionChatMailboxModel;
  statusBarRunId?: string | null;
  statusBarAgentId?: string | null;
  injectedInputValue?: string | null;
}

export interface SessionChatSubmitIntent {
  command_id: string;
  sessionId: string | null;
  prompt: string;
  executorConfig?: ExecutorConfig;
  backendSelection?: BackendSelectionRequestDto;
  imageAttachments?: ImageAttachment[];
  deliveryIntent?: string;
}

export interface SessionChatViewIntents {
  submitComposer: (intent: SessionChatSubmitIntent) => Promise<void>;
  cancelAction?: () => Promise<void>;
  setExecutorConfigOverride?: (config: ExecutorConfig | null) => void;
  promoteMailboxMessage?: (messageId: string) => void;
  deleteMailboxMessage?: (messageId: string) => void;
  resumeMailbox?: () => void;
  recallMailboxMessage?: (messageId: string) => void;
  moveMailboxMessage?: (messageId: string, afterMessageId: string | null) => void;
  forkFromMessageRef?: (forkPointRef: SessionMessageRefDto) => Promise<void>;
  injectedInputConsumed?: () => void;
}

export interface SessionChatViewProps {
  /** ChatView 消费的 UI model；command DTO 已在外层 control-plane 投影完成。 */
  model: SessionChatModel;
  /** ChatView 只表达用户意图，不接触 backend command DTO。 */
  intents: SessionChatViewIntents;

  // ─── 会话生命周期 ────────────────────────────────────

  /** 消息发送成功后回调（父组件可刷新列表等） */
  onMessageSent?: () => void;

  /** Agent turn 结束时回调（turn_completed / turn_failed） */
  onTurnEnd?: () => void;

  /** 收到系统事件时回调，用于父层按事件驱动刷新额外状态面板 */
  onSystemEvent?: (eventType: string, event: BackboneEvent) => void;

  /** task_write 工具完成时回调；用于刷新外部 Task plan 展示。 */
  onTaskPlanChanged?: () => void;

  // ─── 布局插槽 ────────────────────────────────────────

  /** 渲染在状态栏下方、流区域上方 */
  headerSlot?: ReactNode;

  /** 渲染在执行器选择器上方（如 owner binding 信息） */
  inputPrefix?: ReactNode;

  /** 渲染在 composer 工具栏内（如 backend / 模型等可执行上下文选择） */
  inputToolbarSlot?: ReactNode;

  /** 注入到流区域顶部的固定内容（如 Task 上下文卡片），始终显示 */
  streamPrefixContent?: ReactNode;

  /** 隐藏内置连接状态栏 */
  showStatusBar?: boolean;

  /** 无 session 时显示的 prompt 模板按钮 */
  promptTemplates?: PromptTemplate[];

  /** 初始输入值（仅首次挂载时填充） */
  initialInputValue?: string;
}
