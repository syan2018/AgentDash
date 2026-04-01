import type { SessionUpdate } from '@agentclientprotocol/sdk';
import type { AgentDashMetaV1 } from '../../../generated/agentdash-acp-meta';

const EXPECTED_VERSION = 1;

export function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

export function parseAgentDashMeta(meta: unknown): AgentDashMetaV1 | null {
  if (!isRecord(meta)) return null;
  const agentdash = meta.agentdash;
  if (!isRecord(agentdash)) return null;
  const v = agentdash.v;
  if (typeof v !== 'number' || v !== EXPECTED_VERSION) return null;
  return agentdash as unknown as AgentDashMetaV1;
}

export function extractAgentDashMetaFromUpdate(update: SessionUpdate): AgentDashMetaV1 | null {
  const u = update as unknown as Record<string, unknown>;
  // ACP uses `_meta` (preferred), but some SDKs may expose `meta`.
  const direct = u._meta ?? u.meta;
  const fromDirect = parseAgentDashMeta(direct);
  if (fromDirect) return fromDirect;

  // Some updates carry meta nested under `content` (chunk wrappers).
  const content = u.content;
  if (isRecord(content)) {
    return parseAgentDashMeta(content._meta ?? content.meta);
  }
  return null;
}

export function hasAgentDashEvent(update: SessionUpdate): boolean {
  const meta = extractAgentDashMetaFromUpdate(update);
  return Boolean(meta?.event?.type);
}

export interface ToolCallDraftInfo {
  toolCallId?: string;
  toolName?: string;
  phase?: string;
  delta?: string;
  draftInput: string;
  isParseable?: boolean;
}

export function extractToolCallDraftInfo(update: SessionUpdate): ToolCallDraftInfo | null {
  const event = extractAgentDashMetaFromUpdate(update)?.event;
  if (!event || event.type !== 'tool_call_draft' || !isRecord(event.data)) {
    return null;
  }

  const draftInput = event.data.draftInput;
  if (typeof draftInput !== 'string' || draftInput.length === 0) {
    return null;
  }

  return {
    toolCallId: typeof event.data.toolCallId === 'string' ? event.data.toolCallId : undefined,
    toolName: typeof event.data.toolName === 'string' ? event.data.toolName : undefined,
    phase: typeof event.data.phase === 'string' ? event.data.phase : undefined,
    delta: typeof event.data.delta === 'string' ? event.data.delta : undefined,
    draftInput,
    isParseable: typeof event.data.isParseable === 'boolean' ? event.data.isParseable : undefined,
  };
}
