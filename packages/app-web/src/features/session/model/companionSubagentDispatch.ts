import type { AgentDashThreadItem } from "../../../generated/backbone-protocol";
import { agentRunWorkspacePath } from "../../agent/agent-run-paths";

type DynamicItem = Extract<AgentDashThreadItem, { type: "dynamicToolCall" }>;
type CollabItem = Extract<AgentDashThreadItem, { type: "collabAgentToolCall" }>;

export type CompanionSubagentDispatchSource =
  | "companion_request"
  | "collab_spawn_agent";

export type CompanionSubagentDispatchStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "interrupted"
  | "timed_out"
  | "unknown";

export interface CompanionSubagentRawProtocolRefs {
  item_id: string;
  source_tool: string;
  sender_thread_id?: string;
  receiver_thread_ids?: string[];
  dynamic_target?: string;
  details_kind?: string;
  dispatch_id?: string;
  wait_activity_ref?: string;
}

export interface CompanionSubagentDispatchPresentation {
  source: CompanionSubagentDispatchSource;
  title: string;
  childAgentId: string | null;
  status: CompanionSubagentDispatchStatus;
  summary: string | null;
  resultPreview: string | null;
  resultSummary: string | null;
  resultDetails: unknown | null;
  journalUri: string | null;
  waitActivityRef: string | null;
  rawProtocolRefs: CompanionSubagentRawProtocolRefs;
}

export interface CompanionSubagentOpenContext {
  currentRunId?: string | null;
  knownAgentRefs?: readonly CompanionSubagentKnownAgentRef[];
}

export interface CompanionSubagentKnownAgentRef {
  run_id: string;
  agent_id: string;
  display_title?: string | null;
  delivery_status?: string | null;
  last_activity_at?: string | null;
}

export type CompanionSubagentOpenTarget =
  | { enabled: true; path: string }
  | { enabled: false; reason: string };

export function parseCompanionSubagentDispatch(
  item: AgentDashThreadItem,
): CompanionSubagentDispatchPresentation | null {
  if (item.type === "dynamicToolCall") {
    return parseDynamicCompanionRequest(item);
  }
  if (item.type === "collabAgentToolCall") {
    return parseCollabSpawnAgent(item);
  }
  return null;
}

export function isCompanionSubagentDispatchItem(item: AgentDashThreadItem): boolean {
  return parseCompanionSubagentDispatch(item) !== null;
}

export function resolveCompanionSubagentOpenTarget(
  presentation: CompanionSubagentDispatchPresentation,
  context: CompanionSubagentOpenContext,
): CompanionSubagentOpenTarget {
  const childAgentId = presentation.childAgentId;
  if (!childAgentId) {
    return { enabled: false, reason: "等待 child agent id" };
  }

  const currentRunId = normalizeString(context.currentRunId);
  if (currentRunId) {
    return {
      enabled: true,
      path: agentRunWorkspacePath(currentRunId, childAgentId),
    };
  }

  const knownRef = context.knownAgentRefs?.find((ref) => ref.agent_id === childAgentId);
  if (knownRef) {
    return {
      enabled: true,
      path: agentRunWorkspacePath(knownRef.run_id, knownRef.agent_id),
    };
  }

  return { enabled: false, reason: "等待 AgentRun workspace context" };
}

export function resolveCompanionSubagentKnownRef(
  presentation: CompanionSubagentDispatchPresentation,
  refs: readonly CompanionSubagentKnownAgentRef[] | undefined,
): CompanionSubagentKnownAgentRef | null {
  const childAgentId = presentation.childAgentId;
  if (!childAgentId) return null;
  return refs?.find((ref) => ref.agent_id === childAgentId) ?? null;
}

