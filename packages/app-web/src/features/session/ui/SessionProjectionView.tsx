import { useCallback, useEffect, useRef, useState } from "react";

import type {
  ContextBlock,
  RuntimeContextView,
  RuntimeInput,
  RuntimeItemContent,
  DynamicToolCallOutputContentItem,
} from "../../../generated/agent-runtime-contracts";
import {
  compactAgentRunContext,
  fetchAgentRunRuntimeContext,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";
import type { TokenUsageInfo } from "../model/types";
import {
  shouldApplyRuntimeContextResponse,
  type RuntimeContextRequestToken,
} from "../model/runtimeContextRequest";
import type { SessionChatCommandModel } from "./SessionChatViewTypes";
import {
  contextCompactionOutcomeMessage,
  newClientCommandId,
} from "./sessionProjectionCompactionAction";

export interface SessionProjectionViewProps {
  agentRunTarget?: AgentRunRuntimeTarget | null;
  refreshKey?: number;
  tokenUsage?: TokenUsageInfo | null;
  compactContextCommand?: SessionChatCommandModel;
  embedded?: boolean;
}

export interface SessionProjectionViewPanelProps {
  context: RuntimeContextView | null;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  tokenUsage?: TokenUsageInfo | null;
  compactContextCommand?: SessionChatCommandModel;
  isLoading?: boolean;
  error?: string | null;
  onRefresh?: () => void;
  embedded?: boolean;
}

interface ContextCompactionActionState {
  kind: "none" | "pending" | "success" | "error";
  message?: string;
}

function CompactContextIcon({ loading }: { loading: boolean }) {
  if (loading) {
    return <span aria-hidden="true" className="h-3.5 w-3.5 animate-spin rounded-[8px] border border-current border-t-transparent" />;
  }
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M8 3v5H3" />
      <path d="M16 21v-5h5" />
      <path d="M3 8l6-6" />
      <path d="M21 16l-6 6" />
      <path d="M16 3v5h5" />
      <path d="M8 21v-5H3" />
      <path d="M21 8l-6-6" />
      <path d="M3 16l6 6" />
    </svg>
  );
}

function runtimeInputText(input: RuntimeInput): string {
  switch (input.kind) {
    case "text": return input.text;
    case "file_reference": return input.uri;
    case "image": return `[image ${input.mime_type}]`;
    case "structured": return JSON.stringify(input.value, null, 2);
  }
}

function toolContentText(items: DynamicToolCallOutputContentItem[] | null): string {
  return (items ?? []).map((item) => {
    switch (item.type) {
      case "inputText": return item.text;
      case "inputImage": return item.imageUrl;
    }
  }).join("\n");
}

function runtimeItemText(content: RuntimeItemContent): string {
  switch (content.type) {
    case "userMessage": return content.content.map((item) => {
      switch (item.type) {
        case "text": return item.text;
        case "image": return item.url;
        case "localImage": return item.path;
        case "skill": return item.name;
        case "mention": return item.path;
      }
    }).join("\n");
    case "agentMessage": return content.text;
    case "reasoning": return [...(content.summary ?? []), ...(content.content ?? [])].join("\n");
    case "plan": return content.text;
    case "commandExecution": return content.aggregatedOutput ?? content.command;
    case "fileChange": return content.status;
    case "mcpToolCall": return `${content.server}/${content.tool}`;
    case "dynamicToolCall": return content.tool;
    case "collabAgentToolCall": return content.tool;
    case "subAgentActivity": return content.kind;
    case "webSearch": return content.query;
    case "imageView": return content.path;
    case "sleep": return `${content.durationMs}ms`;
    case "imageGeneration": return content.result;
    case "hookPrompt": return "Hook prompt";
    case "enteredReviewMode": return content.review;
    case "exitedReviewMode": return content.review;
    case "contextCompaction": return "上下文压缩";
    case "shellExec": return content.aggregatedOutput ?? content.command;
    case "terminalControl": return content.aggregatedOutput ?? `${content.operation}: ${content.terminalId}`;
    case "fsRead": return content.path;
    case "fsGrep": return content.pattern;
    case "fsGlob": return content.pattern;
    case "vfs": return toolContentText(content.contentItems) || content.resourceUri || content.operation;
    case "runtimeAction": return toolContentText(content.contentItems) || content.actionKey;
    case "workspaceModule": return toolContentText(content.contentItems) || content.resourceUri || content.operation;
    case "companion": return toolContentText(content.contentItems) || content.operation;
    case "task": return toolContentText(content.contentItems) || content.taskId || content.operation;
    case "wait": return toolContentText(content.contentItems) || (content.durationMs == null ? "wait" : `${content.durationMs}ms`);
    case "lifecycleComplete": return toolContentText(content.contentItems) || content.nodeId || "lifecycle complete";
  }
}

