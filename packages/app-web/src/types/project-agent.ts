import type { CapabilityDirective } from "./workflow";
import type { JsonValue } from "../generated/common-contracts";
import type {
  CreateProjectAgentRunRequest as GeneratedCreateProjectAgentRunRequest,
  ProjectAgent as GeneratedProjectAgent,
  ProjectAgentExecutor as GeneratedProjectAgentExecutor,
  ProjectAgentRunStartResult as GeneratedProjectAgentRunStartResult,
  ProjectAgentSummary as GeneratedProjectAgentSummary,
  ThinkingLevel as GeneratedThinkingLevel,
} from "../generated/project-agent-contracts";

export type ThinkingLevel = GeneratedThinkingLevel;

export const THINKING_LEVEL_OPTIONS: Array<{ value: ThinkingLevel; label: string }> = [
  { value: "off", label: "关闭" },
  { value: "minimal", label: "最少" },
  { value: "low", label: "低" },
  { value: "medium", label: "中" },
  { value: "high", label: "高" },
  { value: "xhigh", label: "超高" },
];

export function isThinkingLevel(value: unknown): value is ThinkingLevel {
  return (
    value === "off"
    || value === "minimal"
    || value === "low"
    || value === "medium"
    || value === "high"
    || value === "xhigh"
  );
}

export interface ProjectVfsMountExposureGrant {
  [key: string]: JsonValue | undefined;
  mount_id: string;
  capabilities: Array<"read" | "write" | "list" | "search">;
}

export interface AgentPresetConfig extends Record<string, unknown> {
  executor?: string;
  provider_id?: string;
  model_id?: string;
  agent_id?: string;
  thinking_level?: ThinkingLevel;
  permission_policy?: string;
  system_prompt?: string;
  display_name?: string;
  description?: string;
  capability_directives?: CapabilityDirective[];
  project_vfs_mount_exposure_grants?: ProjectVfsMountExposureGrant[];
  skill_asset_keys?: string[];
  default_companion_enabled?: boolean;
  extra_companions?: string[];
}

export type ProjectAgent = Pick<
  GeneratedProjectAgent,
  "id" | "project_id" | "name" | "agent_type" | "knowledge_enabled" | "created_at" | "updated_at"
> & {
  config: AgentPresetConfig;
  default_lifecycle_key: string | null;
};

export type ProjectAgentExecutor = Omit<
  GeneratedProjectAgentExecutor,
  "provider_id" | "model_id" | "agent_id" | "thinking_level" | "permission_policy"
> & {
  provider_id?: string | null;
  model_id?: string | null;
  agent_id?: string | null;
  thinking_level?: ThinkingLevel | null;
  permission_policy?: string | null;
};

export type ProjectAgentSummary = Omit<
  GeneratedProjectAgentSummary,
  "executor" | "preset_name"
> & {
  executor: ProjectAgentExecutor;
  preset_name?: string | null;
};

export type CreateProjectAgentRunRequest = GeneratedCreateProjectAgentRunRequest;

export type ProjectAgentRunStartResult = Omit<
  GeneratedProjectAgentRunStartResult,
  "agent"
> & {
  agent: ProjectAgentSummary;
};