function parseDynamicCompanionRequest(
  item: DynamicItem,
): CompanionSubagentDispatchPresentation | null {
  if (item.tool !== "companion_request") return null;

  const args = isJsonRecord(item.arguments) ? item.arguments : null;
  const details = findCompanionDispatchDetails(item);
  const target = readString(args, "target") ?? readString(details, "target");
  const detailsKind = readString(details, "kind");

  if (target !== "sub" && detailsKind !== "companion_subagent_dispatch") {
    return null;
  }

  const child = readRecord(details, "child");
  const childAgentId =
    readString(child, "agent_id") ??
    readString(details, "agent_id");
  const journal = readRecord(details, "journal");
  const waitActivity = readRecord(details, "wait_activity") ?? readRecord(details, "wait");
  const waitActivityRef =
    readString(waitActivity, "activity_ref") ??
    readFirstString(waitActivity, "activity_refs") ??
    readString(child, "gate_id") ??
    readString(details, "gate_id");
  const payload = readRecord(args, "payload");
  const title =
    readString(details, "companion_label") ??
    readString(payload, "agent_key") ??
    "Companion subagent";
  const status = normalizeDynamicStatus(item, details);
  const resultPreview =
    readString(details, "result_preview") ?? readString(details, "response_preview");
  const resultValue = resultPreview ? parseJsonValue(resultPreview) : null;
  const summary = readString(details, "summary") ?? readString(payload, "message");
  const resultSummary = resolveResultSummary(status, details, resultValue, resultPreview);

  return {
    source: "companion_request",
    title,
    childAgentId,
    status,
    summary: displayTextEquals(summary, resultSummary) ? null : summary,
    resultPreview,
    resultSummary,
    resultDetails: resolveResultDetails(resultValue),
    journalUri: readString(journal, "uri") ?? childAgentJournalUri(childAgentId),
    waitActivityRef,
    rawProtocolRefs: {
      item_id: item.id,
      source_tool: item.tool,
      dynamic_target: target ?? undefined,
      details_kind: detailsKind ?? undefined,
      dispatch_id: readString(details, "dispatch_id") ?? undefined,
      wait_activity_ref: waitActivityRef ?? undefined,
    },
  };
}

function parseCollabSpawnAgent(
  item: CollabItem,
): CompanionSubagentDispatchPresentation | null {
  if (item.tool !== "spawnAgent") return null;

  const childAgentId = item.receiverThreadIds[0] ?? null;
  const childState = childAgentId ? item.agentsStates[childAgentId] : undefined;
  const title = item.prompt?.trim()
    ? firstLine(item.prompt)
    : "Spawned subagent";

  return {
    source: "collab_spawn_agent",
    title,
    childAgentId,
    status: normalizeCollabStatus(item, childState?.status),
    summary: childState?.message ?? item.prompt ?? null,
    resultPreview: null,
    resultSummary: null,
    resultDetails: null,
    journalUri: childAgentId
      ? `lifecycle://agent-runs/${childAgentId}/sessions/messages`
      : null,
    waitActivityRef: null,
    rawProtocolRefs: {
      item_id: item.id,
      source_tool: item.tool,
      sender_thread_id: item.senderThreadId,
      receiver_thread_ids: item.receiverThreadIds,
    },
  };
}

function findCompanionDispatchDetails(item: DynamicItem): Record<string, unknown> | null {
  const candidates: unknown[] = [];
  const args = item.arguments;
  candidates.push(args);

  if (isJsonRecord(args)) {
    candidates.push(args.details);
    candidates.push(args.result);
    candidates.push(args.structuredContent);
  }

  for (const contentItem of item.contentItems ?? []) {
    if (contentItem.type !== "inputText") continue;
    const parsed = parseJsonObject(contentItem.text);
    if (!parsed) continue;
    candidates.push(parsed);
    candidates.push(parsed.details);
    candidates.push(parsed.result);
    candidates.push(parsed.structuredContent);
  }

  for (const candidate of candidates) {
    if (!isJsonRecord(candidate)) continue;
    if (readString(candidate, "kind") === "companion_subagent_dispatch") {
      return candidate;
    }
  }

  for (const candidate of candidates) {
    if (!isJsonRecord(candidate)) continue;
    if (readString(candidate, "target") === "sub") {
      return candidate;
    }
  }

  return null;
}

function normalizeDynamicStatus(
  item: DynamicItem,
  details: Record<string, unknown> | null,
): CompanionSubagentDispatchStatus {
  const detailStatus = readString(details, "status");
  if (detailStatus === "timed_out") return "timed_out";
  if (detailStatus === "running") return "running";
  if (detailStatus === "completed") return "completed";
  if (detailStatus === "failed") return "failed";
  if (detailStatus === "interrupted") return "interrupted";
  if (item.status === "inProgress") return "running";
  if (item.status === "failed" || item.success === false) return "failed";
  if (item.status === "completed") return "completed";
  return "unknown";
}

