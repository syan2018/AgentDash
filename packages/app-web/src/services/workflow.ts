import { api } from "../api/client";
import { asRecord, asRecordArray, asStringArray, optStringField, requireStringField } from "../api/mappers";
import { isWorkflowJsonValue } from "../types";
import { mapInstalledAssetSource } from "./sharedLibrary";
import type {
  ActivityCompletionPolicy,
  ActivityDefinition,
  ActivityExecutorSpec,
  ActivityJoinPolicy,
  WorkflowGraph,
  ActivityTransition,
  ArtifactAliasPolicy,
  ArtifactBinding,
  CapabilityDirective,
  HookRulePreset,
  ContextStrategy,
  GateStrategy,
  InputPortDefinition,
  JsonValue,
  OutputPortDefinition,
  StandaloneFulfillment,
  LifecycleExecutionEntry,
  LifecycleExecutionEventKind,
  ToolDescriptor,
  TransitionCondition,
  WorkflowContextBinding,
  AgentProcedureContract,
  AgentProcedure,
  DefinitionSource,
  WorkflowCapabilityConfig,
  WorkflowHookRuleSpec,
  WorkflowHookTrigger,
  WorkflowInjectionSpec,
  WorkflowRun,
  WorkflowRunStatus,
  WorkflowTargetKind,
  WorkflowValidationResult,
} from "../types";

/** 将 unknown 转为 string | undefined，适配生成合约中的可选字段（`?:` 而非 `| null`） */
function optUndef(raw: unknown): string | undefined {
  return raw != null ? String(raw) : undefined;
}

// ─── 枚举 normalizer（将后端字符串收窄到前端联合类型）───

const WORKFLOW_TARGET_KINDS = new Set<string>(["project", "story"]);
const WORKFLOW_RUN_STATUSES = new Set<string>([
  "draft", "ready", "running", "blocked", "completed", "failed", "cancelled",
]);
const WORKFLOW_RUN_TOPOLOGIES = new Set<string>(["graphless", "workflow_graph"]);
const WORKFLOW_DEF_SOURCES = new Set<string>(["builtin_seed", "user_authored", "cloned"]);
const WORKFLOW_HOOK_TRIGGERS = new Set<string>([
  "user_prompt_submit",
  "before_tool", "after_tool", "after_turn", "before_stop", "session_terminal",
  "before_subagent_dispatch", "after_subagent_dispatch", "companion_result",
  "before_compact", "after_compact", "before_provider_request",
]);
const GATE_STRATEGIES = new Set<string>(["existence", "schema", "llm_judge"]);
const CONTEXT_STRATEGIES = new Set<string>(["full", "summary", "metadata_only", "custom"]);
const AGENT_REUSE_POLICIES = new Set<string>(["create_activity_agent", "continue_current_agent"]);
const RUNTIME_SESSION_POLICIES = new Set<string>(["create_new", "deliver_to_current_trace"]);
const ARTIFACT_ALIAS_POLICIES = new Set<string>(["latest", "per_attempt", "latest_and_history"]);
const ACTIVITY_TRANSITION_KINDS = new Set<string>(["flow", "artifact"]);
const LIFECYCLE_EXECUTION_EVENT_KINDS = new Set<string>([
  "activity_activated",
  "activity_completed",
  "constraint_blocked",
  "completion_evaluated",
  "artifact_appended",
  "context_injected",
]);

function normalizeEnum<T extends string>(value: unknown, allowed: Set<string>, field: string): T {
  const s = typeof value === "string" ? value : String(value ?? "");
  if (!allowed.has(s)) {
    throw new Error(`未知的 ${field}: ${s}`);
  }
  return s as T;
}

function mapJsonValue(value: unknown, field: string): JsonValue {
  if (isWorkflowJsonValue(value)) {
    return value;
  }
  throw new Error(`${field} 必须是 JSON value`);
}

function mapOptionalJsonValue(value: unknown, field: string): JsonValue | undefined {
  return value === undefined ? undefined : mapJsonValue(value, field);
}

const DEFAULT_WORKFLOW_TARGET_KINDS: WorkflowTargetKind[] = ["story"];

function normalizeTargetKinds(
  raw: unknown,
  field: string,
  fallback: WorkflowTargetKind[] = [],
): WorkflowTargetKind[] {
  const values = (raw === undefined || raw === null ? fallback : asStringArray(raw)).map((value) =>
    normalizeEnum<WorkflowTargetKind>(value, WORKFLOW_TARGET_KINDS, field),
  );
  const normalized = Array.from(new Set(values));
  if (normalized.length === 0) {
    throw new Error(`${field} 至少需要一个挂载类型`);
  }
  return normalized;
}

