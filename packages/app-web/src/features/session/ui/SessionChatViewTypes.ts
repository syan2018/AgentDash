import type { ReactNode } from "react";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type {
  ConversationMailboxSnapshotView,
  ConversationCommandSetView,
  ConversationCommandView,
  ConversationModelConfigView,
  MailboxStateView,
  MailboxMessageView,
} from "../../../generated/workflow-contracts";
import type { ConversationEffectiveExecutorConfigView } from "../../../generated/project-agent-contracts";
import type { ExecutorConfig } from "../../../services/executor";
import type { TaskSessionExecutorSummary } from "../../../types/context";
import type { ProjectAgentExecutor } from "../../../types";
import type { ImageAttachment } from "./composer/useImageAttachments";

export interface PromptTemplate {
  id: string;
  label: string;
  content: string;
}

export interface SessionChatCommandState {
  mode: "draft" | "runtime";
  executionStatus: string;
  commands: ConversationCommandSetView;
  modelConfig: ConversationModelConfigView;
  helperText?: string;
}

export interface SessionChatViewProps {
  /** 当前会话 ID，null 表示尚未创建 */
  sessionId: string | null;
  /** 文件引用依赖的工作空间上下文 */
  workspaceId?: string | null;

  // ─── 会话生命周期 ────────────────────────────────────

  /** @deprecated RuntimeSession 不再提供默认创建入口；业务执行必须从 lifecycle 入口派发。 */
  onCreateSession?: (title: string) => Promise<string>;

  /** @deprecated RuntimeSession 不再由聊天 UI 创建或切换。 */
  onSessionIdChange?: (id: string) => void;

  /** 消息发送成功后回调（父组件可刷新列表等） */
  onMessageSent?: () => void;

  /** Agent turn 结束时回调（turn_completed / turn_failed） */
  onTurnEnd?: () => void;

  /** 收到系统事件时回调，用于父层按事件驱动刷新额外状态面板 */
  onSystemEvent?: (eventType: string, event: BackboneEvent) => void;

  // ─── 执行器 ──────────────────────────────────────────

  /** 执行器提示（如 task 的 agent_type），自动映射为执行器选择 */
  executorHint?: string | null;

  /**
   * 当前 session 绑定的执行器默认值（来自 agent 配置或 session context 真值）。
   * 进入会话 / 切换会话时会被用来 hydrate 本地 executor 状态，避免默认显示"选择模型…"。
   * 用户手动改过之后不会被再次覆盖（按 sessionId 计一次）。
   */
  agentDefaults?: ProjectAgentExecutor | TaskSessionExecutorSummary | ConversationEffectiveExecutorConfigView | null;

  /** AgentRun/frame scoped executor state key; changes force authoritative hydration. */
  executorStateKey?: string | null;

  /** 隐藏执行器选择器（当外部已确定执行器时，如 Task 场景） */
  showExecutorSelector?: boolean;

  // ─── 控制动作 ────────────────────────────────────────

  commandState: SessionChatCommandState;

  onCommand: (
    command: ConversationCommandView,
    sessionId: string | null,
    prompt: string,
    executorConfig?: ExecutorConfig,
    imageAttachments?: ImageAttachment[],
    deliveryIntent?: string,
  ) => Promise<void>;

  onCancelAction?: () => Promise<void>;

  /** 用户在模型选择器中显式选择的本地 override；仅作为 command input，不作为 ProjectAgent 默认值。 */
  onExecutorConfigOverrideChange?: (config: ExecutorConfig | null) => void;

  // ─── Mailbox ─────────────────────────────────

  /** Mailbox 消息列表（来自 runtimeControl.mailbox_messages） */
  mailboxMessages?: MailboxMessageView[];
  /** Mailbox 展示状态（来自 conversation.mailbox） */
  mailboxSnapshot?: ConversationMailboxSnapshotView;
  /** Mailbox 根状态（来自 AgentRunWorkspaceView.mailbox） */
  mailboxState?: MailboxStateView;
  /** 引导 mailbox 消息 */
  onPromoteMailboxMessage?: (messageId: string) => void;
  /** 删除 mailbox 消息 */
  onDeleteMailboxMessage?: (messageId: string) => void;
  /** 恢复暂停的 mailbox */
  onResumeMailbox?: () => void;
  /** 召回 mailbox 消息（获取内容 + 删除 + 填充 composer） */
  onRecallMailboxMessage?: (messageId: string) => void;
  /** 重排序 mailbox 消息 */
  onMoveMailboxMessage?: (messageId: string, afterMessageId: string | null) => void;
  /** 外部注入的输入值（recall 后填充 composer），设置后立即消费 */
  injectedInputValue?: string | null;
  onInjectedInputConsumed?: () => void;

  // ─── 布局插槽 ────────────────────────────────────────

  /** 渲染在状态栏下方、流区域上方 */
  headerSlot?: ReactNode;

  /** 渲染在执行器选择器上方（如 owner binding 信息） */
  inputPrefix?: ReactNode;

  /** 注入到流区域顶部的固定内容（如 Task 上下文卡片），始终显示 */
  streamPrefixContent?: ReactNode;

  /** 隐藏内置连接状态栏 */
  showStatusBar?: boolean;

  /** 无 session 时显示的 prompt 模板按钮 */
  promptTemplates?: PromptTemplate[];

  /** 初始输入值（仅首次挂载时填充） */
  initialInputValue?: string;
}
