import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type {
  RuntimeEventEnvelope,
  RuntimeInteractionKind,
  RuntimeInteractionRequest,
  RuntimeInteractionTerminal,
  RuntimeItemContent,
  RuntimeSnapshot,
  DynamicToolCallOutputContentItem,
} from "../../../generated/agent-runtime-contracts";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import { createRuntimeEventStream, type RuntimeEventStream } from "./runtimeEventStream";

export interface AgentRuntimeFeedEntry {
  id: string;
  turn_id: string | null;
  role: "user" | "agent" | "reasoning" | "tool" | "system";
  text: string;
  status: "streaming" | "completed" | "failed" | "cancelled" | "lost";
  interaction?: {
    interaction_id: string;
    interaction_kind: RuntimeInteractionKind;
    terminal: RuntimeInteractionTerminal | null;
  };
}

interface RuntimeFeedState {
  entries: AgentRuntimeFeedEntry[];
  cursor: number;
}

function toolContentText(items: DynamicToolCallOutputContentItem[] | null): string {
  return (items ?? []).map((item) => {
    switch (item.type) {
      case "inputText": return item.text;
      case "inputImage": return item.imageUrl;
    }
  }).join("\n");
}

export interface UseAgentRuntimeFeedOptions {
  target: AgentRunRuntimeTarget | null;
  snapshot: RuntimeSnapshot | null;
  enabled: boolean;
  onTurnTerminal?: () => void;
  onTaskPlanChanged?: () => void;
  onRuntimeInspectInvalidated?: () => void;
}

export interface UseAgentRuntimeFeedResult {
  entries: AgentRuntimeFeedEntry[];
  durableCursor: number;
  isConnected: boolean;
  isLoading: boolean;
  error: Error | null;
  reconnect: () => void;
}

function contentView(content: RuntimeItemContent): Pick<AgentRuntimeFeedEntry, "role" | "text"> {
  switch (content.type) {
    case "userMessage":
      return {
        role: "user",
        text: content.content.map((item) => {
          switch (item.type) {
            case "text": return item.text;
            case "image": return item.url;
            case "localImage": return item.path;
            case "skill": return item.name;
            case "mention": return item.path;
          }
        }).join("\n"),
      };
    case "agentMessage": return { role: "agent", text: content.text };
    case "reasoning": return { role: "reasoning", text: [...(content.summary ?? []), ...(content.content ?? [])].join("\n") };
    case "plan": return { role: "system", text: content.text };
    case "commandExecution": return { role: "tool", text: content.aggregatedOutput ?? content.command };
    case "fileChange": return { role: "tool", text: content.status };
    case "mcpToolCall": return { role: "tool", text: `${content.server}/${content.tool}` };
    case "dynamicToolCall": return { role: "tool", text: content.tool };
    case "collabAgentToolCall": return { role: "tool", text: content.tool };
    case "subAgentActivity": return { role: "tool", text: content.kind };
    case "webSearch": return { role: "tool", text: content.query };
    case "imageView": return { role: "tool", text: content.path };
    case "sleep": return { role: "tool", text: `${content.durationMs}ms` };
    case "imageGeneration": return { role: "tool", text: content.result };
    case "hookPrompt": return { role: "system", text: "Hook prompt" };
    case "enteredReviewMode": return { role: "system", text: content.review };
    case "exitedReviewMode": return { role: "system", text: content.review };
    case "contextCompaction": return { role: "system", text: "上下文已压缩" };
    case "shellExec": return { role: "tool", text: content.aggregatedOutput ?? content.command };
    case "terminalControl": return { role: "tool", text: content.aggregatedOutput ?? `${content.operation}: ${content.terminalId}` };
    case "fsRead": return { role: "tool", text: content.path };
    case "fsGrep": return { role: "tool", text: content.pattern };
    case "fsGlob": return { role: "tool", text: content.pattern };
    case "vfs": return { role: "tool", text: toolContentText(content.contentItems) || content.resourceUri || content.operation };
    case "runtimeAction": return { role: "tool", text: toolContentText(content.contentItems) || content.actionKey };
    case "workspaceModule": return { role: "tool", text: toolContentText(content.contentItems) || content.resourceUri || content.operation };
    case "companion": return { role: "tool", text: toolContentText(content.contentItems) || content.operation };
    case "task": return { role: "tool", text: toolContentText(content.contentItems) || content.taskId || content.operation };
    case "wait": return { role: "tool", text: toolContentText(content.contentItems) || (content.durationMs == null ? "wait" : `${content.durationMs}ms`) };
    case "lifecycleComplete": return { role: "tool", text: toolContentText(content.contentItems) || content.nodeId || "lifecycle complete" };
  }
}