function mapValidationIssue(raw: Record<string, unknown>) {
  return {
    code: requireStringField(raw, "code"),
    message: requireStringField(raw, "message"),
    field_path: requireStringField(raw, "field_path"),
    severity: raw.severity === "warning" ? "warning" as const : "error" as const,
  };
}

// ─── 子结构 mapper ──────────────────────────────────────

function mapWorkflowContextBinding(raw: Record<string, unknown>): WorkflowContextBinding {
  return {
    locator: requireStringField(raw, "locator"),
    reason: requireStringField(raw, "reason"),
    required: raw.required !== false,
    title: optUndef(raw.title),
  };
}

function mapWorkflowInjectionSpec(raw: unknown): WorkflowInjectionSpec {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("AgentProcedure contract 缺少 injection");
  }
  return {
    guidance: optUndef(value.guidance),
    context_bindings: asRecordArray(value.context_bindings).map(mapWorkflowContextBinding),
  };
}

function mapWorkflowHookRuleSpec(raw: Record<string, unknown>): WorkflowHookRuleSpec {
  return {
    key: requireStringField(raw, "key"),
    trigger: normalizeEnum<WorkflowHookTrigger>(raw.trigger, WORKFLOW_HOOK_TRIGGERS, "workflow hook trigger"),
    description: optStringField(raw, "description"),
    preset: optUndef(raw.preset),
    params: mapOptionalJsonValue(raw.params, "workflow hook params"),
    script: optUndef(raw.script),
    enabled: raw.enabled !== false,
  };
}

function mapOutputPortDefinition(raw: Record<string, unknown>): OutputPortDefinition {
  return {
    key: requireStringField(raw, "key"),
    description: optStringField(raw, "description"),
    gate_strategy: normalizeEnum<GateStrategy>(
      raw.gate_strategy,
      GATE_STRATEGIES,
      "output port gate strategy",
    ),
    gate_params: mapOptionalJsonValue(raw.gate_params, "output port gate params"),
  };
}

function mapInputPortDefinition(raw: Record<string, unknown>): InputPortDefinition {
  return {
    key: requireStringField(raw, "key"),
    description: optStringField(raw, "description"),
    context_strategy: normalizeEnum<ContextStrategy>(
      raw.context_strategy,
      CONTEXT_STRATEGIES,
      "input port context strategy",
    ),
    context_template: optUndef(raw.context_template),
    standalone_fulfillment: mapStandaloneFulfillment(raw.standalone_fulfillment),
  };
}

function mapStandaloneFulfillment(raw: unknown): StandaloneFulfillment {
  if (raw === "required") return "required";
  const value = asRecord(raw);
  const optional = value ? asRecord(value.optional) : null;
  if (optional) {
    return {
      optional: {
        default_value: optUndef(optional.default_value),
      },
    };
  }
  throw new Error("input port standalone_fulfillment 非法");
}

function mapCapabilityDirective(raw: unknown, index: number): CapabilityDirective {
  const value = asRecord(raw);
  if (!value) {
    throw new Error(`capability_config.tool_directives[${index}] 必须是对象`);
  }
  const add = value.add;
  const remove = value.remove;
  if (typeof add === "string") {
    if (add.trim().length === 0) {
      throw new Error(`capability_config.tool_directives[${index}].add 不能为空`);
    }
    return { add };
  }
  if (typeof remove === "string") {
    if (remove.trim().length === 0) {
      throw new Error(`capability_config.tool_directives[${index}].remove 不能为空`);
    }
    return { remove };
  }
  throw new Error(`capability_config.tool_directives[${index}] 缺少 add / remove 字段`);
}

function mapWorkflowCapabilityConfig(raw: unknown): WorkflowCapabilityConfig {
  const value = asRecord(raw);
  const directivesRaw = value && Array.isArray(value.tool_directives)
    ? value.tool_directives
    : [];
  return {
    tool_directives: directivesRaw.map((item, idx) => mapCapabilityDirective(item, idx)),
    mount_directives: value && Array.isArray(value.mount_directives)
      ? value.mount_directives.map((item, index) =>
          mapJsonValue(item, `capability_config.mount_directives[${index}]`),
        )
      : [],
  };
}

