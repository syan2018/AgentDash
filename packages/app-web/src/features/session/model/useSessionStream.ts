import { useEffect, useMemo } from "react";

import type { CanonicalConversationRecord } from "../../../generated/backbone-protocol";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import { useManagedRuntimeFeed } from "../../agent-run-runtime/model/useManagedRuntimeFeed";
import {
  createInitialStreamState,
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
  reconnect: () => void;
  close: () => void;
}

const EMPTY_INITIAL_ENTRIES: SessionDisplayEntry[] = [];

interface RuntimePresentationCoordinate {
  runtimeSequence: bigint | null;
  baseline: boolean;
}

function presentationCoordinates(
  records: readonly CanonicalConversationRecord[],
): Map<string, RuntimePresentationCoordinate> {
  const coordinates = new Map<string, RuntimePresentationCoordinate>();
  for (const record of records) {
    coordinates.set(record.presentation_id, {
      runtimeSequence: null,
      baseline: true,
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
  const feed = useManagedRuntimeFeed({
    agentRunTarget,
    enabled,
  });
  const records = feed.snapshot?.conversation_history ?? [];
  const coordinates = useMemo(
    () => presentationCoordinates(records),
    [records],
  );
  const events = useMemo(
    () =>
      records.map((record, index) =>
        toSessionEvent(
          record,
          index + 1,
          coordinates.get(record.presentation_id) ?? {
            runtimeSequence: null,
            baseline: true,
          },
        )
      ),
    [coordinates, records],
  );
  const state = useMemo(
    () => reduceStreamState(createInitialStreamState(initialEntries), events),
    [events, initialEntries],
  );
  const baselineBoundary = useMemo(() => {
    let boundary = 0;
    for (const event of events) {
      if (event.baseline) boundary = event.event_seq;
    }
    return feed.snapshot ? boundary : null;
  }, [events, feed.snapshot]);
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
    isReceiving: feed.snapshot?.active_turn_id != null,
    error: feed.error,
    tokenUsage: state.tokenUsage,
    reconnect: feed.reconnect,
    close: feed.close,
  };
}

export default useSessionStream;