function seedFromSnapshot(snapshot: RuntimeSnapshot | null): AgentRuntimeFeedEntry[] {
  return (snapshot?.transcript ?? []).map((item) => ({
    id: item.item_id,
    turn_id: item.turn_id,
    ...contentView(item.final_content),
    status: "completed" as const,
  }));
}

function interactionPrompt(request: RuntimeInteractionRequest): string {
  switch (request.kind) {
    case "command_approval": return request.params.reason ?? request.params.command ?? "Approve command execution?";
    case "file_change_approval": return request.params.reason ?? "Approve file changes?";
    case "permission_approval": return request.params.reason ?? "Approve requested permissions?";
    case "user_input_request": return request.params.questions.map((question) => question.question).join("\n");
    case "mcp_elicitation": return request.params.message;
    case "dynamic_tool_execution": return request.params.tool;
  }
}

export function applyRuntimeEvent(
  entries: AgentRuntimeFeedEntry[],
  envelope: RuntimeEventEnvelope,
  baselineItemIds: ReadonlySet<string>,
): AgentRuntimeFeedEntry[] {
  const event = envelope.event;
  if (
    (event.kind === "item_started" || event.kind === "conversation_delta" || event.kind === "item_terminal")
    && baselineItemIds.has(event.item_id)
  ) {
    return entries;
  }
  const next = [...entries];
  if (event.kind === "item_started") {
    const view = contentView(event.initial_content);
    next.push({ id: event.item_id, turn_id: event.turn_id, ...view, status: "streaming" });
    return next;
  }
  if (event.kind === "conversation_delta") {
    const index = next.findIndex((item) => item.id === event.item_id);
    const delta = (() => {
      switch (event.delta.kind) {
        case "mcp_progress": return event.delta.message;
        case "tool_progress": return toolContentText(event.delta.content_items);
        case "agent_message":
        case "reasoning_text":
        case "reasoning_summary":
        case "command_output":
        case "file_change_output":
        case "plan":
          return event.delta.delta;
      }
    })();
    if (index >= 0) next[index] = { ...next[index], text: `${next[index].text}${delta}` };
    return next;
  }
  if (event.kind === "item_terminal") {
    const index = next.findIndex((item) => item.id === event.item_id);
    if (event.terminal.kind === "completed") {
      const view = contentView(event.terminal.final_content);
      const item = { id: event.item_id, turn_id: event.turn_id, ...view, status: "completed" as const };
      if (index >= 0) next[index] = item;
      else next.push(item);
    } else if (index >= 0) {
      next[index] = { ...next[index], status: event.terminal.kind };
    }
    return next;
  }
  if (event.kind === "interaction_requested") {
    const interaction = {
      interaction_id: event.interaction_id,
      interaction_kind: event.request.kind,
      terminal: null,
    };
    const index = next.findIndex((entry) => entry.interaction?.interaction_id === event.interaction_id);
    const entry: AgentRuntimeFeedEntry = {
      id: `interaction:${event.interaction_id}`,
      turn_id: event.turn_id,
      role: "system",
      text: interactionPrompt(event.request),
      status: "streaming",
      interaction,
    };
    if (index >= 0) next[index] = entry;
    else next.push(entry);
    return next;
  }
  if (event.kind === "interaction_terminal") {
    const index = next.findIndex((entry) => entry.interaction?.interaction_id === event.interaction_id);
    if (index < 0) {
      next.push({
        id: `interaction:${event.interaction_id}`,
        turn_id: event.turn_id,
        role: "system",
        text: `Interaction ${event.interaction_id}: ${event.terminal}`,
        status: interactionTerminalStatus(event.terminal),
      });
      return next;
    }
    const current = next[index];
    if (!current?.interaction) return next;
    next[index] = {
      ...current,
      status: interactionTerminalStatus(event.terminal),
      interaction: { ...current.interaction, terminal: event.terminal },
    };
    return next;
  }
  return next;
}

function interactionTerminalStatus(
  terminal: RuntimeInteractionTerminal,
): AgentRuntimeFeedEntry["status"] {
  switch (terminal) {
    case "resolved": return "completed";
    case "failed": return "failed";
    case "lost": return "lost";
    case "cancelled":
    case "expired":
      return "cancelled";
  }
}