function mapAgentProcedureContract(raw: unknown): AgentProcedureContract {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("AgentProcedure contract 缺失或不是对象");
  }
  return {
    injection: mapWorkflowInjectionSpec(value.injection),
    hook_rules: asRecordArray(value.hook_rules).map(mapWorkflowHookRuleSpec),
    capability_config: mapWorkflowCapabilityConfig(value.capability_config),
    output_ports: asRecordArray(value.output_ports).map(mapOutputPortDefinition),
    input_ports: asRecordArray(value.input_ports).map(mapInputPortDefinition),
  };
}

function mapArtifactAliasPolicy(raw: unknown): ArtifactAliasPolicy {
  return normalizeEnum<ArtifactAliasPolicy>(raw ?? "latest", ARTIFACT_ALIAS_POLICIES, "artifact alias policy");
}

function mapActivityExecutorSpec(raw: unknown): ActivityExecutorSpec {
  const value = asRecord(raw);
  if (!value) throw new Error("activity executor 缺失或不是对象");
  const kind = requireStringField(value, "kind");
  if (kind === "agent") {
    return {
      kind: "agent",
      procedure_key: requireStringField(value, "procedure_key"),
      agent_reuse_policy: normalizeEnum(value.agent_reuse_policy, AGENT_REUSE_POLICIES, "agent reuse policy"),
      runtime_session_policy: normalizeEnum(
        value.runtime_session_policy,
        RUNTIME_SESSION_POLICIES,
        "runtime session policy",
      ),
    };
  }
  if (kind === "function") {
    const type = requireStringField(value, "type");
    if (type === "api_request") {
      return {
        kind: "function",
        type: "api_request",
        method: requireStringField(value, "method"),
        url_template: requireStringField(value, "url_template"),
        body_template: mapOptionalJsonValue(value.body_template, "api request body template"),
      };
    }
    if (type === "bash_exec") {
      return {
        kind: "function",
        type: "bash_exec",
        command: requireStringField(value, "command"),
        args: asStringArray(value.args),
        working_directory: optUndef(value.working_directory),
      };
    }
  }
  if (kind === "human") {
    const type = requireStringField(value, "type");
    if (type === "approval") {
      return {
        kind: "human",
        type: "approval",
        form_schema_key: requireStringField(value, "form_schema_key"),
        title: optUndef(value.title),
      };
    }
  }
  throw new Error(`未知的 activity executor: ${kind}`);
}

function mapActivityCompletionPolicy(raw: unknown): ActivityCompletionPolicy {
  const value = asRecord(raw);
  if (!value) return { kind: "executor_terminal" };
  const kind = requireStringField(value, "kind");
  if (kind === "output_ports") {
    return { kind, required_ports: asStringArray(value.required_ports) };
  }
  if (kind === "executor_terminal") {
    return { kind };
  }
  if (kind === "human_decision") {
    return { kind, decision_port: requireStringField(value, "decision_port") };
  }
  if (kind === "hook_gate") {
    return { kind, hook_key: requireStringField(value, "hook_key") };
  }
  if (kind === "open_ended") {
    return { kind };
  }
  throw new Error(`未知的 activity completion policy: ${kind}`);
}

function mapActivityJoinPolicy(raw: unknown): ActivityJoinPolicy {
  if (raw == null) return "all";
  if (typeof raw === "string") {
    if (raw === "all" || raw === "any" || raw === "first") return raw;
    throw new Error(`未知的 activity join policy: ${raw}`);
  }
  const value = asRecord(raw);
  const nOfM = value ? asRecord(value.n_of_m) : null;
  if (nOfM && typeof nOfM.n === "number") {
    return { n_of_m: { n: nOfM.n } };
  }
  throw new Error("activity join policy 非法");
}

function mapActivityDefinition(raw: unknown): ActivityDefinition {
  const value = asRecord(raw);
  if (!value) throw new Error("activity definition 缺失或不是对象");
  const iteration = asRecord(value.iteration_policy);
  return {
    key: requireStringField(value, "key"),
    description: optStringField(value, "description"),
    executor: mapActivityExecutorSpec(value.executor),
    input_ports: asRecordArray(value.input_ports).map(mapInputPortDefinition),
    output_ports: asRecordArray(value.output_ports).map(mapOutputPortDefinition),
    completion_policy: mapActivityCompletionPolicy(value.completion_policy),
    iteration_policy: {
      max_attempts: typeof iteration?.max_attempts === "number" ? iteration.max_attempts : undefined,
      artifact_alias: mapArtifactAliasPolicy(iteration?.artifact_alias),
    },
    join_policy: mapActivityJoinPolicy(value.join_policy),
  };
}

