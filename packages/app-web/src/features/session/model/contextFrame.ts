import { isRecord } from "./platformEvent";
import type { SkillContextExposure } from "../../../types/context";

export interface ContextFrame {
  id: string;
  kind: string;
  source: string;
  phase_node?: string;
  apply_mode?: string;
  delivery_status: string;
  delivery_channel: string;
  message_role: string;
  delivery_metadata: ContextDeliveryMetadata;
  rendered_text: string;
  sections: ContextFrameSection[];
  created_at_ms: number;
}

export type ContextDeliveryPhase =
  | "stable_system"
  | "session_policy"
  | "run_state"
  | "assignment"
  | "discovered_inventory"
  | "turn_runtime";

export type ContextCachePolicy =
  | "static"
  | "session_digest"
  | "runtime_state_digest"
  | "assignment_revision"
  | "discovery_digest"
  | "turn_ephemeral"
  | "uncached";

export type ContextModelChannel =
  | "system"
  | "developer"
  | "context"
  | "user"
  | "audit_only"
  | "ignored";

export type ContextAgentConsumptionMode =
  | "consume"
  | "audit_only"
  | "ignore"
  | "connector_native"
  | "system_override"
  | "system_append";

export interface ContextAgentConsumption {
  target: string;
  mode: ContextAgentConsumptionMode;
  reason: string;
}

export interface ContextConnectorProfile {
  profile_id: string;
  declared_consumption_modes: ContextAgentConsumptionMode[];
}

export interface ContextDeliveryMetadata {
  delivery_phase: ContextDeliveryPhase;
  delivery_order: number;
  cache_policy: ContextCachePolicy;
  cache_key?: string;
  cache_revision?: string;
  model_channel: ContextModelChannel;
  agent_consumption: ContextAgentConsumption;
  frontend_label: string;
  connector_profile: ContextConnectorProfile;
}

export type ContextFrameSection =
  | IdentitySection
  | AssignmentContextSection
  | CapabilityKeyDeltaSection
  | ToolPathDeltaSection
  | McpServerDeltaSection
  | VfsDeltaSection
  | ToolSchemaDeltaSection
  | SkillDeltaSection
  | CompanionAgentRosterDeltaSection
  | SystemNoticeSection
  | PendingActionSection
  | AutoResumeSection
  | CompactionSummarySection
  | UserPreferencesSection
  | ProjectGuidelinesSection
  | UnknownSection;

export interface AssignmentContextSection {
  kind: "assignment_context";
  title: string;
  summary: string;
  fragments: RuntimeContextFragmentEntry[];
}

export interface IdentitySection {
  kind: "identity";
  title: string;
  summary: string;
  base_prompt: string;
  agent_prompt?: string;
  mode: string;
  effective_prompt: string;
}

export interface RuntimeContextFragmentEntry {
  slot: string;
  label: string;
  source: string;
  content: string;
  context_usage_kind?: string;
}

export interface CapabilityKeyDeltaSection {
  kind: "capability_key_delta";
  added_capabilities: string[];
  removed_capabilities: string[];
  effective_capabilities: string[];
}

export interface ToolPathDeltaSection {
  kind: "tool_path_delta";
  blocked_tool_paths: string[];
  unblocked_tool_paths: string[];
  whitelisted_tool_paths: string[];
  removed_whitelist_paths: string[];
}

export interface McpServerDeltaSection {
  kind: "mcp_server_delta";
  added_mcp_servers: string[];
  removed_mcp_servers: string[];
  changed_mcp_servers: string[];
}

export interface VfsDeltaSection {
  kind: "vfs_delta";
  vfs_mounts_added: string[];
  vfs_mounts_removed: string[];
  default_mount_before?: string;
  default_mount_after?: string;
}

export interface ToolSchemaDeltaSection {
  kind: "tool_schema_delta";
  added_tools: RuntimeToolSchemaEntry[];
}

export interface RuntimeToolSchemaEntry {
  name: string;
  description: string;
  parameters_schema: unknown;
  capability_key?: string;
  source?: string;
  tool_path?: string;
  context_usage_kind?: string;
}

export interface SystemNoticeSection {
  kind: "system_notice";
  title: string;
  summary: string;
  body?: string;
}

export interface RuntimeSkillEntry {
  name: string;
  capability_key: string;
  provider_key: string;
  local_name: string;
  display_name?: string;
  description: string;
  file_path: string;
  base_dir?: string;
  exposure: SkillContextExposure;
  disable_model_invocation: boolean;
  context_usage_kind?: string;
}