function contextBlockView(block: ContextBlock): { label: string; text: string } {
  switch (block.kind) {
    case "instruction": return { label: "Instruction", text: block.text };
    case "input": return { label: "Input", text: block.input.map(runtimeInputText).join("\n") };
    case "runtime_item": return { label: "Runtime item", text: runtimeItemText(block.content) };
    case "compaction_summary": return { label: "Compaction summary", text: block.summary };
  }
}

function ContextBlockRow({ block, index }: { block: ContextBlock; index: number }) {
  const view = contextBlockView(block);
  return (
    <div className="grid gap-2 border-t border-border/70 px-3 py-2.5 text-xs md:grid-cols-[140px_1fr]">
      <div className="flex items-start gap-2 text-muted-foreground">
        <span className="font-mono text-[10px]">#{index + 1}</span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 text-[10px] font-medium">{view.label}</span>
      </div>
      <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-words font-sans text-xs leading-relaxed text-foreground/85">{view.text || "(empty)"}</pre>
    </div>
  );
}

export function SessionProjectionViewPanel({
  context,
  agentRunTarget = null,
  tokenUsage,
  compactContextCommand,
  isLoading = false,
  error = null,
  onRefresh,
  embedded = false,
}: SessionProjectionViewPanelProps) {
  const [compactAction, setCompactAction] = useState<ContextCompactionActionState>({ kind: "none" });
  const compactPending = compactAction.kind === "pending";
  const compactUnavailableReason = compactContextCommand?.unavailable_reason
    ?? compactContextCommand?.disabled_code
    ?? "当前不可压缩";
  const compactDisabled = !agentRunTarget
    || !compactContextCommand?.enabled
    || compactPending;

  const handleCompactContext = useCallback(async () => {
    if (!agentRunTarget || !compactContextCommand || compactPending) return;
    if (!compactContextCommand.enabled) {
      setCompactAction({ kind: "error", message: compactUnavailableReason });
      return;
    }
    setCompactAction({ kind: "pending", message: "提交中" });
    try {
      const receipt = await compactAgentRunContext(agentRunTarget.runId, agentRunTarget.agentId, {
        client_command_id: newClientCommandId(),
      });
      setCompactAction({ kind: "success", message: contextCompactionOutcomeMessage(receipt) });
      onRefresh?.();
    } catch (compactError) {
      setCompactAction({
        kind: "error",
        message: compactError instanceof Error ? compactError.message : "压缩请求失败",
      });
    }
  }, [agentRunTarget, compactContextCommand, compactPending, compactUnavailableReason, onRefresh]);

  const card = (
    <div className={embedded
      ? "w-full overflow-hidden rounded-[12px] bg-popover shadow-lg"
      : "mx-auto w-full max-w-4xl overflow-hidden rounded-[8px] border border-border bg-secondary/20"}
    >
      <div className="flex flex-wrap items-center gap-2 px-3 py-2">
        <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-semibold uppercase text-muted-foreground">CONTEXT</span>
        {context ? (
          <>
            <span className="text-xs text-muted-foreground">{context.fidelity}</span>
            <span className="text-xs text-muted-foreground">{context.blocks.length} blocks</span>
            {tokenUsage && <span className="text-xs text-muted-foreground">当前 {tokenUsage.currentContextTokens.toLocaleString()} tokens</span>}
          </>
        ) : (
          <span className="text-xs text-muted-foreground">{isLoading ? "加载中" : "暂无 active context"}</span>
        )}
        <div className="ml-auto flex items-center gap-1.5">
          {compactAction.message && (
            <span className={`max-w-48 truncate text-xs ${compactAction.kind === "error" ? "text-destructive" : "text-muted-foreground"}`} title={compactAction.message}>
              {compactAction.message}
            </span>
          )}
          {compactContextCommand && (
            <button
              type="button"
              onClick={() => { void handleCompactContext(); }}
              disabled={compactDisabled}
              title={compactContextCommand.enabled ? "压缩上下文" : compactUnavailableReason}
              className="inline-flex h-7 items-center gap-1 rounded-[8px] border border-border bg-background px-2 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
            >
              <CompactContextIcon loading={compactPending} />
              <span>{compactPending ? "提交中" : "压缩"}</span>
            </button>
          )}
          <button type="button" onClick={onRefresh} disabled={isLoading} className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50">
            {isLoading ? "刷新中" : "刷新"}
          </button>
        </div>
      </div>
      {error && <div className="border-t border-border px-3 py-2 text-xs text-destructive">{error}</div>}
      {context && (
        <div className="grid gap-3 border-t border-border px-3 py-3 text-xs md:grid-cols-2">
          <div className="space-y-1 text-muted-foreground">
            <div className="text-[10px] font-semibold uppercase tracking-wider">Active head</div>
            {context.head ? (
              <>
                <div className="break-all font-mono text-foreground/80">{context.head.checkpoint_id}</div>
                <div>revision {String(context.head.revision)} · {context.head.fidelity}</div>
                <div>settings {String(context.head.provenance.settings_revision)} · tools {String(context.head.provenance.tool_set_revision)}</div>
                <div className="break-all font-mono text-[10px]">{context.head.digest}</div>
              </>
            ) : <div>尚无 active head</div>}
          </div>
          <div className="space-y-1 text-muted-foreground">
            <div className="text-[10px] font-semibold uppercase tracking-wider">Checkpoint</div>
            {context.checkpoint ? (
              <>
                <div className="break-all font-mono text-foreground/80">{context.checkpoint.checkpoint_id}</div>
                <div>revision {String(context.checkpoint.revision)} · recipe {String(context.checkpoint.materialized.recipe.revision)}</div>
                <div>{context.checkpoint.materialized.recipe.source_item_ids.length} source items</div>
                <div className="break-all font-mono text-[10px]">{context.checkpoint.materialized.digest}</div>
              </>
            ) : <div>尚无 materialized checkpoint</div>}
          </div>
        </div>
      )}
      {context?.blocks.map((block, index) => <ContextBlockRow key={`${block.kind}:${index}`} block={block} index={index} />)}
      {context && context.blocks.length === 0 && <div className="border-t border-border px-3 py-4 text-xs text-muted-foreground">当前 context 没有 materialized blocks。</div>}
    </div>
  );
  if (embedded) return card;
  return <div className="border-b border-border bg-background px-5 py-3">{card}</div>;
}