function normalizeCollabStatus(
  item: CollabItem,
  agentStatus: string | undefined,
): CompanionSubagentDispatchStatus {
  if (agentStatus === "pendingInit") return "pending";
  if (agentStatus === "running") return "running";
  if (agentStatus === "completed" || agentStatus === "shutdown") return "completed";
  if (agentStatus === "errored" || agentStatus === "notFound") return "failed";
  if (agentStatus === "interrupted") return "interrupted";
  if (item.status === "inProgress") return "running";
  if (item.status === "failed") return "failed";
  if (item.status === "completed") return "completed";
  return "unknown";
}

function firstLine(value: string): string {
  const trimmed = value.trim();
  const [line] = trimmed.split(/\r?\n/, 1);
  return line && line.length > 80 ? `${line.slice(0, 79)}...` : line || "Spawned subagent";
}

function parseJsonObject(text: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(text);
    return isJsonRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function parseJsonValue(text: string): unknown | null {
  try {
    const parsed: unknown = JSON.parse(text);
    return parsed;
  } catch {
    return null;
  }
}

function isJsonRecord(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

function readRecord(
  record: Record<string, unknown> | null | undefined,
  key: string,
): Record<string, unknown> | null {
  if (!record) return null;
  const value = record[key];
  return isJsonRecord(value) ? value : null;
}

function readString(
  record: Record<string, unknown> | null | undefined,
  key: string,
): string | null {
  if (!record) return null;
  return normalizeString(record[key]);
}

function readFirstString(
  record: Record<string, unknown> | null | undefined,
  key: string,
): string | null {
  if (!record) return null;
  const value = record[key];
  if (!Array.isArray(value)) return null;
  for (const item of value) {
    const normalized = normalizeString(item);
    if (normalized) return normalized;
  }
  return null;
}

function normalizeString(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0
    ? value.trim()
    : null;
}

function displayTextEquals(left: string | null, right: string | null): boolean {
  if (!left || !right) return false;
  return normalizeDisplayText(left) === normalizeDisplayText(right);
}

function normalizeDisplayText(value: string): string {
  return value.trim().replace(/\s+/g, " ");
}

function childAgentJournalUri(childAgentId: string | null): string | null {
  return childAgentId
    ? `lifecycle://agent-runs/${childAgentId}/sessions/messages`
    : null;
}

function resolveResultSummary(
  status: CompanionSubagentDispatchStatus,
  details: Record<string, unknown> | null,
  resultValue: unknown | null,
  resultPreview: string | null,
): string | null {
  if (status !== "completed" && status !== "failed" && status !== "interrupted") {
    return null;
  }

  const resultRecord = isJsonRecord(resultValue) ? resultValue : null;
  const payload = readRecord(resultRecord, "payload");
  const resultText = normalizeString(resultValue);
  return (
    readString(payload, "summary") ??
    readString(payload, "message") ??
    readString(resultRecord, "summary") ??
    readString(resultRecord, "message") ??
    resultText ??
    readString(details, "summary") ??
    (resultValue == null ? resultPreview : null)
  );
}

function resolveResultDetails(resultValue: unknown | null): unknown | null {
  if (resultValue == null) return null;
  if (normalizeString(resultValue)) return null;
  const withoutSummary = omitResultSummaryFields(resultValue);
  return isEmptyJsonValue(withoutSummary) ? null : withoutSummary;
}

function omitResultSummaryFields(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(omitResultSummaryFields);
  }
  if (!isJsonRecord(value)) {
    return value;
  }

  const entries: Array<[string, unknown]> = [];
  for (const [key, entry] of Object.entries(value)) {
    if (key === "summary" || key === "message") continue;
    if (key === "payload" && isJsonRecord(entry)) {
      const cleanedPayload = omitResultSummaryFields(entry);
      if (!isEmptyJsonValue(cleanedPayload)) {
        entries.push([key, cleanedPayload]);
      }
      continue;
    }
    entries.push([key, omitResultSummaryFields(entry)]);
  }

  return Object.fromEntries(entries);
}

function isEmptyJsonValue(value: unknown): boolean {
  if (Array.isArray(value)) return value.length === 0;
  if (isJsonRecord(value)) return Object.keys(value).length === 0;
  return false;
}
