import { isRecord } from "./platformEvent";
import type {
  ContextFrame as GeneratedContextFrame,
  ContextFrameSection as GeneratedContextFrameSection,
  RuntimeCompanionAgentEntry,
  RuntimeContextFragmentEntry,
  RuntimeMemoryDiagnosticEntry,
  RuntimeMemoryInventoryMode,
  RuntimeMemorySourceEntry,
  RuntimeSkillEntry,
  RuntimeToolSchemaEntry,
  SkillContextExposure,
} from "../../../generated/backbone-protocol";
import type { JsonValue } from "../../../generated/common-contracts";

export type ContextFrame = GeneratedContextFrame;
export type ContextFrameKind = GeneratedContextFrame["kind"];
export type ContextDeliveryStatus = GeneratedContextFrame["delivery_status"];
export type ContextFrameSection = GeneratedContextFrameSection;
export type ContextFragmentsSection = Extract<
  ContextFrameSection,
  { kind: "context_fragments" }
>;
export type CapabilityKeyDeltaSection = Extract<
  ContextFrameSection,
  { kind: "capability_key_delta" }
>;
export type ToolPathDeltaSection = Extract<
  ContextFrameSection,
  { kind: "tool_path_delta" }
>;
export type McpServerDeltaSection = Extract<
  ContextFrameSection,
  { kind: "mcp_server_delta" }
>;
export type VfsDeltaSection = Extract<ContextFrameSection, { kind: "vfs_delta" }>;
export type ToolSchemaDeltaSection = Extract<
  ContextFrameSection,
  { kind: "tool_schema_delta" }
>;
export type SkillDeltaSection = Extract<ContextFrameSection, { kind: "skill_delta" }>;
export type MemoryInventorySection = Extract<
  ContextFrameSection,
  { kind: "memory_inventory" }
>;
export type CompanionAgentRosterDeltaSection = Extract<
  ContextFrameSection,
  { kind: "companion_agent_roster_delta" }
>;
export type SystemNoticeSection = Extract<
  ContextFrameSection,
  { kind: "system_notice" }
>;
export type CompactionSummarySection = Extract<
  ContextFrameSection,
  { kind: "compaction_summary" }
>;
export type {
  RuntimeCompanionAgentEntry,
  RuntimeContextFragmentEntry,
  RuntimeMemoryDiagnosticEntry,
  RuntimeMemoryInventoryMode,
  RuntimeMemorySourceEntry,
  RuntimeSkillEntry,
  RuntimeToolSchemaEntry,
  SkillContextExposure,
};

export function parseContextFrame(input: unknown): ContextFrame | null {
  if (!isRecord(input)) return null;
  const id = readString(input.id);
  const kind = readFrameKind(input.kind);
  const deliveryStatus = readDeliveryStatus(input.delivery_status);
  const renderedText = readString(input.rendered_text);
  const createdAt = readInteger(input.created_at_ms);
  const sections = parseArray(input.sections, parseSection);
  if (
    id == null
    || kind == null
    || deliveryStatus == null
    || renderedText == null
    || createdAt == null
    || sections == null
  ) {
    return null;
  }
  return {
    id,
    kind,
    delivery_status: deliveryStatus,
    rendered_text: renderedText,
    sections,
    created_at_ms: createdAt,
  };
}

function readFrameKind(value: unknown): ContextFrameKind | null {
  if (
    value === "identity"
    || value === "user_context"
    || value === "environment"
    || value === "system_guidelines"
    || value === "assignment_context"
    || value === "capability_state_delta"
    || value === "memory_context"
    || value === "compaction_summary"
  ) return value;
  return null;
}

function readDeliveryStatus(value: unknown): ContextDeliveryStatus | null {
  if (value === "applied_before_prompt" || value === "applied_to_compacted_context") {
    return value;
  }
  return null;
}