export interface RuntimeCompanionAgentEntry {
  agent_key: string;
  executor: string;
  display_name: string;
  context_usage_kind?: string;
}

export interface CompanionAgentRosterDeltaSection {
  kind: "companion_agent_roster_delta";
  added_agents: RuntimeCompanionAgentEntry[];
  removed_agent_keys: string[];
  changed_agents: RuntimeCompanionAgentEntry[];
  effective_agents: RuntimeCompanionAgentEntry[];
}

export interface SkillDeltaSection {
  kind: "skill_delta";
  added_skills: RuntimeSkillEntry[];
  removed_skills: RuntimeSkillEntry[];
  changed_skills: RuntimeSkillEntry[];
}

export interface PendingActionSection {
  kind: "pending_action";
  title: string;
  summary: string;
  action_id: string;
  action_type: string;
  status: string;
  revision: number;
  turn_id?: string;
  instructions: string[];
  injections: RuntimeHookInjectionEntry[];
}

export interface AutoResumeSection {
  kind: "auto_resume";
  title: string;
  summary: string;
  reason: string;
  prompt: string;
}

export interface CompactionSummarySection {
  kind: "compaction_summary";
  title: string;
  summary: string;
  tokens_before: number;
  messages_compacted: number;
  compaction_id?: string;
  projection_version?: number;
  strategy?: string;
  trigger?: string;
  phase?: string;
  source_start_event_seq?: number;
  source_end_event_seq?: number;
  first_kept_event_seq?: number;
  compacted_until_ref?: unknown;
  timestamp_ms?: number;
}

export interface UserPreferencesSection {
  kind: "user_preferences";
  title: string;
  summary: string;
  items: string[];
}

export interface ProjectGuidelineEntry {
  path: string;
  content: string;
}

export interface ProjectGuidelinesSection {
  kind: "project_guidelines";
  title: string;
  summary: string;
  entries: ProjectGuidelineEntry[];
}

export interface UnknownSection {
  kind: "unknown_section";
  original_kind: string;
  raw: Record<string, unknown>;
}

export interface RuntimeHookInjectionEntry {
  slot: string;
  source: string;
  content: string;
  context_usage_kind?: string;
}

export function parseContextFrame(value: Record<string, unknown>): ContextFrame | null {
  const id = readString(value.id);
  const kind = readString(value.kind);
  const source = readString(value.source);
  const delivery = readString(value.delivery_status);
  const deliveryChannel = readString(value.delivery_channel);
  const messageRole = readString(value.message_role);
  const agentText = readRenderedText(value.rendered_text);
  const createdAt = readNumber(value.created_at_ms);
  const rawSections = Array.isArray(value.sections) ? value.sections : [];
  if (!id || !kind || !source || !delivery || !deliveryChannel || !messageRole || agentText == null || createdAt == null) return null;

  return {
    id,
    kind,
    source,
    phase_node: readString(value.phase_node) ?? undefined,
    apply_mode: readString(value.apply_mode) ?? undefined,
    delivery_status: delivery,
    delivery_channel: deliveryChannel,
    message_role: messageRole,
    delivery_metadata: parseDeliveryMetadata(
      value.delivery_metadata,
      kind,
      deliveryChannel,
      messageRole,
    ),
    rendered_text: agentText,
    sections: rawSections.map(parseSection).filter((item): item is ContextFrameSection => item != null),
    created_at_ms: createdAt,
  };
}

