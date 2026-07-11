import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type {
  RuntimeEventEnvelope,
  RuntimeItemContent,
  RuntimeSnapshot,
} from "../../../generated/agent-runtime-contracts";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import { createRuntimeEventStream, type RuntimeEventStream } from "./runtimeEventStream";

export interface AgentRuntimeFeedEntry {
  id: string;
  turn_id: string | null;
  role: "user" | "agent" | "reasoning" | "tool" | "system";
  text: string;
  status: "streaming" | "completed" | "failed" | "cancelled" | "lost";
}

interface RuntimeFeedState {
  entries: AgentRuntimeFeedEntry[];
  cursor: number;
}

export interface UseAgentRuntimeFeedOptions {
  target: AgentRunRuntimeTarget | null;
  snapshot: RuntimeSnapshot | null;
  enabled: boolean;
  onTurnTerminal?: () => void;
  onTaskPlanChanged?: () => void;
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
  switch (content.kind) {
    case "user_message":
      return {
        role: "user",
        text: content.input.map((item) => {
          if (item.kind === "text") return item.text;
          if (item.kind === "file_reference") return item.uri;
          if (item.kind === "image") return `[${item.mime_type}]`;
          return JSON.stringify(item.value);
        }).join("\n"),
      };
    case "agent_message": return { role: "agent", text: content.text };
    case "reasoning": return { role: "reasoning", text: content.text };
    case "tool_call": return { role: "tool", text: `${content.name} ${JSON.stringify(content.arguments)}` };
    case "tool_result": return { role: "tool", text: `${content.name}\n${JSON.stringify(content.output, null, 2)}` };
    case "plan": return { role: "system", text: content.steps.join("\n") };
    case "context_compaction": return { role: "system", text: `上下文已压缩至 ${content.checkpoint_id}` };
    case "system_context_change": return { role: "system", text: `上下文切换至 ${content.checkpoint_id}` };
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

function applyRuntimeEvent(
  entries: AgentRuntimeFeedEntry[],
  envelope: RuntimeEventEnvelope,
  baselineItemIds: ReadonlySet<string>,
): AgentRuntimeFeedEntry[] {
  const event = envelope.event;
  if (
    (event.kind === "item_started" || event.kind === "item_delta" || event.kind === "item_terminal")
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
  if (event.kind === "item_delta") {
    const index = next.findIndex((item) => item.id === event.item_id);
    if (index >= 0) next[index] = { ...next[index], text: `${next[index].text}${event.delta}` };
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
    next.push({
      id: `interaction:${event.interaction_id}`,
      turn_id: event.turn_id,
      role: "system",
      text: event.prompt,
      status: "streaming",
    });
  }
  return next;
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
    };
  }, [options.onTaskPlanChanged, options.onTurnTerminal]);

  useEffect(() => {
    baselineItemIdsRef.current = baselineItemIds;
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      setState((current) => ({
        entries: seedFromSnapshot(options.snapshot),
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
        if (
          envelope.event.kind === "item_terminal"
          && envelope.event.terminal.kind === "completed"
          && envelope.event.terminal.final_content.kind === "tool_result"
          && envelope.event.terminal.final_content.name === "task_write"
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
