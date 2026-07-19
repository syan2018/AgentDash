import type { ManagedRuntimePlatformChange } from "../../../generated/agent-runtime-validators";
import type { ConversationEffectiveExecutorConfigView } from "../../../generated/project-agent-contracts";
import type { ProjectAgentExecutor } from "../../../types";
import type { TaskSessionExecutorSummary } from "../../../types/context";
import type { ExecutorConfigSource } from "../../executor-selector/model/types";

export function isAgentRunWorkspaceActionRunning(input: {
  executionStatus: string;
}): boolean {
  return input.executionStatus === "starting_claimed"
    || input.executionStatus === "running_active"
    || input.executionStatus === "cancelling";
}

export function toExecutorConfigSource(
  defaults:
    | ProjectAgentExecutor
    | TaskSessionExecutorSummary
    | ConversationEffectiveExecutorConfigView
    | null
    | undefined,
): ExecutorConfigSource | null {
  if (!defaults) return null;
  const source: ExecutorConfigSource = {};
  if (defaults.executor) source.executor = defaults.executor;
  if (defaults.provider_id) source.providerId = defaults.provider_id;
  if (defaults.model_id) source.modelId = defaults.model_id;
  if (defaults.thinking_level) source.thinkingLevel = defaults.thinking_level;
  return Object.keys(source).length === 0 ? null : source;
}

function normalizeExecutorToken(raw: string): string {
  return raw.trim().replace(/[-\s]+/g, "_").toUpperCase();
}

export function resolveExecutorFromHint(
  hint: string | null | undefined,
  executors: Array<{ id: string }>,
): string | null {
  const trimmed = (hint ?? "").trim();
  if (!trimmed) return null;
  const exact = executors.find((item) => item.id === trimmed);
  if (exact) return exact.id;
  const normalized = normalizeExecutorToken(trimmed);
  const matched = executors.find(
    (item) => normalizeExecutorToken(item.id) === normalized,
  );
  return matched?.id ?? trimmed;
}

/**
 * Projection refresh is driven only by committed canonical Runtime changes.
 * The sequence itself is the refresh key; worker or request timing is not
 * consulted.
 */
export function computeAgentRunRuntimeProjectionRefreshKey(
  changes: readonly ManagedRuntimePlatformChange[],
): bigint {
  let refreshKey = 0n;
  for (const change of changes) {
    if (
      change.delta.kind === "source_projection_changed"
      || change.delta.kind === "runtime_lifecycle_changed"
      || change.delta.kind === "source_binding_changed"
    ) {
      if (change.sequence > refreshKey) {
        refreshKey = change.sequence;
      }
    }
  }
  return refreshKey;
}
