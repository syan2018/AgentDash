export type {
  AgentAssignmentRefDto,
  AgentFrameRefDto,
  LifecycleAgentRefDto,
  LifecycleRunRefDto,
  RuntimeSessionRefDto,
  SubjectRefDto,
} from "../generated/project-agent-contracts";
export type {
  ActiveActivityRefDto,
  ActivityAttemptView,
  ActivityStateView,
  AgentFrameRuntimeView,
  LifecycleAgentView,
  LifecycleRunView,
  LifecycleSubjectAssociationDto,
  ProjectActiveAgentsView,
  ProjectSessionListEntry,
  ProjectSessionListView,
  RuntimeSessionExecutionAnchorDto,
  RuntimeSessionTraceView,
  SessionRuntimeControlView,
  SessionShellDto,
  SubjectExecutionView,
  WorkflowGraphInstanceView,
} from "../generated/workflow-contracts";

// ─── Subject Execution 索引 key ─────────────────────────

export function subjectExecutionKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}
