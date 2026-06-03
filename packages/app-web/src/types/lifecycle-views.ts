export type {
  ActiveActivityRefDto,
  ActivityAttemptView,
  ActivityStateView,
  AgentAssignmentRefDto,
  AgentFrameRefDto,
  AgentFrameRuntimeView,
  LifecycleAgentRefDto,
  LifecycleAgentView,
  LifecycleRunRefDto,
  LifecycleRunView,
  LifecycleSubjectAssociationDto,
  ProjectActiveAgentsView,
  ProjectSessionListEntry,
  ProjectSessionListView,
  RuntimeSessionExecutionAnchorDto,
  RuntimeSessionRefDto,
  RuntimeSessionTraceView,
  SessionRuntimeControlView,
  SessionShellDto,
  SubjectExecutionView,
  SubjectRefDto,
  WorkflowGraphInstanceView,
} from "../generated/workflow-contracts";

// ─── Subject Execution 索引 key ─────────────────────────

export function subjectExecutionKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}