function mapTransitionCondition(raw: unknown): TransitionCondition {
  const value = asRecord(raw);
  if (!value) return { kind: "always" };
  const kind = requireStringField(value, "kind");
  if (kind === "always") return { kind };
  if (kind === "artifact_field_equals") {
    return {
      kind,
      activity: requireStringField(value, "activity"),
      port: requireStringField(value, "port"),
      path: requireStringField(value, "path"),
      value: mapJsonValue(value.value, "transition condition value"),
    };
  }
  if (kind === "human_decision_equals") {
    return {
      kind,
      activity: requireStringField(value, "activity"),
      decision_port: requireStringField(value, "decision_port"),
      value: requireStringField(value, "value"),
    };
  }
  if (kind === "agent_signal_equals") {
    return {
      kind,
      activity: requireStringField(value, "activity"),
      signal_key: requireStringField(value, "signal_key"),
      value: mapJsonValue(value.value, "transition condition value"),
    };
  }
  throw new Error(`未知的 transition condition: ${kind}`);
}

function mapArtifactBinding(raw: unknown): ArtifactBinding {
  const value = asRecord(raw);
  if (!value) throw new Error("artifact binding 缺失或不是对象");
  return {
    from_activity: optUndef(value.from_activity),
    from_port: requireStringField(value, "from_port"),
    to_port: requireStringField(value, "to_port"),
    alias: mapArtifactAliasPolicy(value.alias),
  };
}

function mapActivityTransition(raw: unknown): ActivityTransition {
  const value = asRecord(raw);
  if (!value) throw new Error("activity transition 缺失或不是对象");
  return {
    from: requireStringField(value, "from"),
    to: requireStringField(value, "to"),
    kind: normalizeEnum(value.kind ?? "flow", ACTIVITY_TRANSITION_KINDS, "activity transition kind"),
    condition: mapTransitionCondition(value.condition),
    artifact_bindings: asRecordArray(value.artifact_bindings).map(mapArtifactBinding),
    max_traversals: typeof value.max_traversals === "number" ? value.max_traversals : undefined,
  };
}

function mapLifecycleExecutionEntry(raw: Record<string, unknown>): LifecycleExecutionEntry {
  return {
    timestamp: requireStringField(raw, "timestamp"),
    activity_key: requireStringField(raw, "activity_key"),
    event_kind: normalizeEnum<LifecycleExecutionEventKind>(raw.event_kind, LIFECYCLE_EXECUTION_EVENT_KINDS, "lifecycle execution event kind"),
    summary: requireStringField(raw, "summary"),
    detail: mapOptionalJsonValue(raw.detail, "lifecycle execution detail"),
  };
}

// ─── Entity mapper ─────────────────────────────────────

export function mapAgentProcedure(raw: Record<string, unknown>): AgentProcedure {
  return {
    id: requireStringField(raw, "id"),
    project_id: requireStringField(raw, "project_id"),
    key: requireStringField(raw, "key"),
    name: requireStringField(raw, "name"),
    description: optStringField(raw, "description"),
    target_kinds: normalizeTargetKinds(
      raw.target_kinds,
      "workflow target kinds",
      DEFAULT_WORKFLOW_TARGET_KINDS,
    ),
    source: normalizeEnum<DefinitionSource>(raw.source, WORKFLOW_DEF_SOURCES, "definition source"),
    installed_source: mapInstalledAssetSource(raw.installed_source),
    version: Number.isFinite(Number(raw.version)) ? Number(raw.version) : 1,
    contract: mapAgentProcedureContract(raw.contract),
    created_at: requireStringField(raw, "created_at"),
    updated_at: requireStringField(raw, "updated_at"),
  };
}

export function mapWorkflowGraph(raw: Record<string, unknown>): WorkflowGraph {
  return {
    id: requireStringField(raw, "id"),
    project_id: requireStringField(raw, "project_id"),
    key: requireStringField(raw, "key"),
    name: requireStringField(raw, "name"),
    description: optStringField(raw, "description"),
    target_kinds: normalizeTargetKinds(
      raw.target_kinds,
      "workflow graph target kinds",
      DEFAULT_WORKFLOW_TARGET_KINDS,
    ),
    source: normalizeEnum<DefinitionSource>(raw.source, WORKFLOW_DEF_SOURCES, "definition source"),
    installed_source: mapInstalledAssetSource(raw.installed_source),
    version: Number.isFinite(Number(raw.version)) ? Number(raw.version) : 1,
    entry_activity_key: requireStringField(raw, "entry_activity_key"),
    activities: Array.isArray(raw.activities) ? raw.activities.map(mapActivityDefinition) : [],
    transitions: Array.isArray(raw.transitions) ? raw.transitions.map(mapActivityTransition) : [],
    created_at: requireStringField(raw, "created_at"),
    updated_at: requireStringField(raw, "updated_at"),
  };
}