function parseSection(value: unknown): ContextFrameSection | null {
  if (!isRecord(value)) return null;
  const kind = readString(value.kind);
  if (kind === "context_fragments") {
    const fragments = parseArray(value.fragments, parseFragmentEntry);
    return fragments == null ? null : { kind, fragments };
  }
  if (kind === "capability_key_delta") {
    const addedCapabilities = readStringArray(value.added_capabilities);
    const removedCapabilities = readStringArray(value.removed_capabilities);
    const effectiveCapabilities = readStringArray(value.effective_capabilities);
    if (
      addedCapabilities == null
      || removedCapabilities == null
      || effectiveCapabilities == null
    ) return null;
    return {
      kind,
      added_capabilities: addedCapabilities,
      removed_capabilities: removedCapabilities,
      effective_capabilities: effectiveCapabilities,
    };
  }
  if (kind === "tool_path_delta") {
    const blockedToolPaths = readStringArray(value.blocked_tool_paths);
    const unblockedToolPaths = readStringArray(value.unblocked_tool_paths);
    const whitelistedToolPaths = readStringArray(value.whitelisted_tool_paths);
    const removedWhitelistPaths = readStringArray(value.removed_whitelist_paths);
    if (
      blockedToolPaths == null
      || unblockedToolPaths == null
      || whitelistedToolPaths == null
      || removedWhitelistPaths == null
    ) return null;
    return {
      kind,
      blocked_tool_paths: blockedToolPaths,
      unblocked_tool_paths: unblockedToolPaths,
      whitelisted_tool_paths: whitelistedToolPaths,
      removed_whitelist_paths: removedWhitelistPaths,
    };
  }
  if (kind === "mcp_server_delta") {
    const addedMcpServers = readStringArray(value.added_mcp_servers);
    const removedMcpServers = readStringArray(value.removed_mcp_servers);
    const changedMcpServers = readStringArray(value.changed_mcp_servers);
    if (
      addedMcpServers == null
      || removedMcpServers == null
      || changedMcpServers == null
    ) return null;
    return {
      kind,
      added_mcp_servers: addedMcpServers,
      removed_mcp_servers: removedMcpServers,
      changed_mcp_servers: changedMcpServers,
    };
  }
  if (kind === "vfs_delta") {
    const vfsMountsAdded = readStringArray(value.vfs_mounts_added);
    const vfsMountsRemoved = readStringArray(value.vfs_mounts_removed);
    const defaultMountBefore = readOptionalString(value.default_mount_before);
    const defaultMountAfter = readOptionalString(value.default_mount_after);
    if (
      vfsMountsAdded == null
      || vfsMountsRemoved == null
      || defaultMountBefore === INVALID
      || defaultMountAfter === INVALID
    ) return null;
    return {
      kind,
      vfs_mounts_added: vfsMountsAdded,
      vfs_mounts_removed: vfsMountsRemoved,
      default_mount_before: defaultMountBefore,
      default_mount_after: defaultMountAfter,
    };
  }
  if (kind === "tool_schema_delta") {
    const addedTools = parseArray(value.added_tools, parseToolSchemaEntry);
    const removedTools = readStringArray(value.removed_tools);
    const changedTools = parseArray(value.changed_tools, parseToolSchemaEntry);
    if (addedTools == null || removedTools == null || changedTools == null) return null;
    return {
      kind,
      added_tools: addedTools,
      removed_tools: removedTools,
      changed_tools: changedTools,
    };
  }
  if (kind === "skill_delta") {
    const added = parseArray(value.added_skills, parseSkillEntry);
    const removed = parseArray(value.removed_skills, parseSkillEntry);
    const changed = parseArray(value.changed_skills, parseSkillEntry);
    if (added == null || removed == null || changed == null) return null;
    return {
      kind,
      added_skills: added,
      removed_skills: removed,
      changed_skills: changed,
    };
  }
  if (kind === "memory_inventory") {
    const title = readString(value.title);
    const summary = readString(value.summary);
    const mode = readMemoryInventoryMode(value.mode);
    const sources = parseArray(value.sources, parseMemorySourceEntry);
    const diagnostics = parseArray(value.diagnostics, parseMemoryDiagnosticEntry);
    const added = parseArray(value.added_sources, parseMemorySourceEntry);
    const removed = parseArray(value.removed_sources, parseMemorySourceEntry);
    const changed = parseArray(value.changed_sources, parseMemorySourceEntry);
    if (
      title == null
      || summary == null
      || mode == null
      || sources == null
      || diagnostics == null
      || added == null
      || removed == null
      || changed == null
    ) return null;
    return {
      kind,
      title,
      summary,
      mode,
      sources,
      diagnostics,
      added_sources: added,
      removed_sources: removed,
      changed_sources: changed,
    };
  }
  if (kind === "companion_agent_roster_delta") {
    const added = parseArray(value.added_agents, parseCompanionAgentEntry);
    const removed = readStringArray(value.removed_agent_keys);
    const changed = parseArray(value.changed_agents, parseCompanionAgentEntry);
    const effective = parseArray(value.effective_agents, parseCompanionAgentEntry);
    if (added == null || removed == null || changed == null || effective == null) return null;
    return {
      kind,
      added_agents: added,
      removed_agent_keys: removed,
      changed_agents: changed,
      effective_agents: effective,
    };
  }
  if (kind === "system_notice") {
    const title = readString(value.title);
    const summary = readString(value.summary);
    const body = readOptionalString(value.body);
    if (title == null || summary == null || body === INVALID) return null;
    return {
      kind,
      title,
      summary,
      body,
    };
  }
  if (kind === "compaction_summary") {
    const title = readString(value.title);
    const summary = readString(value.summary);
    const tokensBefore = readUnsignedInteger(value.tokens_before);
    const messagesCompacted = readUnsignedInteger(value.messages_compacted);
    const compactionId = readOptionalString(value.compaction_id);
    const projectionVersion = readOptionalUnsignedInteger(value.projection_version);
    const strategy = readOptionalString(value.strategy);
    const trigger = readOptionalString(value.trigger);
    const phase = readOptionalString(value.phase);
    const sourceStartEventSeq = readOptionalUnsignedInteger(value.source_start_event_seq);
    const sourceEndEventSeq = readOptionalUnsignedInteger(value.source_end_event_seq);
    const firstKeptEventSeq = readOptionalUnsignedInteger(value.first_kept_event_seq);
    const compactedUntilRef = readOptionalJson(value.compacted_until_ref);
    const timestampMs = readOptionalUnsignedInteger(value.timestamp_ms);
    if (
      title == null
      || summary == null
      || tokensBefore == null
      || messagesCompacted == null
      || compactionId === INVALID
      || projectionVersion === INVALID
      || strategy === INVALID
      || trigger === INVALID
      || phase === INVALID
      || sourceStartEventSeq === INVALID
      || sourceEndEventSeq === INVALID
      || firstKeptEventSeq === INVALID
      || compactedUntilRef === INVALID
      || timestampMs === INVALID
    ) return null;
    return {
      kind,
      title,
      summary,
      tokens_before: tokensBefore,
      messages_compacted: messagesCompacted,
      compaction_id: compactionId,
      projection_version: projectionVersion,
      strategy,
      trigger,
      phase,
      source_start_event_seq: sourceStartEventSeq,
      source_end_event_seq: sourceEndEventSeq,
      first_kept_event_seq: firstKeptEventSeq,
      compacted_until_ref: compactedUntilRef,
      timestamp_ms: timestampMs,
    };
  }
  return null;
}