function parseDeliveryMetadata(
  value: unknown,
  frameKind: string,
  deliveryChannel: string,
  messageRole: string,
): ContextDeliveryMetadata {
  if (!isRecord(value)) {
    return defaultDeliveryMetadata(frameKind, deliveryChannel, messageRole);
  }
  const fallback = defaultDeliveryMetadata(frameKind, deliveryChannel, messageRole);
  const rawConsumption = isRecord(value.agent_consumption) ? value.agent_consumption : {};
  const rawProfile = isRecord(value.connector_profile) ? value.connector_profile : {};
  const declaredModes = Array.isArray(rawProfile.declared_consumption_modes)
    ? rawProfile.declared_consumption_modes
    : [];

  return {
    delivery_phase: readDeliveryPhase(value.delivery_phase) ?? fallback.delivery_phase,
    delivery_order: readNumber(value.delivery_order) ?? fallback.delivery_order,
    cache_policy: readCachePolicy(value.cache_policy) ?? fallback.cache_policy,
    cache_key: readString(value.cache_key) ?? undefined,
    cache_revision: readString(value.cache_revision) ?? undefined,
    model_channel: readModelChannel(value.model_channel) ?? fallback.model_channel,
    agent_consumption: {
      target: readString(rawConsumption.target) ?? fallback.agent_consumption.target,
      mode: readConsumptionMode(rawConsumption.mode) ?? fallback.agent_consumption.mode,
      reason: readString(rawConsumption.reason) ?? fallback.agent_consumption.reason,
    },
    frontend_label: readString(value.frontend_label) ?? fallback.frontend_label,
    connector_profile: {
      profile_id: readString(rawProfile.profile_id) ?? "",
      declared_consumption_modes: declaredModes
        .map(readConsumptionMode)
        .filter((item): item is ContextAgentConsumptionMode => item != null),
    },
  };
}

function defaultDeliveryMetadata(
  frameKind: string,
  _deliveryChannel: string,
  messageRole: string,
): ContextDeliveryMetadata {
  return {
    delivery_phase: defaultDeliveryPhase(frameKind),
    delivery_order: defaultDeliveryOrder(frameKind),
    cache_policy: defaultCachePolicy(frameKind),
    model_channel: defaultModelChannel(frameKind, messageRole),
    agent_consumption: {
      target: "",
      mode: "consume",
      reason: `default_${frameKind}_delivery`,
    },
    frontend_label: defaultFrontendLabel(frameKind),
    connector_profile: {
      profile_id: "",
      declared_consumption_modes: [],
    },
  };
}

function defaultDeliveryPhase(frameKind: string): ContextDeliveryPhase {
  if (frameKind === "identity") return "stable_system";
  if (frameKind === "system_guidelines") return "session_policy";
  if (frameKind === "compaction_summary") return "run_state";
  if (frameKind === "assignment_context") return "assignment";
  if (frameKind === "capability_state_delta" || frameKind === "memory_context") {
    return "discovered_inventory";
  }
  return "turn_runtime";
}

function defaultDeliveryOrder(frameKind: string): number {
  if (frameKind === "identity") return 10;
  if (frameKind === "system_guidelines") return 20;
  if (frameKind === "compaction_summary") return 30;
  if (frameKind === "assignment_context") return 40;
  if (frameKind === "capability_state_delta") return 50;
  if (frameKind === "memory_context") return 60;
  if (frameKind === "pending_action") return 70;
  if (frameKind === "auto_resume") return 80;
  return 100;
}

function defaultCachePolicy(frameKind: string): ContextCachePolicy {
  if (frameKind === "identity") return "static";
  if (frameKind === "system_guidelines") return "session_digest";
  if (frameKind === "compaction_summary") return "runtime_state_digest";
  if (frameKind === "assignment_context") return "assignment_revision";
  if (frameKind === "capability_state_delta" || frameKind === "memory_context") {
    return "discovery_digest";
  }
  if (frameKind === "pending_action" || frameKind === "auto_resume") return "turn_ephemeral";
  return "uncached";
}

function defaultModelChannel(frameKind: string, messageRole: string): ContextModelChannel {
  if (frameKind === "identity" || frameKind === "system_guidelines") return "system";
  if (
    frameKind === "memory_context"
    || frameKind === "compaction_summary"
    || frameKind === "assignment_context"
  ) return "context";
  if (frameKind === "auto_resume" || frameKind === "pending_action") return "user";
  if (messageRole === "system") return "system";
  if (messageRole === "developer") return "developer";
  if (messageRole === "user") return "user";
  return "context";
}

function defaultFrontendLabel(frameKind: string): string {
  if (frameKind === "identity") return "Identity";
  if (frameKind === "system_guidelines") return "System Guidelines";
  if (frameKind === "compaction_summary") return "Compaction Summary";
  if (frameKind === "assignment_context") return "Assignment Context";
  if (frameKind === "capability_state_delta") return "Capability State Delta";
  if (frameKind === "memory_context") return "Memory Context";
  if (frameKind === "pending_action") return "Pending Action";
  if (frameKind === "auto_resume") return "Auto Resume";
  return "Context Frame";
}

