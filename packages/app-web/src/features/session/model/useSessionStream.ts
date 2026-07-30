import { useCallback, useEffect, useRef, useState } from "react";

import type { CanonicalConversationRecord } from "../../../generated/backbone-protocol";
import type {
  AgentRuntimeOperationReceipt,
  AgentRuntimeUpdate,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-codecs";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import type { AgentRunProductRuntimeCommandRequest } from "../../../services/agentRunRuntime";
import { useAgentRuntimeConnection } from "../../agent-run-runtime/model/useAgentRuntimeConnection";
import {
  createInitialStreamState,
  constrainStreamStateToActiveTurn,
  reduceStreamState,
} from "./sessionStreamReducer";
import type {
  SessionDisplayEntry,
  SessionEventEnvelope,
  TokenUsageInfo,
} from "./types";

export interface UseSessionStreamOptions {
  agentRunTarget?: AgentRunRuntimeTarget | null;
  enabled?: boolean;
  initialEntries?: SessionDisplayEntry[];
  onConnectionChange?: (connected: boolean) => void;
  onError?: (error: Error) => void;
}

export interface UseSessionStreamResult {
  entries: SessionDisplayEntry[];
  rawEvents: SessionEventEnvelope[];
  historyReplayBoundarySeq: number | null;
  providerWaitingSeqs: ReadonlyMap<string, number>;
  boundTargetKey: string | null;
  isConnected: boolean;
  isLoading: boolean;
  isReceiving: boolean;
  error: Error | null;
  tokenUsage: TokenUsageInfo | null;
  runtimeView: AgentRuntimeView | null;
  executeRuntimeCommand: (
    request: AgentRunProductRuntimeCommandRequest,
  ) => Promise<AgentRuntimeOperationReceipt>;
  refresh: () => Promise<void>;
  reconnect: () => void;
  close: () => void;
}

const EMPTY_INITIAL_ENTRIES: SessionDisplayEntry[] = [];
interface RuntimePresentationCoordinate {
  runtimeSequence: bigint | null;
  baseline: boolean;
}

export function presentationCoordinates(
  records: readonly CanonicalConversationRecord[],
  baselinePresentationIds: ReadonlySet<string>,
): Map<string, RuntimePresentationCoordinate> {
  const coordinates = new Map<string, RuntimePresentationCoordinate>();
  for (const record of records) {
    coordinates.set(record.presentation_id, {
      runtimeSequence: null,
      baseline: baselinePresentationIds.has(record.presentation_id),
    });
  }
  return coordinates;
}

function observedAtMs(record: CanonicalConversationRecord): number {
  const value = Date.parse(record.presentation.envelope.observedAt);
  if (!Number.isFinite(value)) {
    throw new Error(
      `canonical presentation ${record.presentation_id} has an invalid observedAt`,
    );
  }
  return value;
}

function toSessionEvent(
  record: CanonicalConversationRecord,
  eventSeq: number,
  coordinate: RuntimePresentationCoordinate,
): SessionEventEnvelope {
  const envelope = record.presentation.envelope;
  const occurredAtMs = observedAtMs(record);
  const ephemeral = record.presentation.durability === "ephemeral";
  return {
    session_id: envelope.sessionId,
    event_seq: eventSeq,
    occurred_at_ms: occurredAtMs,
    committed_at_ms: ephemeral ? null : occurredAtMs,
    session_update_type: envelope.event.type,
    turn_id: envelope.trace.turnId ?? null,
    entry_index: envelope.trace.entryIndex ?? null,
    tool_call_id: null,
    notification: envelope,
    ephemeral,
    presentation_id: record.presentation_id,
    runtime_change_sequence: coordinate.runtimeSequence,
    baseline: coordinate.baseline,
  };
}

export function useSessionStream({
  agentRunTarget = null,
  enabled = true,
  initialEntries = EMPTY_INITIAL_ENTRIES,
  onConnectionChange,
  onError,
}: UseSessionStreamOptions): UseSessionStreamResult {
  const [state, setState] = useState(() =>
    createInitialStreamState(initialEntries)
  );
  const [baselineBoundary, setBaselineBoundary] = useState<number | null>(null);
  const eventSeqRef = useRef(0);
  const activeTurnIdRef = useRef<string | null>(null);
  const presentationSignaturesRef = useRef(new Map<string, string>());
  const lastBaselineRef = useRef<AgentRuntimeView | null>(null);

  const foldBaseline = useCallback((view: AgentRuntimeView) => {
    lastBaselineRef.current = view;
    const records = view.observation.conversation;
    const baselineIds = new Set(
      records.map((record) => record.presentation_id),
    );
    const coordinates = presentationCoordinates(records, baselineIds);
    const events = records.map((record, index) =>
      toSessionEvent(
        record,
        index + 1,
        coordinates.get(record.presentation_id) ?? {
          runtimeSequence: null,
          baseline: true,
        },
      )
    );
    eventSeqRef.current = events.length;
    activeTurnIdRef.current =
      view.observation.execution.active_turn?.turn_id ?? null;
    presentationSignaturesRef.current = new Map(
      records.map((record) => [
        record.presentation_id,
        JSON.stringify(record.presentation),
      ]),
    );
    setBaselineBoundary(events.length);
    setState(
      constrainStreamStateToActiveTurn(
        reduceStreamState(createInitialStreamState(initialEntries), events),
        activeTurnIdRef.current,
      ),
    );
  }, [initialEntries]);

  const foldUpdate = useCallback((update: AgentRuntimeUpdate) => {
    if (update.state) {
      activeTurnIdRef.current =
        update.state.execution.active_turn?.turn_id ?? null;
    }
    const events: SessionEventEnvelope[] = [];
    for (const record of update.presentations) {
      const signature = JSON.stringify(record.presentation);
      if (presentationSignaturesRef.current.get(record.presentation_id) === signature) {
        continue;
      }
      presentationSignaturesRef.current.set(record.presentation_id, signature);
      eventSeqRef.current += 1;
      events.push(
        toSessionEvent(record, eventSeqRef.current, {
          runtimeSequence: update.lane_sequence,
          baseline: false,
        }),
      );
    }
    setState((current) =>
      constrainStreamStateToActiveTurn(
        reduceStreamState(current, events),
        activeTurnIdRef.current,
      )
    );
  }, []);

  const foldReset = useCallback(() => {
    const baseline = lastBaselineRef.current;
    if (baseline) foldBaseline(baseline);
  }, [foldBaseline]);

  const feed = useAgentRuntimeConnection({
    agentRunTarget,
    enabled,
    onBaseline: foldBaseline,
    onUpdate: foldUpdate,
    onReset: foldReset,
  });
  useEffect(() => {
    onConnectionChange?.(feed.lifecycle === "connected");
  }, [feed.lifecycle, onConnectionChange]);

  useEffect(() => {
    if (feed.error) onError?.(feed.error);
  }, [feed.error, onError]);

  return {
    entries: state.entries,
    rawEvents: state.rawEvents,
    historyReplayBoundarySeq: baselineBoundary,
    providerWaitingSeqs: state.providerWaitingSeqs,
    boundTargetKey: feed.boundTargetKey,
    isConnected: feed.lifecycle === "connected",
    isLoading: feed.isLoading,
    isReceiving: feed.view?.observation.execution.active_turn != null,
    error: feed.error,
    tokenUsage: state.tokenUsage,
    runtimeView: feed.view,
    executeRuntimeCommand: feed.execute,
    refresh: feed.refresh,
    reconnect: feed.reconnect,
    close: feed.close,
  };
}

export default useSessionStream;
