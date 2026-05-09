import { isRecord } from "./platformEvent";

export interface RuntimeContextNotice {
  id: string;
  source: string;
  phase_node?: string;
  apply_mode?: string;
  delivery_status: string;
  agent_visible_text: string;
  sections: RuntimeContextNoticeSection[];
  created_at_ms: number;
}

export type RuntimeContextNoticeSection =
  | CapabilityDeltaSection
  | ToolSchemaSection
  | WorkflowContextSection
  | HookInjectionSection
  | SystemNoticeSection;

export interface CapabilityDeltaSection {
  kind: "capability_delta";
  added_capabilities: string[];
  removed_capabilities: string[];
  effective_capabilities: string[];
  blocked_tool_paths: string[];
  unblocked_tool_paths: string[];
  whitelisted_tool_paths: string[];
  removed_whitelist_paths: string[];
  added_mcp_servers: string[];
  removed_mcp_servers: string[];
  changed_mcp_servers: string[];
  vfs_mounts_added: string[];
  vfs_mounts_removed: string[];
  default_mount_before?: string;
  default_mount_after?: string;
}

export interface ToolSchemaSection {
  kind: "tool_schema";
  tools: RuntimeToolSchemaEntry[];
}

export interface RuntimeToolSchemaEntry {
  name: string;
  description: string;
  parameters_schema: unknown;
  capability_key?: string;
  source?: string;
}

export interface WorkflowContextSection {
  kind: "workflow_context";
  title: string;
  summary: string;
  injections: RuntimeHookInjectionEntry[];
}

export interface HookInjectionSection {
  kind: "hook_injection";
  title: string;
  summary: string;
  injections: RuntimeHookInjectionEntry[];
}

export interface SystemNoticeSection {
  kind: "system_notice";
  title: string;
  summary: string;
  body?: string;
}

export interface RuntimeHookInjectionEntry {
  slot: string;
  source: string;
  content: string;
}

export function parseRuntimeContextNotice(value: Record<string, unknown>): RuntimeContextNotice | null {
  const id = readString(value.id);
  const source = readString(value.source);
  const delivery = readString(value.delivery_status);
  const agentText = readString(value.agent_visible_text);
  const createdAt = readNumber(value.created_at_ms);
  const rawSections = Array.isArray(value.sections) ? value.sections : [];
  if (!id || !source || !delivery || !agentText || createdAt == null) return null;

  return {
    id,
    source,
    phase_node: readString(value.phase_node) ?? undefined,
    apply_mode: readString(value.apply_mode) ?? undefined,
    delivery_status: delivery,
    agent_visible_text: agentText,
    sections: rawSections.map(parseSection).filter((item): item is RuntimeContextNoticeSection => item != null),
    created_at_ms: createdAt,
  };
}

function parseSection(value: unknown): RuntimeContextNoticeSection | null {
  if (!isRecord(value)) return null;
  const kind = readString(value.kind);
  if (kind === "capability_delta") {
    return {
      kind,
      added_capabilities: readStringArray(value.added_capabilities),
      removed_capabilities: readStringArray(value.removed_capabilities),
      effective_capabilities: readStringArray(value.effective_capabilities),
      blocked_tool_paths: readStringArray(value.blocked_tool_paths),
      unblocked_tool_paths: readStringArray(value.unblocked_tool_paths),
      whitelisted_tool_paths: readStringArray(value.whitelisted_tool_paths),
      removed_whitelist_paths: readStringArray(value.removed_whitelist_paths),
      added_mcp_servers: readStringArray(value.added_mcp_servers),
      removed_mcp_servers: readStringArray(value.removed_mcp_servers),
      changed_mcp_servers: readStringArray(value.changed_mcp_servers),
      vfs_mounts_added: readStringArray(value.vfs_mounts_added),
      vfs_mounts_removed: readStringArray(value.vfs_mounts_removed),
      default_mount_before: readString(value.default_mount_before) ?? undefined,
      default_mount_after: readString(value.default_mount_after) ?? undefined,
    };
  }
  if (kind === "tool_schema") {
    const tools = Array.isArray(value.tools) ? value.tools : [];
    return {
      kind,
      tools: tools.map(parseToolSchemaEntry).filter((item): item is RuntimeToolSchemaEntry => item != null),
    };
  }
  if (kind === "workflow_context" || kind === "hook_injection") {
    const title = readString(value.title) ?? (kind === "workflow_context" ? "Workflow Context" : "Hook Injection");
    const summary = readString(value.summary) ?? "";
    const injections = Array.isArray(value.injections) ? value.injections : [];
    return {
      kind,
      title,
      summary,
      injections: injections.map(parseInjectionEntry).filter((item): item is RuntimeHookInjectionEntry => item != null),
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
  return null;
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
  };
}

function parseInjectionEntry(value: unknown): RuntimeHookInjectionEntry | null {
  if (!isRecord(value)) return null;
  return {
    slot: readString(value.slot) ?? "context",
    source: readString(value.source) ?? "unknown",
    content: readString(value.content) ?? "",
  };
}

function readString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function readNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function readStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.map(readString).filter((item): item is string => item != null);
}
