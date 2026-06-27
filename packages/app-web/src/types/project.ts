import type { ContextSourceRef as GeneratedContextSourceRef } from "../generated/context-contracts";
import type {
  AgentPreset as GeneratedAgentPreset,
  ProjectAccessSummaryResponse,
  ProjectConfig as GeneratedProjectConfig,
  ProjectResponse,
  ProjectSubjectGrantResponse,
} from "../generated/project-contracts";
import type {
  ProjectVfsMountContentDto,
  ProjectVfsMountResponse,
} from "../generated/vfs-contracts";

export type AgentPreset = GeneratedAgentPreset;
export type ContextSourceRef = GeneratedContextSourceRef;
export type Project = ProjectResponse;
export type ProjectAccessSummary = ProjectAccessSummaryResponse;
export type ProjectConfig = GeneratedProjectConfig;
export type ProjectSubjectGrant = ProjectSubjectGrantResponse;
export type ProjectVfsMountContent = ProjectVfsMountContentDto;
export type ProjectVfsMount = ProjectVfsMountResponse;

export type {
  ContextSourceKind,
} from "../generated/context-contracts";

export type {
  ProjectRole,
  ProjectSubjectType,
  ProjectVisibility,
} from "../generated/project-contracts";