function parseSection(value: unknown): ContextFrameSection | null {
  if (!isRecord(value)) return null;
  const kind = readString(value.kind);
  if (kind === "assignment_context") {
    const fragments = Array.isArray(value.fragments) ? value.fragments : [];
    return {
      kind,
      title: readString(value.title) ?? "Assignment Context",
      summary: readString(value.summary) ?? "",
      fragments: fragments.map(parseFragmentEntry).filter((item): item is RuntimeContextFragmentEntry => item != null),
    };
  }
  if (kind === "identity") {
    return {
      kind,
      title: readString(value.title) ?? "Identity",
      summary: readString(value.summary) ?? "",
      base_prompt: readString(value.base_prompt) ?? "",
      agent_prompt: readString(value.agent_prompt) ?? undefined,
      mode: readString(value.mode) ?? "append",
      effective_prompt: readString(value.effective_prompt) ?? "",
    };
  }
  if (kind === "capability_key_delta") {
    return {
      kind,
      added_capabilities: readStringArray(value.added_capabilities),
      removed_capabilities: readStringArray(value.removed_capabilities),
      effective_capabilities: readStringArray(value.effective_capabilities),
    };
  }
  if (kind === "tool_path_delta") {
    return {
      kind,
      blocked_tool_paths: readStringArray(value.blocked_tool_paths),
      unblocked_tool_paths: readStringArray(value.unblocked_tool_paths),
      whitelisted_tool_paths: readStringArray(value.whitelisted_tool_paths),
      removed_whitelist_paths: readStringArray(value.removed_whitelist_paths),
    };
  }
  if (kind === "mcp_server_delta") {
    return {
      kind,
      added_mcp_servers: readStringArray(value.added_mcp_servers),
      removed_mcp_servers: readStringArray(value.removed_mcp_servers),
      changed_mcp_servers: readStringArray(value.changed_mcp_servers),
    };
  }
  if (kind === "vfs_delta") {
    return {
      kind,
      vfs_mounts_added: readStringArray(value.vfs_mounts_added),
      vfs_mounts_removed: readStringArray(value.vfs_mounts_removed),
      default_mount_before: readString(value.default_mount_before) ?? undefined,
      default_mount_after: readString(value.default_mount_after) ?? undefined,
    };
  }
  if (kind === "tool_schema_delta") {
    const addedTools = Array.isArray(value.added_tools) ? value.added_tools : [];
    return {
      kind,
      added_tools: addedTools.map(parseToolSchemaEntry).filter((item): item is RuntimeToolSchemaEntry => item != null),
    };
  }
  if (kind === "skill_delta") {
    const added = Array.isArray(value.added_skills) ? value.added_skills : [];
    const removed = Array.isArray(value.removed_skills) ? value.removed_skills : [];
    const changed = Array.isArray(value.changed_skills) ? value.changed_skills : [];
    return {
      kind,
      added_skills: added.map(parseSkillEntry).filter((item): item is RuntimeSkillEntry => item != null),
      removed_skills: removed.map(parseSkillEntry).filter((item): item is RuntimeSkillEntry => item != null),
      changed_skills: changed.map(parseSkillEntry).filter((item): item is RuntimeSkillEntry => item != null),
    };
  }
  if (kind === "companion_agent_roster_delta") {
    const added = Array.isArray(value.added_agents) ? value.added_agents : [];
    const removed = Array.isArray(value.removed_agent_keys) ? value.removed_agent_keys : [];
    const changed = Array.isArray(value.changed_agents) ? value.changed_agents : [];
    const effective = Array.isArray(value.effective_agents) ? value.effective_agents : [];
    return {
      kind,
      added_agents: added
        .map(parseCompanionAgentEntry)
        .filter((item): item is RuntimeCompanionAgentEntry => item != null),
      removed_agent_keys: removed.map(readString).filter((item): item is string => item != null),
      changed_agents: changed
        .map(parseCompanionAgentEntry)
        .filter((item): item is RuntimeCompanionAgentEntry => item != null),
      effective_agents: effective
        .map(parseCompanionAgentEntry)
        .filter((item): item is RuntimeCompanionAgentEntry => item != null),
    };
  }
  if (kind === "system_notice") {
    return {
      kind,
      title: readString(value.title) ?? "系统通知",
      summary: readString(value.summary) ?? "",
      body: readString(value.body) ?? undefined,
    };
  }
  if (kind === "pending_action") {
    const instructions = Array.isArray(value.instructions) ? value.instructions : [];
    const injections = Array.isArray(value.injections) ? value.injections : [];
    return {
      kind,
      title: readString(value.title) ?? "Pending Action",
      summary: readString(value.summary) ?? "",
      action_id: readString(value.action_id) ?? "",
      action_type: readString(value.action_type) ?? "",
      status: readString(value.status) ?? "pending",
      revision: readNumber(value.revision) ?? 0,
      turn_id: readString(value.turn_id) ?? undefined,
      instructions: instructions.map(readString).filter((item): item is string => item != null),
      injections: injections.map(parseInjectionEntry).filter((item): item is RuntimeHookInjectionEntry => item != null),
    };
  }
  if (kind === "auto_resume") {
    return {
      kind,
      title: readString(value.title) ?? "Auto Resume",
      summary: readString(value.summary) ?? "",
      reason: readString(value.reason) ?? "",
      prompt: readString(value.prompt) ?? "",
    };
  }
  if (kind === "compaction_summary") {
    return {
      kind,
      title: readString(value.title) ?? "Compaction Summary",
      summary: readString(value.summary) ?? "",
      tokens_before: readNumber(value.tokens_before) ?? 0,
      messages_compacted: readNumber(value.messages_compacted) ?? 0,
      compaction_id: readString(value.compaction_id) ?? undefined,
      projection_version: readNumber(value.projection_version) ?? undefined,
      strategy: readString(value.strategy) ?? undefined,
      trigger: readString(value.trigger) ?? undefined,
      phase: readString(value.phase) ?? undefined,
      source_start_event_seq: readNumber(value.source_start_event_seq) ?? undefined,
      source_end_event_seq: readNumber(value.source_end_event_seq) ?? undefined,
      first_kept_event_seq: readNumber(value.first_kept_event_seq) ?? undefined,
      compacted_until_ref: value.compacted_until_ref,
      timestamp_ms: readNumber(value.timestamp_ms) ?? undefined,
    };
  }
  if (kind === "user_preferences") {
    const items = Array.isArray(value.items) ? value.items : [];
    return {
      kind,
      title: readString(value.title) ?? "User Preferences",
      summary: readString(value.summary) ?? "",
      items: items.map(readString).filter((item): item is string => item != null),
    };
  }
  if (kind === "project_guidelines") {
    const entries = Array.isArray(value.entries) ? value.entries : [];
    return {
      kind,
      title: readString(value.title) ?? "Project Guidelines",
      summary: readString(value.summary) ?? "",
      entries: entries
        .map(parseProjectGuidelineEntry)
        .filter((item): item is ProjectGuidelineEntry => item != null),
    };
  }
  return {
    kind: "unknown_section",
    original_kind: kind ?? "unknown",
    raw: value,
  };
}

