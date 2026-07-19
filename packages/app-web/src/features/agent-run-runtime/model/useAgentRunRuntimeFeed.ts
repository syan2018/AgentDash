import { useMemo } from "react";

import type {
  ManagedRuntimeCommandAvailability,
  ManagedRuntimePlatformChange,
  ManagedRuntimeSnapshot,
} from "../../../generated/agent-runtime-validators";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import type { TokenUsageInfo } from "../../session/model/tokenUsage";
import { useManagedRuntimeFeed } from "./useManagedRuntimeFeed";

export interface UseAgentRunRuntimeFeedOptions {
  agentRunTarget?: AgentRunRuntimeTarget | null;
  enabled?: boolean;
}

export type AgentRunRuntimeItem = ManagedRuntimeSnapshot["items"][number];
export type AgentRunRuntimeInteraction =
  ManagedRuntimeSnapshot["interactions"][number];
export type AgentRunRuntimeTurnStatus =
  | "active"
  | "completed"
  | "failed"
  | "interrupted"
  | "lost";

export interface AgentRunRuntimeTurnActivityStatus {
  kind: "connecting" | "reconnecting" | "retry_exhausted";
  label: string;
  phase?: string;
  attempt?: number;
  maxAttempts?: number;
}

export interface AgentRunRuntimeTurnSegment {
  turnId: string | null;
  status: AgentRunRuntimeTurnStatus;
  startedAtMs?: number;
  durationMs?: number;
  activity?: AgentRunRuntimeTurnActivityStatus;
  items: AgentRunRuntimeItem[];
  finalOutput: AgentRunRuntimeItem | null;
}

export interface UseAgentRunRuntimeFeedResult {
  snapshot: ManagedRuntimeSnapshot | null;
  changes: ManagedRuntimePlatformChange[];
  interactions: AgentRunRuntimeInteraction[];
  commandAvailability: ManagedRuntimeSnapshot["command_availability"];
  displayItems: AgentRunRuntimeItem[];
  turnSegments: AgentRunRuntimeTurnSegment[];
  rawEntries: AgentRunRuntimeItem[];
  isConnected: boolean;
  isLoading: boolean;
  isReceiving: boolean;
  error: Error | null;
  reconnect: () => void;
  close: () => void;
  streamingEntryId: string | null;
  tokenUsage: TokenUsageInfo | null;
}

function runtimeTimestampMs(value: bigint | null): number | undefined {
  if (value === null) return undefined;
  if (value > 9_007_199_254_740_991n) {
    throw new RangeError(
      "Managed Runtime presentation timestamp exceeds JavaScript safe integer range",
    );
  }
  return Number(value);
}

function turnStatus(
  status: ManagedRuntimeSnapshot["turns"][number]["status"],
): AgentRunRuntimeTurnStatus {
  return status === "accepted" || status === "running" ? "active" : status;
}

function turnSegments(
  snapshot: ManagedRuntimeSnapshot,
): AgentRunRuntimeTurnSegment[] {
  const byId = new Map(snapshot.items.map((item) => [item.id, item]));
  return snapshot.turns.map((turn) => {
    const items = turn.item_ids
      .map((id) => byId.get(id))
      .filter((item): item is AgentRunRuntimeItem => item !== undefined);
    const firstStartedAt = items
      .map((item) => item.presentation.started_at_ms)
      .find((value) => value !== null) ?? null;
    const terminal = [...items]
      .reverse()
      .find((item) => item.presentation.terminal !== null)
      ?.presentation.terminal;
    return {
      turnId: turn.id,
      status: turnStatus(turn.status),
      startedAtMs: runtimeTimestampMs(firstStartedAt),
      durationMs: runtimeTimestampMs(terminal?.duration_ms ?? null),
      items,
      finalOutput:
        [...items]
          .reverse()
          .find((item) => item.presentation.body.kind === "agent_message") ??
        null,
    };
  });
}

export interface AgentRunRuntimeProjection {
  displayItems: AgentRunRuntimeItem[];
  turnSegments: AgentRunRuntimeTurnSegment[];
  rawEntries: AgentRunRuntimeItem[];
  interactions: AgentRunRuntimeInteraction[];
}

export function projectAgentRunRuntimeSnapshot(
  snapshot: ManagedRuntimeSnapshot,
): AgentRunRuntimeProjection {
  return {
    displayItems: snapshot.items,
    turnSegments: turnSegments(snapshot),
    rawEntries: snapshot.items,
    interactions: snapshot.interactions,
  };
}

export function useAgentRunRuntimeFeed({
  agentRunTarget = null,
  enabled = true,
}: UseAgentRunRuntimeFeedOptions): UseAgentRunRuntimeFeedResult {
  const feed = useManagedRuntimeFeed({ agentRunTarget, enabled });
  const projection = useMemo(
    () => (feed.snapshot ? projectAgentRunRuntimeSnapshot(feed.snapshot) : null),
    [feed.snapshot],
  );
  const rawEntries = projection?.rawEntries ?? [];
  const isReceiving = feed.snapshot?.active_turn_id != null;

  return {
    snapshot: feed.snapshot,
    changes: feed.changes,
    interactions: projection?.interactions ?? [],
    commandAvailability: feed.snapshot?.command_availability ?? {},
    displayItems: projection?.displayItems ?? [],
    turnSegments: projection?.turnSegments ?? [],
    rawEntries,
    isConnected: feed.lifecycle === "connected",
    isLoading: feed.isLoading,
    isReceiving,
    error: feed.error,
    reconnect: feed.reconnect,
    close: feed.close,
    streamingEntryId:
      isReceiving && rawEntries.length > 0
        ? rawEntries[rawEntries.length - 1]?.id ?? null
        : null,
    tokenUsage: null,
  };
}

export function commandIsAvailable(
  availability: ManagedRuntimeCommandAvailability | undefined,
): boolean {
  return availability?.status === "available";
}
