export type {
  AgentFrameRefDto,
  AgentRunRefDto,
  LifecycleRunRefDto,
  RuntimeSessionRefDto,
} from "../generated/agent-run-mailbox-contracts";
export type {
  SubjectRefDto,
} from "../generated/project-agent-contracts";
export type {
  ActiveRuntimeNodeRefDto,
  AgentFrameRuntimeView,
  AgentRunLineageRef,
  AgentRunListChild,
  AgentRunView,
  AgentRunWorkspaceControlPlaneStatus,
  AgentRunWorkspaceControlPlaneView,
  AgentRunWorkspaceListEntry,
  AgentRunWorkspaceListView,
  AgentRunWorkspaceView,
  LifecycleRunView,
  LifecycleSubjectAssociationDto,
  OrchestrationInstanceView,
  ProjectActiveAgentsView,
  RuntimeNodeView,
  RuntimeSessionExecutionAnchorDto,
  RuntimeSessionTraceView,
  SessionRuntimeControlView,
  SessionShellDto,
  SubjectExecutionView,
} from "../generated/workflow-contracts";

// ─── Subject Execution 索引 key ─────────────────────────

export function subjectExecutionKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}