function parseFragmentEntry(value: unknown): RuntimeContextFragmentEntry | null {
  if (!isRecord(value)) return null;
  const slot = readString(value.slot);
  const label = readString(value.label);
  const source = readString(value.source);
  const content = readString(value.content);
  const contextUsageKind = readOptionalString(value.context_usage_kind);
  if (
    slot == null
    || label == null
    || source == null
    || content == null
    || contextUsageKind === INVALID
  ) return null;
  return {
    slot,
    label,
    source,
    content,
    context_usage_kind: contextUsageKind,
  };
}

function parseToolSchemaEntry(value: unknown): RuntimeToolSchemaEntry | null {
  if (!isRecord(value)) return null;
  const name = readString(value.name);
  const description = readString(value.description);
  const parametersSchema = readRequiredJson(value.parameters_schema);
  const capabilityKey = readOptionalString(value.capability_key);
  const source = readOptionalString(value.source);
  const toolPath = readOptionalString(value.tool_path);
  const contextUsageKind = readOptionalString(value.context_usage_kind);
  if (
    name == null
    || description == null
    || parametersSchema === INVALID
    || capabilityKey === INVALID
    || source === INVALID
    || toolPath === INVALID
    || contextUsageKind === INVALID
  ) return null;
  return {
    name,
    description,
    parameters_schema: parametersSchema,
    capability_key: capabilityKey,
    source,
    tool_path: toolPath,
    context_usage_kind: contextUsageKind,
  };
}