function parseFragmentEntry(value: unknown): RuntimeContextFragmentEntry | null {
  if (!isRecord(value)) return null;
  const slot = readString(value.slot);
  const content = readString(value.content) ?? "";
  if (!slot && !content) return null;
  return {
    slot: slot ?? "context",
    label: readString(value.label) ?? slot ?? "context",
    source: readString(value.source) ?? "unknown",
    content,
    context_usage_kind: readString(value.context_usage_kind) ?? undefined,
  };
}

function parseToolSchemaEntry(value: unknown): RuntimeToolSchemaEntry | null {
  if (!isRecord(value)) return null;
  const name = readString(value.name);
  const description = readString(value.description) ?? "";
  if (!name) return null;
  return {
    name,
    description,
    parameters_schema: value.parameters_schema,
    capability_key: readString(value.capability_key) ?? undefined,
    source: readString(value.source) ?? undefined,
    tool_path: readString(value.tool_path) ?? undefined,
    context_usage_kind: readString(value.context_usage_kind) ?? undefined,
  };
}

function parseInjectionEntry(value: unknown): RuntimeHookInjectionEntry | null {
  if (!isRecord(value)) return null;
  return {
    slot: readString(value.slot) ?? "context",
    source: readString(value.source) ?? "unknown",
    content: readString(value.content) ?? "",
    context_usage_kind: readString(value.context_usage_kind) ?? undefined,
  };
}

