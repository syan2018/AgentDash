import { api } from "../api/client";
import { isWorkspaceModulePresentation } from "../features/workspace-module/model/presentation";
import type {
  AgentRunProjectionTarget,
  AgentRunTerminalChangePage,
  AgentRunTerminalOwnerFence,
  AgentRunTerminalProjection,
  AgentRunTerminalSnapshot,
  WorkspaceModulePresentationChangePage,
  WorkspaceModulePresentationIntent,
  WorkspaceModulePresentationSnapshot,
} from "../generated/agent-run-product-projection-contracts";
import {
  agentRunScopedPath,
  type AgentRunRuntimeTarget,
} from "./agentRunRuntime";

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isNonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}

function isProjectionTarget(value: unknown): value is AgentRunProjectionTarget {
  return isRecord(value)
    && typeof value.run_id === "string"
    && typeof value.agent_id === "string";
}

function isSourceBinding(value: unknown): boolean {
  return isRecord(value)
    && typeof value.source_ref === "string"
    && isNonNegativeInteger(value.committed_at_revision)
    && isNonNegativeInteger(value.applied_surface_revision)
    && (value.activated_at_revision === null
      || value.activated_at_revision === undefined
      || isNonNegativeInteger(value.activated_at_revision));
}

function isOwnerFence(value: unknown): value is AgentRunTerminalOwnerFence {
  return isRecord(value)
    && typeof value.terminal_owner_epoch_id === "string"
    && isProjectionTarget(value.target)
    && typeof value.runtime_thread_id === "string"
    && isSourceBinding(value.source_binding)
    && typeof value.backend_id === "string";
}

function isPresentationIntent(value: unknown): value is WorkspaceModulePresentationIntent {
  if (!isRecord(value) || !isRecord(value.cause) || !isRecord(value.currentness_fence)) {
    return false;
  }
  return typeof value.intent_id === "string"
    && typeof value.effect_id === "string"
    && isProjectionTarget(value.target)
    && isRecord(value.actor)
    && ["agent_tool", "user", "system"].includes(String(value.actor.kind))
    && typeof value.actor.actor_id === "string"
    && typeof value.cause.runtime_thread_id === "string"
    && (value.cause.runtime_operation_id === null
      || value.cause.runtime_operation_id === undefined
      || typeof value.cause.runtime_operation_id === "string")
    && typeof value.cause.runtime_turn_id === "string"
    && typeof value.cause.runtime_item_id === "string"
    && typeof value.currentness_fence.runtime_thread_id === "string"
    && isSourceBinding(value.currentness_fence.source_binding)
    && isNonNegativeInteger(value.currentness_fence.surface_revision)
    && typeof value.currentness_fence.module_id === "string"
    && typeof value.currentness_fence.view_key === "string"
    && typeof value.currentness_fence.renderer_kind === "string"
    && typeof value.currentness_fence.presentation_uri === "string"
    && typeof value.presentation_digest === "string"
    && isWorkspaceModulePresentation(value.presentation)
    && isNonNegativeInteger(value.committed_at_ms);
}

function isTerminalChangeOrigin(value: unknown): boolean {
  if (!isRecord(value)) return false;
  if (value.kind === "source_fact") {
    return typeof value.terminal_owner_epoch_id === "string"
      && isNonNegativeInteger(value.source_sequence);
  }
  if (value.kind === "product_fact") {
    return value.change_kind === "backend_availability"
      || value.change_kind === "control_correlation"
      || value.change_kind === "reconcile_lost";
  }
  return false;
}