function parseSkillEntry(value: unknown): RuntimeSkillEntry | null {
  if (!isRecord(value)) return null;
  const name = readString(value.name);
  const capabilityKey = readString(value.capability_key);
  const providerKey = readString(value.provider_key);
  const localName = readString(value.local_name);
  const displayName = readOptionalString(value.display_name);
  const description = readString(value.description);
  const filePath = readString(value.file_path);
  const baseDir = readOptionalString(value.base_dir);
  const exposure = readSkillExposure(value.exposure);
  const disableModelInvocation = readBoolean(value.disable_model_invocation);
  const contextUsageKind = readOptionalString(value.context_usage_kind);
  if (
    name == null
    || capabilityKey == null
    || providerKey == null
    || localName == null
    || displayName === INVALID
    || description == null
    || filePath == null
    || baseDir === INVALID
    || exposure == null
    || disableModelInvocation == null
    || contextUsageKind === INVALID
  ) return null;
  return {
    name,
    capability_key: capabilityKey,
    provider_key: providerKey,
    local_name: localName,
    display_name: displayName,
    description,
    file_path: filePath,
    base_dir: baseDir,
    exposure,
    disable_model_invocation: disableModelInvocation,
    context_usage_kind: contextUsageKind,
  };
}

function parseCompanionAgentEntry(value: unknown): RuntimeCompanionAgentEntry | null {
  if (!isRecord(value)) return null;
  const agentKey = readString(value.agent_key);
  const executor = readString(value.executor);
  const displayName = readString(value.display_name);
  const contextUsageKind = readOptionalString(value.context_usage_kind);
  if (
    agentKey == null
    || executor == null
    || displayName == null
    || contextUsageKind === INVALID
  ) return null;
  return {
    agent_key: agentKey,
    executor,
    display_name: displayName,
    context_usage_kind: contextUsageKind,
  };
}

function parseMemorySourceEntry(value: unknown): RuntimeMemorySourceEntry | null {
  if (!isRecord(value)) return null;
  const providerKey = readString(value.provider_key);
  const sourceKey = readString(value.source_key);
  const displayName = readString(value.display_name);
  const sourceUri = readString(value.source_uri);
  const indexUri = readString(value.index_uri);
  const mountId = readString(value.mount_id);
  const scope = readString(value.scope);
  const indexStatus = readString(value.index_status);
  const trustLevel = readString(value.trust_level);
  const revision = readString(value.revision);
  const summary = readOptionalString(value.summary);
  const contextUsageKind = readOptionalString(value.context_usage_kind);
  if (
    providerKey == null
    || sourceKey == null
    || displayName == null
    || sourceUri == null
    || indexUri == null
    || mountId == null
    || scope == null
    || indexStatus == null
    || trustLevel == null
    || revision == null
    || summary === INVALID
    || contextUsageKind === INVALID
  ) return null;
  return {
    provider_key: providerKey,
    source_key: sourceKey,
    display_name: displayName,
    source_uri: sourceUri,
    index_uri: indexUri,
    mount_id: mountId,
    scope,
    index_status: indexStatus,
    trust_level: trustLevel,
    revision,
    summary,
    context_usage_kind: contextUsageKind,
  };
}

function parseMemoryDiagnosticEntry(value: unknown): RuntimeMemoryDiagnosticEntry | null {
  if (!isRecord(value)) return null;
  const providerKey = readString(value.provider_key);
  const code = readString(value.code);
  const message = readString(value.message);
  const sourceKey = readOptionalString(value.source_key);
  const uri = readOptionalString(value.uri);
  const contextUsageKind = readOptionalString(value.context_usage_kind);
  if (
    providerKey == null
    || code == null
    || message == null
    || sourceKey === INVALID
    || uri === INVALID
    || contextUsageKind === INVALID
  ) return null;
  return {
    provider_key: providerKey,
    code,
    message,
    source_key: sourceKey,
    uri,
    context_usage_kind: contextUsageKind,
  };
}

function readSkillExposure(value: unknown): SkillContextExposure | null {
  if (value === "explicit_only" || value === "default_exposed") return value;
  return null;
}

function readMemoryInventoryMode(value: unknown): RuntimeMemoryInventoryMode | null {
  if (value === "snapshot" || value === "delta") return value;
  return null;
}

function readString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function readBoolean(value: unknown): boolean | null {
  return typeof value === "boolean" ? value : null;
}

function readInteger(value: unknown): number | null {
  return typeof value === "number" && Number.isSafeInteger(value) ? value : null;
}

