import { useCallback, useEffect, useRef, useState } from "react";
import {
  fetchAgentRunRuntimeInspect,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";
import { createRuntimeEventStream, type RuntimeEventStream } from "../../agent-run-workspace/model/runtimeEventStream";
import type { SessionDisplayEntry, SessionEventEnvelope, TokenUsageInfo } from "./types";
import {
  createInitialStreamState,
  reduceStreamState,
  type SessionStreamState,
} from "./sessionStreamReducer";
import { projectRuntimeEnvelope, projectRuntimeSnapshot, runtimeSnapshotCursor } from "./runtimeSessionAdapter";
import type { RuntimeEvent } from "../../../generated/agent-runtime-contracts";

export interface UseSessionStreamOptions {
  agentRunTarget?: AgentRunRuntimeTarget | null;
  enabled?: boolean;
  initialEntries?: SessionDisplayEntry[];
  onConnectionChange?: (connected: boolean) => void;
  onError?: (error: Error) => void;
  onRuntimeInspectInvalidated?: () => void;
}

export interface UseSessionStreamResult {
  entries: SessionDisplayEntry[];
  rawEvents: SessionEventEnvelope[];
  historyReplayBoundarySeq: number | null;
  providerWaitingSeqs: ReadonlyMap<string, number>;
  isConnected: boolean;
  isLoading: boolean;
  isReceiving: boolean;
  error: Error | null;
  tokenUsage: TokenUsageInfo | null;
  reconnect: () => void;
  close: () => void;
}

const EMPTY_INITIAL_ENTRIES: SessionDisplayEntry[] = [];

export function useSessionStream(options: UseSessionStreamOptions): UseSessionStreamResult {
  const {
    agentRunTarget = null,
    enabled = true,
    initialEntries = EMPTY_INITIAL_ENTRIES,
    onConnectionChange,
    onError,
    onRuntimeInspectInvalidated,
  } = options;
  const [streamState, setStreamState] = useState<SessionStreamState>(() => createInitialStreamState(initialEntries));
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [isReceiving, setIsReceiving] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [connectKey, setConnectKey] = useState(0);
  const [historyReplayBoundarySeq, setHistoryReplayBoundarySeq] = useState<number | null>(null);
  const streamRef = useRef<RuntimeEventStream | null>(null);
  const receivingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const invalidationRef = useRef(onRuntimeInspectInvalidated);
  const currentTargetKeyRef = useRef<string | null>(null);
  useEffect(() => { invalidationRef.current = onRuntimeInspectInvalidated; }, [onRuntimeInspectInvalidated]);
  useEffect(() => {
    currentTargetKeyRef.current = agentRunTarget == null ? null : `${agentRunTarget.runId}:${agentRunTarget.agentId}`;
  }, [agentRunTarget]);

  useEffect(() => {
    streamRef.current?.close();
    streamRef.current = null;
    if (!enabled || !agentRunTarget) {
      // Target removal is an explicit external stream lifecycle transition.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setStreamState(createInitialStreamState(initialEntries));
      setIsConnected(false);
      setIsLoading(false);
      setHistoryReplayBoundarySeq(null);
      return;
    }

    let cancelled = false;
    setIsLoading(true);
    setError(null);
    setIsConnected(false);
    setHistoryReplayBoundarySeq(null);
    setStreamState(createInitialStreamState(initialEntries));

    void fetchAgentRunRuntimeInspect(agentRunTarget).then((inspect) => {
      if (cancelled) return;
      const snapshotEvents = projectRuntimeSnapshot(inspect.snapshot);
      let baseline = createInitialStreamState(initialEntries);
      baseline = reduceStreamState(baseline, snapshotEvents);
      const snapshotCursor = runtimeSnapshotCursor(inspect.snapshot);
      const streamTargetKey = `${agentRunTarget.runId}:${agentRunTarget.agentId}`;
      baseline = {
        ...baseline,
        lastAppliedSeq: snapshotCursor,
        lastEphemeralSeq: 0,
        lastEphemeralGeneration: null,
      };
      setStreamState(baseline);
      // Snapshot is the historical presentation baseline. Durable events arriving after this
      // point are reducer facts, while control-plane invalidation remains owned by Runtime inspect.
      setHistoryReplayBoundarySeq(snapshotCursor);

      streamRef.current = createRuntimeEventStream({
        target: agentRunTarget,
        after: snapshotCursor,
        onEvent: (envelope, durableCursor) => {
          if (cancelled) return;
          if (shouldInvalidateRuntimeInspect({
            event: envelope.event,
            durableCursor,
            historyBoundary: snapshotCursor,
            streamTargetKey,
            currentTargetKey: currentTargetKeyRef.current,
            accepted: true,
          })) invalidationRef.current?.();
          const event = projectRuntimeEnvelope(envelope);
          if (!event) return;
          setStreamState((prev) => reduceStreamState(prev, [event]));
          setIsReceiving(true);
          if (receivingTimerRef.current) clearTimeout(receivingTimerRef.current);
          receivingTimerRef.current = setTimeout(() => setIsReceiving(false), 600);
        },
        onLifecycleChange: (lifecycle) => {
          if (cancelled) return;
          const connected = lifecycle === "connected";
          setIsConnected(connected);
          setIsLoading(lifecycle === "connecting" || lifecycle === "reconnecting");
          onConnectionChange?.(connected);
        },
        onError: (streamError) => {
          if (cancelled) return;
          setError(streamError);
          setIsLoading(false);
          onError?.(streamError);
        },
      });
    }).catch((loadError: unknown) => {
      if (cancelled) return;
      const normalized = loadError instanceof Error ? loadError : new Error("加载 Runtime snapshot 失败");
      setError(normalized);
      setIsLoading(false);
      onError?.(normalized);
    });

    return () => {
      cancelled = true;
      streamRef.current?.close();
      streamRef.current = null;
      if (receivingTimerRef.current) clearTimeout(receivingTimerRef.current);
    };
    // Stream identity intentionally follows scalar target coordinates; callback/object identities
    // are not transport coordinates and must not cause reconnect loops.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentRunTarget?.runId, agentRunTarget?.agentId, connectKey, enabled]);

  const close = useCallback(() => {
    streamRef.current?.close();
    streamRef.current = null;
    setIsConnected(false);
    setIsLoading(false);
  }, []);

  const reconnect = useCallback(() => {
    streamRef.current?.close();
    streamRef.current = null;
    setConnectKey((value) => value + 1);
  }, []);

  return {
    entries: streamState.entries,
    rawEvents: streamState.rawEvents,
    historyReplayBoundarySeq,
    providerWaitingSeqs: streamState.providerWaitingSeqs,
    isConnected,
    isLoading,
    isReceiving,
    error,
    tokenUsage: streamState.tokenUsage,
    reconnect,
    close,
  };
}

export default useSessionStream;

export function runtimeEventInvalidatesInspect(event: Pick<RuntimeEvent, "kind">): boolean {
  switch (event.kind) {
    case "turn_started":
    case "turn_terminal":
    case "interaction_requested":
    case "interaction_terminal":
    case "binding_established":
    case "binding_lost":
    case "binding_reestablished":
    case "thread_status_changed":
      return true;
    default:
      return false;
  }
}

export interface RuntimeInspectInvalidationContext {
  event: Pick<RuntimeEvent, "kind">;
  durableCursor: number | null;
  historyBoundary: number;
  streamTargetKey: string;
  currentTargetKey: string | null;
  accepted: boolean;
}

export function shouldInvalidateRuntimeInspect(context: RuntimeInspectInvalidationContext): boolean {
  if (!context.accepted || context.streamTargetKey !== context.currentTargetKey) return false;
  if (context.durableCursor != null && context.durableCursor <= context.historyBoundary) return false;
  return runtimeEventInvalidatesInspect(context.event);
}
