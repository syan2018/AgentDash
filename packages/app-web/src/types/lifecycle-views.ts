export type {
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
  RuntimeSessionRefDto,
  RuntimeSessionTraceView,
  SubjectExecutionView,
  SubjectRefDto,
  WorkflowGraphInstanceView,
} from "../generated/workflow-contracts";

// ─── Subject Execution 索引 key ─────────────────────────

export function subjectExecutionKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}