export function useAgentRuntimeFeed(options: UseAgentRuntimeFeedOptions): UseAgentRuntimeFeedResult {
  const [state, setState] = useState<RuntimeFeedState>({ entries: [], cursor: 0 });
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [connectKey, setConnectKey] = useState(0);
  const streamRef = useRef<RuntimeEventStream | null>(null);
  const cursorRef = useRef(0);
  const streamTargetKeyRef = useRef<string | null>(null);
  const callbackRef = useRef({
    onTurnTerminal: options.onTurnTerminal,
    onTaskPlanChanged: options.onTaskPlanChanged,
    onRuntimeInspectInvalidated: options.onRuntimeInspectInvalidated,
  });
  const baselineItemIds = useMemo(
    () => new Set((options.snapshot?.transcript ?? []).map((item) => item.item_id)),
    [options.snapshot?.transcript],
  );
  const baselineItemIdsRef = useRef<ReadonlySet<string>>(baselineItemIds);
  const targetKey = options.target ? `${options.target.runId}:${options.target.agentId}` : null;

  useEffect(() => {
    callbackRef.current = {
      onTurnTerminal: options.onTurnTerminal,
      onTaskPlanChanged: options.onTaskPlanChanged,
      onRuntimeInspectInvalidated: options.onRuntimeInspectInvalidated,
    };
  }, [options.onRuntimeInspectInvalidated, options.onTaskPlanChanged, options.onTurnTerminal]);

  useEffect(() => {
    baselineItemIdsRef.current = baselineItemIds;
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      setState((current) => ({
        entries: [
          ...seedFromSnapshot(options.snapshot),
          ...(streamTargetKeyRef.current === targetKey
            ? current.entries.filter((entry) => entry.interaction != null)
            : []),
        ],
        cursor: current.cursor,
      }));
    });
    return () => { cancelled = true; };
  }, [baselineItemIds, options.snapshot, targetKey]);

  useEffect(() => {
    streamRef.current?.close();
    streamRef.current = null;
    const targetChanged = streamTargetKeyRef.current !== targetKey;
    streamTargetKeyRef.current = targetKey;
    if (targetChanged) {
      cursorRef.current = 0;
    }
    queueMicrotask(() => {
      setError(null);
      setIsConnected(false);
    });
    if (!options.enabled || !options.target) {
      queueMicrotask(() => setIsLoading(false));
      return;
    }
    queueMicrotask(() => setIsLoading(true));
    const stream = createRuntimeEventStream({
      target: options.target,
      after: cursorRef.current,
      onEvent: (envelope, durableCursor) => {
        if (durableCursor != null && durableCursor <= cursorRef.current) return;
        if (durableCursor != null) cursorRef.current = durableCursor;
        setState((current) => ({
          cursor: cursorRef.current,
          entries: applyRuntimeEvent(current.entries, envelope, baselineItemIdsRef.current),
        }));
        if (envelope.event.kind === "turn_terminal") callbackRef.current.onTurnTerminal?.();
        if (runtimeEventRequestsRuntimeInspectRefresh(envelope)) {
          callbackRef.current.onRuntimeInspectInvalidated?.();
        }
        if (
          envelope.event.kind === "item_terminal"
          && envelope.event.terminal.kind === "completed"
          && envelope.event.terminal.final_content.type === "dynamicToolCall"
          && envelope.event.terminal.final_content.tool === "task_write"
        ) {
          callbackRef.current.onTaskPlanChanged?.();
        }
      },
      onLifecycleChange: (lifecycle) => {
        setIsConnected(lifecycle === "connected");
        setIsLoading(lifecycle === "connecting" || lifecycle === "reconnecting");
      },
      onError: (streamError) => {
        setError(streamError);
        setIsConnected(false);
      },
    });
    streamRef.current = stream;
    return () => {
      stream.close();
      if (streamRef.current === stream) streamRef.current = null;
    };
  }, [connectKey, options.enabled, options.target, targetKey]);

  const reconnect = useCallback(() => setConnectKey((value) => value + 1), []);
  return {
    entries: state.entries,
    durableCursor: state.cursor,
    isConnected,
    isLoading,
    error,
    reconnect,
  };
}

export function runtimeEventRequestsRuntimeInspectRefresh(
  envelope: RuntimeEventEnvelope,
): boolean {
  return envelope.event.kind === "turn_started"
    || envelope.event.kind === "turn_terminal"
    || envelope.event.kind === "interaction_requested"
    || envelope.event.kind === "interaction_terminal";
}
