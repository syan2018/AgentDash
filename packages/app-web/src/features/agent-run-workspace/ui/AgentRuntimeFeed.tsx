import { useState, type ReactNode, type RefObject } from "react";

import type {
  CommandAvailability,
  InteractionResponse,
  RuntimeInteractionKind,
} from "../../../generated/agent-runtime-contracts";
import type { AgentRuntimeFeedEntry } from "../model/useAgentRuntimeFeed";
import { interactionResponseFromText } from "../model/interactionResponse";

export interface AgentRuntimeFeedProps {
  containerRef: RefObject<HTMLDivElement | null>;
  entries: AgentRuntimeFeedEntry[];
  isLoading: boolean;
  streamPrefixContent?: ReactNode;
  onScroll: () => void;
  interactionAvailability?: CommandAvailability;
  onResolveInteraction: (interactionId: string, response: InteractionResponse) => Promise<void>;
}

function entryStyle(entry: AgentRuntimeFeedEntry): string {
  if (entry.role === "user") return "ml-auto bg-primary/10 text-foreground";
  if (entry.role === "agent") return "mr-auto bg-card text-foreground shadow-sm";
  return "mr-auto bg-secondary/40 text-muted-foreground";
}

export function AgentRuntimeFeed({
  containerRef,
  entries,
  isLoading,
  streamPrefixContent,
  onScroll,
  interactionAvailability,
  onResolveInteraction,
}: AgentRuntimeFeedProps) {
  const [submittedInteractionIds, setSubmittedInteractionIds] = useState<string[]>([]);
  const [interactionErrors, setInteractionErrors] = useState<Record<string, string>>({});

  const resolveInteraction = async (
    interactionId: string,
    response: InteractionResponse,
  ) => {
    if (submittedInteractionIds.includes(interactionId)) return;
    setInteractionErrors((current) => ({ ...current, [interactionId]: "" }));
    setSubmittedInteractionIds((current) => [...current, interactionId]);
    try {
      await onResolveInteraction(interactionId, response);
    } catch (error) {
      setSubmittedInteractionIds((current) => current.filter((id) => id !== interactionId));
      setInteractionErrors((current) => ({
        ...current,
        [interactionId]: error instanceof Error ? error.message : "提交 interaction response 失败",
      }));
    }
  };

  return (
    <div ref={containerRef} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      {streamPrefixContent}
      {entries.length === 0 && (
        <div className="flex min-h-32 items-center justify-center text-sm text-muted-foreground">
          {isLoading ? "正在读取 Agent Runtime…" : "尚无 Runtime transcript"}
        </div>
      )}
      <div className="space-y-3">
        {entries.map((entry) => (
          <article
            key={entry.id}
            className={`w-fit max-w-[85%] rounded-[8px] px-3 py-2 text-sm ${entryStyle(entry)}`}
          >
            <div className="mb-1 flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground">
              <span>{entry.role}</span>
              {entry.status !== "completed" && <span>{entry.status}</span>}
            </div>
            <div className="whitespace-pre-wrap break-words leading-relaxed">{entry.text}</div>
            {entry.interaction && (
              <InteractionControls
                interaction={entry.interaction}
                submitted={submittedInteractionIds.includes(entry.interaction.interaction_id)}
                error={interactionErrors[entry.interaction.interaction_id]}
                availability={interactionAvailability}
                onResolve={(response) => {
                  const interaction = entry.interaction;
                  if (interaction) void resolveInteraction(interaction.interaction_id, response);
                }}
              />
            )}
          </article>
        ))}
      </div>
    </div>
  );
}

function interactionKindLabel(kind: RuntimeInteractionKind): string {
  switch (kind) {
    case "command_approval": return "命令审批";
    case "file_change_approval": return "文件变更审批";
    case "permission_approval": return "权限审批";
    case "user_input_request": return "用户输入请求";
    case "mcp_elicitation": return "MCP elicitation";
    case "dynamic_tool_execution": return "动态工具执行";
  }
}

function interactionSupportsApproval(kind: RuntimeInteractionKind): boolean {
  return kind === "command_approval"
    || kind === "file_change_approval"
    || kind === "permission_approval";
}

function InteractionControls({
  interaction,
  submitted,
  error,
  availability,
  onResolve,
}: {
  interaction: NonNullable<AgentRuntimeFeedEntry["interaction"]>;
  submitted: boolean;
  error?: string;
  availability?: CommandAvailability;
  onResolve: (response: InteractionResponse) => void;
}) {
  const [responseInput, setResponseInput] = useState("");
  const [validationError, setValidationError] = useState<string | null>(null);
  const pending = interaction.terminal == null;
  const supportsApproval = interactionSupportsApproval(interaction.interaction_kind);
  const available = availability?.status === "available";
  const unavailableReason = availability?.status === "unavailable"
    ? availability.reason
    : "Agent Runtime snapshot 尚未声明 interaction response 可用。";
  const canSubmit = pending && available && !submitted;
  const submitTypedInput = () => {
    const result = interactionResponseFromText(interaction.interaction_kind, responseInput);
    if (!result.ok) {
      setValidationError(result.error);
      return;
    }
    setValidationError(null);
    onResolve(result.response);
  };
  return (
    <div className="mt-2 border-t border-border/60 pt-2">
      <div className="flex flex-wrap items-center gap-2 text-[10px] text-muted-foreground">
        <span className="rounded-[6px] bg-background px-1.5 py-0.5">{interactionKindLabel(interaction.interaction_kind)}</span>
        <span className="break-all font-mono">{interaction.interaction_id}</span>
        <span>{interaction.terminal ?? (submitted ? "response submitted" : "pending")}</span>
      </div>
      {pending && supportsApproval && (
        <div className="mt-2 flex gap-2">
          <button
            type="button"
            disabled={!canSubmit}
            onClick={() => onResolve({ kind: "approved" })}
            className="rounded-[6px] border border-success/30 bg-success/10 px-2 py-1 text-xs text-success transition-colors hover:bg-success/15 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {submitted ? "已提交" : "批准"}
          </button>
          <button
            type="button"
            disabled={!canSubmit}
            onClick={() => onResolve({ kind: "denied", reason: null })}
            className="rounded-[6px] border border-warning/30 bg-warning/10 px-2 py-1 text-xs text-warning transition-colors hover:bg-warning/15 disabled:cursor-not-allowed disabled:opacity-50"
          >
            拒绝
          </button>
        </div>
      )}
      {pending && !supportsApproval && (
        <div className="mt-2 space-y-2">
          <textarea
            value={responseInput}
            onChange={(event) => setResponseInput(event.target.value)}
            disabled={!available || submitted}
            placeholder={interaction.interaction_kind === "user_input_request" ? "输入回复" : "输入 JSON payload"}
            className="min-h-20 w-full rounded-[8px] border border-input bg-background px-2.5 py-2 text-xs text-foreground outline-none focus:border-primary disabled:cursor-not-allowed disabled:opacity-50"
          />
          <button
            type="button"
            disabled={!canSubmit}
            onClick={submitTypedInput}
            className="rounded-[6px] border border-border bg-background px-2.5 py-1 text-xs text-foreground transition-colors hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-50"
          >
            {submitted ? "已提交" : "提交响应"}
          </button>
        </div>
      )}
      {pending && !available && <div className="mt-2 text-xs text-warning">{unavailableReason}</div>}
      {validationError && <div className="mt-2 text-xs text-destructive">{validationError}</div>}
      {error && <div className="mt-2 text-xs text-destructive">{error}</div>}
    </div>
  );
}