export function mapWorkflowRun(raw: Record<string, unknown>): WorkflowRun {
  return {
    id: requireStringField(raw, "id"),
    project_id: requireStringField(raw, "project_id"),
    topology: normalizeEnum<WorkflowRun["topology"]>(
      raw.topology,
      WORKFLOW_RUN_TOPOLOGIES,
      "workflow run topology",
    ),
    root_graph_id: optStringField(raw, "root_graph_id") || undefined,
    status: normalizeEnum<WorkflowRunStatus>(raw.status, WORKFLOW_RUN_STATUSES, "workflow run status"),
    execution_log: asRecordArray(raw.execution_log).map(mapLifecycleExecutionEntry),
    created_at: requireStringField(raw, "created_at"),
    updated_at: requireStringField(raw, "updated_at"),
    last_activity_at: requireStringField(raw, "last_activity_at"),
  };
}

export async function fetchAgentProcedures(opts?: {
  projectId?: string;
  targetKind?: WorkflowTargetKind;
}): Promise<AgentProcedure[]> {
  const params = new URLSearchParams();
  if (opts?.projectId) params.set("project_id", opts.projectId);
  if (opts?.targetKind) params.set("target_kind", opts.targetKind);
  const query = params.toString() ? `?${params}` : "";
  const raw = await api.get<Record<string, unknown>[]>(`/agent-procedures${query}`);
  return raw.map(mapAgentProcedure);
}

export async function fetchWorkflowGraphs(opts?: {
  projectId?: string;
  targetKind?: WorkflowTargetKind;
}): Promise<WorkflowGraph[]> {
  const params = new URLSearchParams();
  if (opts?.projectId) params.set("project_id", opts.projectId);
  if (opts?.targetKind) params.set("target_kind", opts.targetKind);
  const query = params.toString() ? `?${params}` : "";
  const raw = await api.get<Record<string, unknown>[]>(`/workflow-graphs${query}`);
  return raw.map(mapWorkflowGraph);
}

export async function createWorkflowGraph(input: {
  project_id: string;
  key: string;
  name: string;
  description?: string;
  target_kinds: WorkflowTargetKind[];
  entry_activity_key: string;
  activities: ActivityDefinition[];
  transitions: ActivityTransition[];
}): Promise<WorkflowGraph> {
  const raw = await api.post<Record<string, unknown>>("/workflow-graphs", {
    project_id: input.project_id,
    key: input.key,
    name: input.name,
    description: input.description,
    target_kinds: input.target_kinds,
    entry_activity_key: input.entry_activity_key,
    activities: input.activities,
    transitions: input.transitions,
  });
  return mapWorkflowGraph(raw);
}

export async function getWorkflowGraph(id: string): Promise<WorkflowGraph> {
  const raw = await api.get<Record<string, unknown>>(`/workflow-graphs/${id}`);
  return mapWorkflowGraph(raw);
}

export async function updateWorkflowGraph(
  id: string,
  input: {
    name?: string;
    description?: string;
    entry_activity_key?: string;
    activities?: ActivityDefinition[];
    transitions?: ActivityTransition[];
  },
): Promise<WorkflowGraph> {
  const raw = await api.put<Record<string, unknown>>(`/workflow-graphs/${id}`, {
    name: input.name,
    description: input.description,
    entry_activity_key: input.entry_activity_key,
    activities: input.activities,
    transitions: input.transitions,
  });
  return mapWorkflowGraph(raw);
}

