// ─── Workflow ─────────────────────────────────────────
import type { JsonValue } from "../generated/common-contracts";
import type {
  ActivityCompletionPolicy,
  ActivityDefinition as GeneratedActivityDefinition,
  ActivityExecutorSpec,
  ActivityIterationPolicy,
  ActivityJoinPolicy,
  ActivityTransition as GeneratedActivityTransition,
  ActivityTransitionKind,
  AgentActivityExecutorSpec,
  AgentReusePolicy,
  ApiRequestExecutorSpec,
  ArtifactAliasPolicy,
  ArtifactBinding,
  BashExecExecutorSpec,
  CapabilityConfig as GeneratedCapabilityConfig,
  ContextStrategy,
  EffectiveSessionContract,
  ExecutorRunRef,
  FunctionActivityExecutorSpec,
  GateStrategy,
  HumanActivityExecutorSpec,
  HumanApprovalExecutorSpec,
  InputPortDefinition,
  LifecycleExecutionEntry,
  LifecycleExecutionEventKind,
  LifecycleRunStatus,
  OutputPortDefinition,
  RuntimeThreadPolicy,
  StandaloneFulfillment,
  ToolCapabilityDirective,
  ToolCapabilityPath,
  TransitionCondition,
  ValidationIssue,
  ValidationSeverity,
  WorkflowContextBinding,
  AgentProcedureContract as GeneratedAgentProcedureContract,
  AgentProcedureResponse,
  CapabilityCatalogEntryDto,
  CapabilityCatalogResponse,
  CapabilityScopeDto,
  DefinitionSource,
  PlatformMcpScopeDto,
  ToolClusterDto,
  WorkflowGraphResponse,
  WorkflowHookRuleSpec,
  WorkflowHookTrigger,
  WorkflowInjectionSpec,
  WorkflowTargetKind,
  ToolDescriptorDto,
  ToolSourceDto,
} from "../generated/workflow-contracts";

export type {
  ActivityCompletionPolicy,
  ActivityExecutorSpec,
  ActivityIterationPolicy,
  ActivityJoinPolicy,
  ActivityTransitionKind,
  AgentActivityExecutorSpec,
  AgentReusePolicy,
  ApiRequestExecutorSpec,
  ArtifactAliasPolicy,
  ArtifactBinding,
  BashExecExecutorSpec,
  ContextStrategy,
  EffectiveSessionContract,
  ExecutorRunRef,
  FunctionActivityExecutorSpec,
  GateStrategy,
  HumanActivityExecutorSpec,
  HumanApprovalExecutorSpec,
  InputPortDefinition,
  JsonValue,
  LifecycleExecutionEntry,
  LifecycleExecutionEventKind,
  LifecycleRunStatus,
  OutputPortDefinition,
  RuntimeThreadPolicy,
  StandaloneFulfillment,
  ToolCapabilityDirective,
  ToolCapabilityPath,
  TransitionCondition,
  ValidationIssue,
  ValidationSeverity,
  WorkflowContextBinding,
  DefinitionSource,
  CapabilityCatalogEntryDto,
  CapabilityCatalogResponse,
  CapabilityScopeDto,
  PlatformMcpScopeDto,
  ToolClusterDto,
  ToolDescriptorDto,
  ToolSourceDto,
  WorkflowTargetKind,
  WorkflowHookRuleSpec,
  WorkflowHookTrigger,
  WorkflowInjectionSpec,
};

export type WorkflowRunStatus = LifecycleRunStatus;
export type CapabilityDirective = ToolCapabilityDirective;

export type WorkflowCapabilityConfig = GeneratedCapabilityConfig & {
  tool_directives: ToolCapabilityDirective[];
  mount_directives: unknown[];
};

export type CapabilityConfig = WorkflowCapabilityConfig;

export type AgentProcedureContract = Omit<
  GeneratedAgentProcedureContract,
  "capability_config" | "output_ports" | "input_ports"
> & {
  capability_config: WorkflowCapabilityConfig;
  output_ports: OutputPortDefinition[];
  input_ports: InputPortDefinition[];
};

export type ActivityDefinition = GeneratedActivityDefinition;

export type ActivityTransition = GeneratedActivityTransition;

export function isWorkflowJsonValue(value: unknown): value is JsonValue {
  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return true;
  }
  if (Array.isArray(value)) {
    return value.every(isWorkflowJsonValue);
  }
  if (typeof value !== "object") {
    return false;
  }
  return Object.values(value).every(isWorkflowJsonValue);
}

export interface HookRulePreset {
  key: string;
  trigger: WorkflowHookTrigger;
  label: string;
  description: string;
  param_schema?: Record<string, unknown> | null;
  script?: string;
  source?: "builtin" | "user_defined";
}

/**
 * Capability 路径 —— 统一表达「能力级」和「工具级」两种寻址。
 *
 * JSON 形式序列化为 qualified string：`"file_read"` / `"file_read::fs_grep"`。
 */
export interface CapabilityPath {
  capability: string;
  tool: string | null;
}

const CAPABILITY_PATH_SEPARATOR = "::";

export function toQualifiedString(path: CapabilityPath): string {
  return path.tool === null || path.tool === ""
    ? path.capability
    : `${path.capability}${CAPABILITY_PATH_SEPARATOR}${path.tool}`;
}

export function parseCapabilityPath(qualified: string): CapabilityPath {
  const trimmed = qualified.trim();
  if (trimmed.length === 0) {
    throw new Error("CapabilityPath 不能为空");
  }
  const parts = trimmed.split(CAPABILITY_PATH_SEPARATOR);
  if (parts.length === 1) {
    return { capability: trimmed, tool: null };
  }
  if (parts.length === 2) {
    const [cap, tool] = parts;
    if (cap.length === 0 || tool.length === 0) {
      throw new Error(`CapabilityPath 非法：${qualified}`);
    }
    return { capability: cap, tool };
  }
  throw new Error(`CapabilityPath 不允许多级嵌套：${qualified}`);
}

export function directivePath(directive: CapabilityDirective): string {
  return "add" in directive ? directive.add : directive.remove;
}

export function directiveKind(directive: CapabilityDirective): "add" | "remove" {
  return "add" in directive ? "add" : "remove";
}

export type ToolDescriptor = ToolDescriptorDto;

export interface WorkflowValidationResult {
  valid: boolean;
  issues: ValidationIssue[];
}

export interface WorkflowTemplateWorkflow {
  key: string;
  name: string;
  description: string;
  contract: AgentProcedureContract;
}

export interface WorkflowTemplate {
  key: string;
  name: string;
  description: string;
  target_kinds: WorkflowTargetKind[];
  workflows: WorkflowTemplateWorkflow[];
  lifecycle: {
    key: string;
    name: string;
    description: string;
    entry_activity_key: string;
    activities: ActivityDefinition[];
    transitions: ActivityTransition[];
  };
}

export type AgentProcedure = AgentProcedureResponse;

export type WorkflowGraph = WorkflowGraphResponse;

export interface WorkflowRun {
  id: string;
  project_id: string;
  topology: "plain" | "workflow_graph";
  status: WorkflowRunStatus;
  execution_log: LifecycleExecutionEntry[];
  created_at: string;
  updated_at: string;
  last_activity_at: string;
}