function isTerminalProjection(value: unknown): value is AgentRunTerminalProjection {
  if (!isRecord(value) || !isRecord(value.output)) return false;
  return typeof value.terminal_id === "string"
    && isOwnerFence(value.owner)
    && (value.mount_id === null || value.mount_id === undefined || typeof value.mount_id === "string")
    && (value.cwd === null || value.cwd === undefined || typeof value.cwd === "string")
    && (value.capability === "interactive" || value.capability === "read_only_output")
    && isNonNegativeInteger(value.max_output_bytes)
    && ["starting", "running", "exited", "killed", "lost"].includes(String(value.state))
    && ["online", "offline", "reconciling"].includes(String(value.availability))
    && isNonNegativeInteger(value.latest_source_sequence)
    && (value.exit_code === null || value.exit_code === undefined || Number.isInteger(value.exit_code))
    && (value.process_id === null || value.process_id === undefined || isNonNegativeInteger(value.process_id))
    && isNonNegativeInteger(value.created_at_ms)
    && (value.exited_at_ms === null
      || value.exited_at_ms === undefined
      || isNonNegativeInteger(value.exited_at_ms))
    && isNonNegativeInteger(value.output.next_sequence)
    && typeof value.output.retained_output === "string"
    && typeof value.output.truncated === "boolean"
    && isNonNegativeInteger(value.output.omitted_bytes);
}

function isTerminalDelta(value: unknown): boolean {
  if (!isRecord(value) || typeof value.kind !== "string") return false;
  if (value.kind === "registered") return isTerminalProjection(value.terminal);
  if (!isOwnerFence(value.owner) || typeof value.terminal_id !== "string") return false;
  switch (value.kind) {
    case "output_appended":
      return isNonNegativeInteger(value.output_sequence)
        && ["stdout", "stderr", "pty"].includes(String(value.stream))
        && typeof value.data === "string";
    case "output_omitted":
      return isNonNegativeInteger(value.output_sequence)
        && isNonNegativeInteger(value.omitted_bytes)
        && typeof value.retained_output === "string";
    case "state_changed":
      return ["starting", "running", "exited", "killed", "lost"].includes(String(value.state))
        && isNonNegativeInteger(value.changed_at_ms);
    case "availability_changed":
      return ["online", "offline", "reconciling"].includes(String(value.availability))
        && isNonNegativeInteger(value.changed_at_ms);
    case "control_correlated":
      return typeof value.correlation_id === "string"
        && ["input", "resize", "terminate", "read", "status"].includes(String(value.control))
        && ["accepted", "completed", "failed"].includes(String(value.status));
    case "removed":
      return true;
    default:
      return false;
  }
}

export function isWorkspaceModulePresentationSnapshot(
  value: unknown,
): value is WorkspaceModulePresentationSnapshot {
  return isRecord(value)
    && isProjectionTarget(value.target)
    && isNonNegativeInteger(value.revision)
    && isNonNegativeInteger(value.latest_change_sequence)
    && isNonNegativeInteger(value.captured_at_ms)
    && Array.isArray(value.pending_intents)
    && value.pending_intents.every((pending) =>
      isRecord(pending)
      && isNonNegativeInteger(pending.change_sequence)
      && isPresentationIntent(pending.intent)
    );
}

export function isWorkspaceModulePresentationChangePage(
  value: unknown,
): value is WorkspaceModulePresentationChangePage {
  return isRecord(value)
    && isProjectionTarget(value.target)
    && isNonNegativeInteger(value.next)
    && Array.isArray(value.changes)
    && value.changes.every(isWorkspaceModulePresentationChange)
    && (value.gap === null
      || value.gap === undefined
      || (isRecord(value.gap)
        && isNonNegativeInteger(value.gap.earliest_available)
        && isNonNegativeInteger(value.gap.latest_available)
        && isNonNegativeInteger(value.gap.snapshot_revision)));
}

function isWorkspaceModulePresentationChange(
  change: unknown,
): change is WorkspaceModulePresentationChangePage["changes"][number] {
  return isRecord(change)
    && typeof change.change_id === "string"
    && isProjectionTarget(change.target)
    && isNonNegativeInteger(change.sequence)
    && isNonNegativeInteger(change.revision)
    && (change.status === "pending" || change.status === "fulfilled")
    && isPresentationIntent(change.intent)
    && (
      (change.status === "pending" && (change.acknowledgement === null
        || change.acknowledgement === undefined))
      || (change.status === "fulfilled"
        && isRecord(change.acknowledgement)
        && typeof change.acknowledgement.ack_id === "string"
        && isProjectionTarget(change.acknowledgement.target)
        && typeof change.acknowledgement.intent_id === "string"
        && typeof change.acknowledgement.effect_id === "string"
        && isNonNegativeInteger(change.acknowledgement.acknowledged_change_sequence)
        && isNonNegativeInteger(change.acknowledgement.fulfilled_at_ms))
    );
}

