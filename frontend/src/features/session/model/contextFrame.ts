import { isRecord } from "./platformEvent";

export interface ContextFrame {
  id: string;
  kind: string;
  source: string;
  phase_node?: string;
  apply_mode?: string;
  delivery_status: string;
  delivery_channel: string;
  message_role: string;
  rendered_text: string;
  sections: ContextFrameSection[];
  created_at_ms: number;
}

export type ContextFrameSection =
  | BootstrapContextSection
  | CapabilityDeltaSection
  | ToolSchemaSection
  | ToolSchemaDeltaSection
  | WorkflowContextSection
  | HookInjectionSection
  | SystemNoticeSection
  | WorkspaceSurfaceSection
  | SkillSurfaceSection
  | HookRuntimeSurfaceSection
  | AutoResumeSection;

export interface BootstrapContextSection {
  kind: "bootstrap_context";
  title: string;
  summary: string;
  fragments: RuntimeContextFragmentEntry[];
}

export interface RuntimeContextFragmentEntry {
  slot: string;
  label: string;
  source: string;
  content: string;
}

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

export interface ToolSchemaDeltaSection {
  kind: "tool_schema_delta";
  added_tools: RuntimeToolSchemaEntry[];
  removed_tool_paths: string[];
  restored_tool_paths: string[];
  blocked_tool_paths: string[];
}

export interface RuntimeToolSchemaEntry {
  name: string;
  description: string;
  parameters_schema: unknown;
  capability_key?: string;
  source?: string;
  tool_path?: string;
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

export interface WorkspaceSurfaceSection {
  kind: "workspace_surface";
  title: string;
  summary: string;
  working_directory?: string;
  default_mount?: string;
  mounts: RuntimeWorkspaceMountEntry[];
}

export interface RuntimeWorkspaceMountEntry {
  id: string;
  display_name: string;
  provider: string;
  root_ref: string;
  capabilities: string[];
}

export interface SkillSurfaceSection {
  kind: "skill_surface";
  title: string;
  summary: string;
  read_tool?: string;
  skills: RuntimeSkillEntry[];
}

export interface RuntimeSkillEntry {
  name: string;
  description: string;
  file_path: string;
  disable_model_invocation: boolean;
}

export interface HookRuntimeSurfaceSection {
  kind: "hook_runtime_surface";
  title: string;
  summary: string;
  pending_action_count: number;
}

export interface AutoResumeSection {
  kind: "auto_resume";
  title: string;
  summary: string;
  reason: string;
  prompt: string;
}

export interface RuntimeHookInjectionEntry {
  slot: string;
  source: string;
  content: string;
}

export function parseContextFrame(value: Record<string, unknown>): ContextFrame | null {
  const id = readString(value.id);
  const kind = readString(value.kind);
  const source = readString(value.source);
  const delivery = readString(value.delivery_status);
  const deliveryChannel = readString(value.delivery_channel);
  const messageRole = readString(value.message_role);
  const agentText = readString(value.rendered_text);
  const createdAt = readNumber(value.created_at_ms);
  const rawSections = Array.isArray(value.sections) ? value.sections : [];
  if (!id || !kind || !source || !delivery || !deliveryChannel || !messageRole || !agentText || createdAt == null) return null;

  return {
    id,
    kind,
    source,
    phase_node: readString(value.phase_node) ?? undefined,
    apply_mode: readString(value.apply_mode) ?? undefined,
    delivery_status: delivery,
    delivery_channel: deliveryChannel,
    message_role: messageRole,
    rendered_text: agentText,
    sections: rawSections.map(parseSection).filter((item): item is ContextFrameSection => item != null),
    created_at_ms: createdAt,
  };
}

function parseSection(value: unknown): ContextFrameSection | null {
  if (!isRecord(value)) return null;
  const kind = readString(value.kind);
  if (kind === "bootstrap_context") {
    const fragments = Array.isArray(value.fragments) ? value.fragments : [];
    return {
      kind,
      title: readString(value.title) ?? "Bootstrap Context",
      summary: readString(value.summary) ?? "",
      fragments: fragments.map(parseFragmentEntry).filter((item): item is RuntimeContextFragmentEntry => item != null),
    };
  }
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
  if (kind === "tool_schema_delta") {
    const addedTools = Array.isArray(value.added_tools) ? value.added_tools : [];
    return {
      kind,
      added_tools: addedTools.map(parseToolSchemaEntry).filter((item): item is RuntimeToolSchemaEntry => item != null),
      removed_tool_paths: readStringArray(value.removed_tool_paths),
      restored_tool_paths: readStringArray(value.restored_tool_paths),
      blocked_tool_paths: readStringArray(value.blocked_tool_paths),
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
  if (kind === "workspace_surface") {
    const mounts = Array.isArray(value.mounts) ? value.mounts : [];
    return {
      kind,
      title: readString(value.title) ?? "Workspace Surface",
      summary: readString(value.summary) ?? "",
      working_directory: readString(value.working_directory) ?? undefined,
      default_mount: readString(value.default_mount) ?? undefined,
      mounts: mounts.map(parseWorkspaceMountEntry).filter((item): item is RuntimeWorkspaceMountEntry => item != null),
    };
  }
  if (kind === "skill_surface") {
    const skills = Array.isArray(value.skills) ? value.skills : [];
    return {
      kind,
      title: readString(value.title) ?? "Skill Surface",
      summary: readString(value.summary) ?? "",
      read_tool: readString(value.read_tool) ?? undefined,
      skills: skills.map(parseSkillEntry).filter((item): item is RuntimeSkillEntry => item != null),
    };
  }
  if (kind === "hook_runtime_surface") {
    return {
      kind,
      title: readString(value.title) ?? "Hook Runtime Surface",
      summary: readString(value.summary) ?? "",
      pending_action_count: readNumber(value.pending_action_count) ?? 0,
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
  return null;
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

function parseWorkspaceMountEntry(value: unknown): RuntimeWorkspaceMountEntry | null {
  if (!isRecord(value)) return null;
  const id = readString(value.id);
  if (!id) return null;
  return {
    id,
    display_name: readString(value.display_name) ?? id,
    provider: readString(value.provider) ?? "unknown",
    root_ref: readString(value.root_ref) ?? "",
    capabilities: readStringArray(value.capabilities),
  };
}

function parseSkillEntry(value: unknown): RuntimeSkillEntry | null {
  if (!isRecord(value)) return null;
  const name = readString(value.name);
  if (!name) return null;
  return {
    name,
    description: readString(value.description) ?? "",
    file_path: readString(value.file_path) ?? "",
    disable_model_invocation: value.disable_model_invocation === true,
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