function readUnsignedInteger(value: unknown): number | null {
  const parsed = readInteger(value);
  return parsed != null && parsed >= 0 ? parsed : null;
}

function readStringArray(value: unknown): string[] | null {
  return parseArray(value, readString);
}

function parseArray<T>(
  value: unknown,
  parseItem: (item: unknown) => T | null,
): T[] | null {
  if (!Array.isArray(value)) return null;
  const parsed: T[] = [];
  for (const item of value) {
    const next = parseItem(item);
    if (next == null) return null;
    parsed.push(next);
  }
  return parsed;
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value === null) return true;
  if (
    typeof value === "string"
    || typeof value === "boolean"
    || (typeof value === "number" && Number.isFinite(value))
  ) return true;
  if (Array.isArray(value)) return value.every(isJsonValue);
  return isRecord(value) && Object.values(value).every(isJsonValue);
}

const INVALID = Symbol("invalid_optional_context_frame_field");
type OptionalParsed<T> = T | null | undefined | typeof INVALID;

function readRequiredJson(value: unknown): JsonValue | typeof INVALID {
  return isJsonValue(value) ? value : INVALID;
}

function readOptionalString(value: unknown): OptionalParsed<string> {
  if (value === undefined || value === null) return value;
  return typeof value === "string" ? value : INVALID;
}

function readOptionalUnsignedInteger(value: unknown): OptionalParsed<number> {
  if (value === undefined || value === null) return value;
  return readUnsignedInteger(value) ?? INVALID;
}

function readOptionalJson(value: unknown): OptionalParsed<JsonValue> {
  if (value === undefined || value === null) return value;
  return isJsonValue(value) ? value : INVALID;
}

// ──────────────────────────────────────────────────────────────────────────────
// Token / Variant 映射纯函数
//
// frame.kind → token：用于外层 frame tab 条上的徽标
// section.kind → token：用于内层 section header 行的徽标
//
// 颜色 token 仅限项目既有 BADGE 五色中性集，保持 EventCards 的 "badge 是
// 唯一染色点" 约束。
// ──────────────────────────────────────────────────────────────────────────────

export type ContextBadgeVariant = "neutral" | "primary" | "warning";

export interface ContextTokenInfo {
  token: string;
  variant: ContextBadgeVariant;
}

/** 由 frame.kind 推导外层 tab 上的 token 与徽标颜色 */
export function frameKindToToken(kind: string): ContextTokenInfo {
  switch (kind) {
    case "identity":
      return { token: "IDN", variant: "primary" };
    case "capability_state_delta":
      return { token: "CAP", variant: "neutral" };
    case "assignment_context":
      return { token: "ASN", variant: "primary" };
    case "compaction_summary":
      return { token: "CMP", variant: "warning" };
    case "system_guidelines":
      return { token: "GUID", variant: "primary" };
    case "memory_context":
      return { token: "MEM", variant: "primary" };
    case "user_context":
      return { token: "USR", variant: "primary" };
    case "environment":
      return { token: "ENV", variant: "neutral" };
    default:
      return {
        token: (kind.replace(/[^a-zA-Z0-9]/g, "").slice(0, 4) || "CTX").toUpperCase(),
        variant: "neutral",
      };
  }
}

/** 由 section.kind 推导内层 section 行 token 与徽标颜色 */
export function sectionKindToToken(kind: ContextFrameSection["kind"]): ContextTokenInfo {
  switch (kind) {
    case "context_fragments":
      return { token: "CTX", variant: "primary" };
    case "capability_key_delta":
      return { token: "CAP", variant: "neutral" };
    case "tool_path_delta":
      return { token: "PATH", variant: "neutral" };
    case "mcp_server_delta":
      return { token: "MCP", variant: "neutral" };
    case "vfs_delta":
      return { token: "VFS", variant: "neutral" };
    case "tool_schema_delta":
      return { token: "TOOL", variant: "neutral" };
    case "skill_delta":
      return { token: "SKL", variant: "neutral" };
    case "memory_inventory":
      return { token: "MEM", variant: "primary" };
    case "companion_agent_roster_delta":
      return { token: "AGNT", variant: "primary" };
    case "system_notice":
      return { token: "SYS", variant: "neutral" };
    case "compaction_summary":
      return { token: "CMP", variant: "warning" };
  }
}