export function isAgentRunTerminalSnapshot(value: unknown): value is AgentRunTerminalSnapshot {
  return isRecord(value)
    && isProjectionTarget(value.target)
    && isNonNegativeInteger(value.revision)
    && isNonNegativeInteger(value.latest_change_sequence)
    && isNonNegativeInteger(value.captured_at_ms)
    && Array.isArray(value.terminals)
    && value.terminals.every(isTerminalProjection);
}

export function isAgentRunTerminalChangePage(
  value: unknown,
): value is AgentRunTerminalChangePage {
  return isRecord(value)
    && isProjectionTarget(value.target)
    && isNonNegativeInteger(value.next)
    && Array.isArray(value.changes)
    && value.changes.every((change) =>
      isRecord(change)
      && typeof change.change_id === "string"
      && isProjectionTarget(change.target)
      && isNonNegativeInteger(change.sequence)
      && isNonNegativeInteger(change.revision)
      && isTerminalChangeOrigin(change.origin)
      && typeof change.payload_digest === "string"
      && isTerminalDelta(change.delta)
    )
    && (value.gap === null
      || value.gap === undefined
      || (isRecord(value.gap)
        && isNonNegativeInteger(value.gap.earliest_available)
        && isNonNegativeInteger(value.gap.latest_available)
        && isNonNegativeInteger(value.gap.snapshot_revision)));
}

function changePath(route: string, after?: bigint, limit = 256): string {
  const params = new URLSearchParams({ limit: String(limit) });
  if (after !== undefined) params.set("after", String(after));
  return `${route}?${params.toString()}`;
}

export async function fetchWorkspacePresentationSnapshot(
  target: AgentRunRuntimeTarget,
): Promise<WorkspaceModulePresentationSnapshot> {
  const value = await api.get<unknown>(
    agentRunScopedPath(target, "/workspace-presentations/snapshot"),
  );
  if (!isWorkspaceModulePresentationSnapshot(value)) {
    throw new Error("Workspace presentation snapshot 响应不符合 Product contract");
  }
  return value;
}

export async function fetchWorkspacePresentationChanges(
  target: AgentRunRuntimeTarget,
  after?: bigint,
): Promise<WorkspaceModulePresentationChangePage> {
  const value = await api.get<unknown>(
    agentRunScopedPath(target, changePath("/workspace-presentations/changes", after)),
  );
  if (!isWorkspaceModulePresentationChangePage(value)) {
    throw new Error("Workspace presentation change page 响应不符合 Product contract");
  }
  return value;
}

export async function acknowledgeWorkspacePresentation(
  target: AgentRunRuntimeTarget,
  intentId: string,
  observedChangeSequence: bigint,
): Promise<WorkspaceModulePresentationChangePage["changes"][number]> {
  const value = await api.post<unknown>(
    agentRunScopedPath(
      target,
      `/workspace-presentations/${encodeURIComponent(intentId)}/ack`,
    ),
    { observed_change_sequence: Number(observedChangeSequence) },
  );
  if (!isWorkspaceModulePresentationChange(value)) {
    throw new Error("Workspace presentation acknowledgement 响应不符合 Product contract");
  }
  return value;
}

export async function fetchAgentRunTerminalSnapshot(
  target: AgentRunRuntimeTarget,
): Promise<AgentRunTerminalSnapshot> {
  const value = await api.get<unknown>(
    agentRunScopedPath(target, "/runtime/terminals/snapshot"),
  );
  if (!isAgentRunTerminalSnapshot(value)) {
    throw new Error("AgentRun terminal snapshot 响应不符合 Product contract");
  }
  return value;
}

export async function fetchAgentRunTerminalChanges(
  target: AgentRunRuntimeTarget,
  after?: bigint,
): Promise<AgentRunTerminalChangePage> {
  const value = await api.get<unknown>(
    agentRunScopedPath(target, changePath("/runtime/terminals/changes", after)),
  );
  if (!isAgentRunTerminalChangePage(value)) {
    throw new Error("AgentRun terminal change page 响应不符合 Product contract");
  }
  return value;
}