export function SessionProjectionView({
  agentRunTarget = null,
  refreshKey = 0,
  tokenUsage = null,
  compactContextCommand,
  embedded = false,
}: SessionProjectionViewProps) {
  const targetKey = agentRunTarget
    ? `${agentRunTarget.runId}:${agentRunTarget.agentId}`
    : "no-agent-run-target";
  return (
    <SessionProjectionViewForTarget
      key={targetKey}
      agentRunTarget={agentRunTarget}
      refreshKey={refreshKey}
      tokenUsage={tokenUsage}
      compactContextCommand={compactContextCommand}
      embedded={embedded}
    />
  );
}

function SessionProjectionViewForTarget({
  agentRunTarget,
  refreshKey,
  tokenUsage,
  compactContextCommand,
  embedded,
}: Required<Pick<SessionProjectionViewProps, "refreshKey" | "embedded">>
  & Omit<SessionProjectionViewProps, "refreshKey" | "embedded">) {
  const [context, setContext] = useState<RuntimeContextView | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generationRef = useRef(0);
  const latestRequestRef = useRef<RuntimeContextRequestToken | null>(null);
  const mountedRef = useRef(true);
  const targetKey = agentRunTarget
    ? `${agentRunTarget.runId}:${agentRunTarget.agentId}`
    : "no-agent-run-target";

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    if (!agentRunTarget) return;
    const request = {
      target_key: targetKey,
      generation: generationRef.current + 1,
    };
    generationRef.current = request.generation;
    latestRequestRef.current = request;
    setIsLoading(true);
    setError(null);
    try {
      const next = await fetchAgentRunRuntimeContext(agentRunTarget);
      if (!shouldApplyRuntimeContextResponse(mountedRef.current, latestRequestRef.current, request)) return;
      setContext(next);
    } catch (contextError) {
      if (!shouldApplyRuntimeContextResponse(mountedRef.current, latestRequestRef.current, request)) return;
      setError(contextError instanceof Error ? contextError.message : "加载模型上下文失败");
    } finally {
      if (shouldApplyRuntimeContextResponse(mountedRef.current, latestRequestRef.current, request)) {
        setIsLoading(false);
      }
    }
  }, [agentRunTarget, targetKey]);

  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (!cancelled) void refresh();
    });
    return () => {
      cancelled = true;
    };
  }, [refresh, refreshKey]);

  return (
    <SessionProjectionViewPanel
      context={context}
      agentRunTarget={agentRunTarget}
      tokenUsage={tokenUsage}
      compactContextCommand={compactContextCommand}
      isLoading={isLoading}
      error={error}
      onRefresh={() => { void refresh(); }}
      embedded={embedded}
    />
  );
}

export default SessionProjectionView;