function parseSkillEntry(value: unknown): RuntimeSkillEntry | null {
  if (!isRecord(value)) return null;
  const rawName = readString(value.name);
  const providerKey = readString(value.provider_key) ?? "";
  const localName = readString(value.local_name) ?? rawName ?? "";
  const displayName = readString(value.display_name) ?? undefined;
  const capabilityKey =
    readString(value.capability_key)
    ?? (providerKey && localName ? `${providerKey}/${localName}` : rawName ?? localName);
  const name = rawName ?? displayName ?? localName ?? capabilityKey;
  if (!name) return null;
  return {
    name,
    capability_key: capabilityKey,
    provider_key: providerKey,
    local_name: localName || name,
    display_name: displayName,
    description: readString(value.description) ?? "",
    file_path: readString(value.file_path) ?? "",
    base_dir: readString(value.base_dir) ?? undefined,
    exposure: readSkillExposure(value.exposure),
    disable_model_invocation: value.disable_model_invocation === true,
    context_usage_kind: readString(value.context_usage_kind) ?? undefined,
  };
}

function parseCompanionAgentEntry(value: unknown): RuntimeCompanionAgentEntry | null {
  if (!isRecord(value)) return null;
  const agentKey = readString(value.agent_key);
  if (!agentKey) return null;
  return {
    agent_key: agentKey,
    executor: readString(value.executor) ?? "",
    display_name: readString(value.display_name) ?? agentKey,
    context_usage_kind: readString(value.context_usage_kind) ?? undefined,
  };
}

function parseProjectGuidelineEntry(value: unknown): ProjectGuidelineEntry | null {
  if (!isRecord(value)) return null;
  const path = readString(value.path);
  const content = readRenderedText(value.content);
  if (!path || content == null) return null;
  return { path, content };
}

function readSkillExposure(value: unknown): SkillContextExposure {
  if (value === "explicit_only") return "explicit_only";
  return "default_exposed";
}

function readString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function readDeliveryPhase(value: unknown): ContextDeliveryPhase | null {
  if (
    value === "stable_system"
    || value === "session_policy"
    || value === "run_state"
    || value === "assignment"
    || value === "discovered_inventory"
    || value === "turn_runtime"
  ) return value;
  return null;
}

function readCachePolicy(value: unknown): ContextCachePolicy | null {
  if (
    value === "static"
    || value === "session_digest"
    || value === "runtime_state_digest"
    || value === "assignment_revision"
    || value === "discovery_digest"
    || value === "turn_ephemeral"
    || value === "uncached"
  ) return value;
  return null;
}

function readModelChannel(value: unknown): ContextModelChannel | null {
  if (
    value === "system"
    || value === "developer"
    || value === "context"
    || value === "user"
    || value === "audit_only"
    || value === "ignored"
  ) return value;
  return null;
}

function readConsumptionMode(value: unknown): ContextAgentConsumptionMode | null {
  if (
    value === "consume"
    || value === "audit_only"
    || value === "ignore"
    || value === "connector_native"
    || value === "system_override"
    || value === "system_append"
  ) return value;
  return null;
}

function readRenderedText(value: unknown): string | null {
  if (typeof value !== "string") return null;
  return value;
}

function readNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function readStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.map(readString).filter((item): item is string => item != null);
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
    case "capability_state_snapshot":
      return { token: "CAPS", variant: "primary" };
    case "capability_state_delta":
      return { token: "CAP", variant: "neutral" };
    case "assignment_context":
      return { token: "ASN", variant: "primary" };
    case "pending_action":
      return { token: "ACT", variant: "warning" };
    case "auto_resume":
      return { token: "RSM", variant: "warning" };
    case "compaction_summary":
      return { token: "CMP", variant: "warning" };
    case "system_guidelines":
      return { token: "GUID", variant: "primary" };
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
    case "identity":
      return { token: "IDN", variant: "primary" };
    case "assignment_context":
      return { token: "ASN", variant: "primary" };
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
    case "companion_agent_roster_delta":
      return { token: "AGNT", variant: "primary" };
    case "system_notice":
      return { token: "SYS", variant: "neutral" };
    case "pending_action":
      return { token: "ACT", variant: "warning" };
    case "auto_resume":
      return { token: "RSM", variant: "warning" };
    case "compaction_summary":
      return { token: "CMP", variant: "warning" };
    case "user_preferences":
      return { token: "PREF", variant: "primary" };
    case "project_guidelines":
      return { token: "GUID", variant: "primary" };
    case "unknown_section":
      return { token: "UNK", variant: "warning" };
  }
}
