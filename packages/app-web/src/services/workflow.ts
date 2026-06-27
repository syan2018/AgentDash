import { api } from "../api/client";
import { asRecord } from "../api/mappers";
import type {
  CapabilityCatalogResponse,
  HookPresetsResponse,
  PreflightWorkflowScriptRequest,
  PreflightWorkflowScriptResponse,
  SubmitOrchestrationHumanDecisionRequest,
  SubmitOrchestrationHumanDecisionResponse,
} from "../generated/workflow-contracts";
import type {
  ActivityDefinition,
  ActivityTransition,
  AgentProcedure,
  AgentProcedureContract,
  HookRulePreset,
  ToolDescriptor,
  WorkflowGraph,
  WorkflowHookTrigger,
  WorkflowTargetKind,
  WorkflowValidationResult,
} from "../types";

export function mapAgentProcedure(definition: AgentProcedure): AgentProcedure {
  return definition;
}

export function mapWorkflowGraph(definition: WorkflowGraph): WorkflowGraph {
  return definition;
}

export async function fetchAgentProcedures(opts?: {
  projectId?: string;
  targetKind?: WorkflowTargetKind;
}): Promise<AgentProcedure[]> {
  const params = new URLSearchParams();
  if (opts?.projectId) params.set("project_id", opts.projectId);
  if (opts?.targetKind) params.set("target_kind", opts.targetKind);
  const query = params.toString() ? `?${params}` : "";
  return api.get<AgentProcedure[]>(`/agent-procedures${query}`);
}

export async function fetchWorkflowGraphs(opts?: {
  projectId?: string;
  targetKind?: WorkflowTargetKind;
}): Promise<WorkflowGraph[]> {
  const params = new URLSearchParams();
  if (opts?.projectId) params.set("project_id", opts.projectId);
  if (opts?.targetKind) params.set("target_kind", opts.targetKind);
  const query = params.toString() ? `?${params}` : "";
  return api.get<WorkflowGraph[]>(`/workflow-graphs${query}`);
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
  return api.post<WorkflowGraph>("/workflow-graphs", input);
}

export async function getWorkflowGraph(id: string): Promise<WorkflowGraph> {
  return api.get<WorkflowGraph>(`/workflow-graphs/${id}`);
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
  return api.put<WorkflowGraph>(`/workflow-graphs/${id}`, input);
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
  return api.post<WorkflowValidationResult>("/workflow-graphs/validate", input);
}

export async function preflightWorkflowScript(
  input: PreflightWorkflowScriptRequest,
): Promise<PreflightWorkflowScriptResponse> {
  return api.post<PreflightWorkflowScriptResponse>("/workflow-scripts/preflight", input);
}

export async function deleteWorkflowGraph(id: string): Promise<void> {
  await api.delete(`/workflow-graphs/${id}`);
}

export async function submitOrchestrationHumanDecision(
  runId: string,
  input: SubmitOrchestrationHumanDecisionRequest,
): Promise<SubmitOrchestrationHumanDecisionResponse> {
  return api.post<SubmitOrchestrationHumanDecisionResponse>(
    `/lifecycle-runs/${encodeURIComponent(runId)}/orchestration-human-decisions`,
    input,
  );
}

export async function createAgentProcedure(input: {
  project_id: string;
  key: string;
  name: string;
  description?: string;
  target_kinds: WorkflowTargetKind[];
  contract: AgentProcedureContract;
}): Promise<AgentProcedure> {
  return api.post<AgentProcedure>("/agent-procedures", input);
}

export async function getAgentProcedure(id: string): Promise<AgentProcedure> {
  return api.get<AgentProcedure>(`/agent-procedures/${id}`);
}

export async function updateAgentProcedure(
  id: string,
  input: {
    name?: string;
    description?: string;
    contract?: AgentProcedureContract;
  },
): Promise<AgentProcedure> {
  return api.put<AgentProcedure>(`/agent-procedures/${id}`, input);
}

export async function validateAgentProcedure(input: {
  project_id: string;
  key: string;
  name: string;
  description?: string;
  target_kinds: WorkflowTargetKind[];
  contract: AgentProcedureContract;
}): Promise<WorkflowValidationResult> {
  return api.post<WorkflowValidationResult>("/agent-procedures/validate", input);
}

export async function deleteAgentProcedure(id: string): Promise<void> {
  await api.delete(`/agent-procedures/${id}`);
}

export async function fetchCapabilityCatalog(
  capabilityKeys?: string[],
): Promise<CapabilityCatalogResponse> {
  const params = new URLSearchParams();
  if (capabilityKeys && capabilityKeys.length > 0) {
    params.set("capabilities", capabilityKeys.join(","));
  }
  const query = params.toString();
  return api.get<CapabilityCatalogResponse>(`/tool-catalog${query ? `?${query}` : ""}`);
}

export async function fetchToolCatalog(capabilityKeys: string[]): Promise<ToolDescriptor[]> {
  const catalog = await fetchCapabilityCatalog(capabilityKeys);
  return catalog.capabilities.flatMap((entry) => entry.tools);
}

export async function fetchHookPresets(): Promise<HookRulePreset[]> {
  const response = await api.get<HookPresetsResponse>("/hook-presets");
  return Object.entries(response.presets).flatMap(([groupKey, items]) =>
    (items ?? []).map((item, index) => {
      const trigger = workflowHookTrigger(item.trigger);
      if (!trigger) {
        throw new Error(`hook presets.${groupKey}[${index}].trigger 非法`);
      }
      return {
        key: item.key,
        trigger,
        label: item.label,
        description: item.description,
        param_schema: asRecord(item.param_schema),
        script: item.script,
        source: item.source === "builtin" || item.source === "user_defined" ? item.source : undefined,
      };
    }),
  );
}

function workflowHookTrigger(raw: unknown): WorkflowHookTrigger | null {
  switch (raw) {
    case "user_prompt_submit":
    case "before_tool":
    case "after_tool":
    case "after_turn":
    case "before_stop":
    case "session_terminal":
    case "before_subagent_dispatch":
    case "after_subagent_dispatch":
    case "companion_result":
    case "before_compact":
    case "after_compact":
    case "before_provider_request":
      return raw;
    default:
      return null;
  }
}