export async function validateWorkflowGraph(input: {
  project_id: string;
  key: string;
  name: string;
  description?: string;
  target_kinds: WorkflowTargetKind[];
  entry_activity_key: string;
  activities: ActivityDefinition[];
  transitions: ActivityTransition[];
}): Promise<WorkflowValidationResult> {
  const raw = await api.post<Record<string, unknown>>("/workflow-graphs/validate", {
    project_id: input.project_id,
    key: input.key,
    name: input.name,
    description: input.description,
    target_kinds: input.target_kinds,
    entry_activity_key: input.entry_activity_key,
    activities: input.activities,
    transitions: input.transitions,
  });
  return {
    valid: Boolean(raw.valid),
    issues: Array.isArray(raw.issues)
      ? raw.issues.map((item, index) => {
          if (!item || typeof item !== "object") {
            throw new Error(`activity lifecycle validation issue[${index}] 必须是对象`);
          }
          return mapValidationIssue(item as Record<string, unknown>);
        })
      : [],
  };
}

export async function deleteWorkflowGraph(id: string): Promise<void> {
  await api.delete(`/workflow-graphs/${id}`);
}

export async function createAgentProcedure(input: {
  project_id: string;
  key: string;
  name: string;
  description?: string;
  target_kinds: WorkflowTargetKind[];
  contract: AgentProcedureContract;
}): Promise<AgentProcedure> {
  const raw = await api.post<Record<string, unknown>>("/agent-procedures", {
    project_id: input.project_id,
    key: input.key,
    name: input.name,
    description: input.description,
    target_kinds: input.target_kinds,
    contract: input.contract,
  });
  return mapAgentProcedure(raw);
}

export async function getAgentProcedure(id: string): Promise<AgentProcedure> {
  const raw = await api.get<Record<string, unknown>>(`/agent-procedures/${id}`);
  return mapAgentProcedure(raw);
}

export async function updateAgentProcedure(
  id: string,
  input: {
    name?: string;
    description?: string;
    contract?: AgentProcedureContract;
  },
): Promise<AgentProcedure> {
  const raw = await api.put<Record<string, unknown>>(`/agent-procedures/${id}`, {
    name: input.name,
    description: input.description,
    contract: input.contract,
  });
  return mapAgentProcedure(raw);
}

export async function validateAgentProcedure(input: {
  project_id: string;
  key: string;
  name: string;
  description?: string;
  target_kinds: WorkflowTargetKind[];
  contract: AgentProcedureContract;
}): Promise<WorkflowValidationResult> {
  const raw = await api.post<Record<string, unknown>>("/agent-procedures/validate", {
    project_id: input.project_id,
    key: input.key,
    name: input.name,
    description: input.description,
    target_kinds: input.target_kinds,
    contract: input.contract,
  });
  return {
    valid: Boolean(raw.valid),
    issues: Array.isArray(raw.issues)
      ? raw.issues.map((item, index) => {
          if (!item || typeof item !== "object") {
            throw new Error(`lifecycle validation issue[${index}] 必须是对象`);
          }
          return mapValidationIssue(item as Record<string, unknown>);
        })
      : [],
  };
}

export async function deleteAgentProcedure(id: string): Promise<void> {
  await api.delete(`/agent-procedures/${id}`);
}

// ─── Tool Catalog ──

/**
 * 查询能力下属的工具目录。
 * @param capabilityKeys 逗号分隔的 capability key，如 ["file_read", "canvas"]
 */
export async function fetchToolCatalog(capabilityKeys: string[]): Promise<ToolDescriptor[]> {
  const qs = capabilityKeys.join(",");
  return api.get<ToolDescriptor[]>(`/tool-catalog?capabilities=${encodeURIComponent(qs)}`);
}

// ─── Hook Presets ──

export async function fetchHookPresets(): Promise<HookRulePreset[]> {
  const raw = await api.get<Record<string, unknown>>("/hook-presets");
  const grouped = asRecord(raw.presets);
  if (!grouped) {
    throw new Error("hook presets 响应缺少 presets");
  }
  return Object.entries(grouped).flatMap(([groupKey, items]) => {
    if (!Array.isArray(items)) {
      throw new Error(`hook presets.${groupKey} 必须是数组`);
    }
    return items.map((item, index) => {
      if (!item || typeof item !== "object") {
        throw new Error(`hook presets.${groupKey}[${index}] 必须是对象`);
      }
      const record = item as Record<string, unknown>;
      return {
        key: requireStringField(record, "key"),
        trigger: normalizeEnum<WorkflowHookTrigger>(record.trigger, WORKFLOW_HOOK_TRIGGERS, "hook preset trigger"),
        label: requireStringField(record, "label"),
        description: optStringField(record, "description"),
        param_schema: asRecord(record.param_schema),
        script: typeof record.script === "string" ? record.script : undefined,
        source: record.source === "builtin" || record.source === "user_defined" ? record.source : undefined,
      };
    });
  });
}
