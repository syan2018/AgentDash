export type {
  AgentFrameRefDto,
  AgentRunRefDto,
  LifecycleRunRefDto,
} from "../generated/agent-run-mailbox-contracts";
export type {
  SubjectRefDto,
} from "../generated/project-agent-contracts";
export type {
  ActiveRuntimeNodeRefDto,
  AgentFrameRuntimeView,
  AgentRunListChildView,
  AgentRunListEntryView,
  AgentRunView,
  LifecycleRunView,
  LifecycleSubjectAssociationDto,
  OrchestrationInstanceView,
  ProjectActiveAgentsView,
  ProjectAgentRunListView,
  RuntimeNodeView,
  RuntimeSessionRefDto,
  RuntimeSessionTraceView,
  SubjectExecutionView,
  SubjectRuntimeAttemptView,
} from "../generated/workflow-contracts";

// ─── Subject Execution 索引 key ─────────────────────────

export function subjectExecutionKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}
