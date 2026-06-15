import { useEffect } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import type {
  AgentFrameRuntimeView,
  AgentRunView,
  LifecycleRunView,
  OrchestrationInstanceView,
  RuntimeNodeView,
  SubjectExecutionView,
} from "../types";
import { subjectExecutionKey } from "../types";
import { useLifecycleStore } from "../stores/lifecycleStore";
import { agentRunWorkspacePath } from "../features/agent/agent-run-paths";

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-[8px] border border-border bg-background">
      <div className="border-b border-border px-4 py-3">
        <h2 className="text-sm font-semibold text-foreground">{title}</h2>
      </div>
      <div className="p-4">{children}</div>
    </section>
  );
}

function EmptyHint({ message }: { message: string }) {
  return (
    <p className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-6 text-center text-sm text-muted-foreground">
      {message}
    </p>
  );
}

function RuntimeNodeTree({ nodes }: { nodes: RuntimeNodeView[] }) {
  if (nodes.length === 0) return <EmptyHint message="暂无 runtime node" />;
  return (
    <div className="space-y-2">
      {nodes.map((node) => (
        <div
          key={`${node.node_path}:${node.attempt}`}
          className="border-l border-border py-1 pl-3"
        >
          <div className="flex min-w-0 items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="truncate font-mono text-xs text-foreground">{node.node_path}</p>
              <p className="text-xs text-muted-foreground">
                {node.kind} · attempt {node.attempt}
              </p>
            </div>
            <span className="shrink-0 rounded-[6px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground">
              {node.status}
            </span>
          </div>
          {node.children.length > 0 && (
            <div className="mt-2">
              <RuntimeNodeTree nodes={node.children} />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function OrchestrationSummary({ orchestration }: { orchestration: OrchestrationInstanceView }) {
  return (
    <div className="space-y-3 py-3">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="font-mono text-muted-foreground">{orchestration.orchestration_id}</span>
        <span className="rounded-[6px] border border-border bg-background px-2 py-1 text-muted-foreground">
          {orchestration.role}
        </span>
        <span className="rounded-[6px] border border-border bg-background px-2 py-1 text-muted-foreground">
          {orchestration.status}
        </span>
        <span className="rounded-[6px] border border-border bg-background px-2 py-1 text-muted-foreground">
          ready {orchestration.ready_node_ids.length}
        </span>
      </div>
      <RuntimeNodeTree nodes={orchestration.nodes} />
    </div>
  );
}

function RunSummary({ lifecycleRun }: { lifecycleRun: LifecycleRunView }) {
  const navigate = useNavigate();
  return (
    <div className="space-y-4">
      <Section title="Run">
        <div className="flex flex-wrap gap-2 text-xs">
          <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 font-mono text-muted-foreground">
            {lifecycleRun.run_ref.run_id}
          </span>
          <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 text-muted-foreground">
            {lifecycleRun.status}
          </span>
          <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 text-muted-foreground">
            orchestrations {lifecycleRun.orchestrations.length}
          </span>
          <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 text-muted-foreground">
            agent {lifecycleRun.agents.length}
          </span>
        </div>
      </Section>

      <Section title="Orchestrations">
        {lifecycleRun.orchestrations.length === 0 ? (
          <EmptyHint message="暂无 orchestration" />
        ) : (
          <div className="divide-y divide-border">
            {lifecycleRun.orchestrations.map((orchestration) => (
              <OrchestrationSummary
                key={orchestration.orchestration_id}
                orchestration={orchestration}
              />
            ))}
          </div>
        )}
      </Section>

      <Section title="Agents">
        {lifecycleRun.agents.length === 0 ? (
          <EmptyHint message="暂无 agent" />
        ) : (
          <div className="space-y-2">
            {lifecycleRun.agents.map((agent) => (
              <button
                key={agent.agent_ref.agent_id}
                type="button"
                onClick={() => navigate(`/agent/${agent.agent_ref.agent_id}`, {
                  state: { run_id: lifecycleRun.run_ref.run_id, frame_id: agent.current_frame_id ?? null },
                })}
                className="flex w-full items-center justify-between gap-3 rounded-[8px] border border-border bg-secondary/20 px-3 py-2 text-left hover:bg-secondary/40"
              >
                <span className="flex min-w-0 items-center gap-1.5 truncate text-sm text-foreground">
                  {agent.agent_kind || agent.agent_role}
                  {agent.agent_role && agent.agent_role !== "primary" && (
                    <span className="shrink-0 rounded-[6px] bg-secondary px-1.5 text-[10px] text-muted-foreground">
                      {agent.agent_role}
                    </span>
                  )}
                </span>
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {agent.agent_ref.agent_id.slice(0, 8)}
                </span>
              </button>
            ))}
          </div>
        )}
      </Section>

      <Section title="Runtime Traces">
        {lifecycleRun.runtime_trace_refs.length === 0 ? (
          <EmptyHint message="暂无 runtime trace" />
        ) : (
          <div className="flex flex-wrap gap-2">
            {lifecycleRun.runtime_trace_refs.map((ref) => (
              <button
                key={ref.runtime_session_id}
                type="button"
                onClick={() => {}}
                className="rounded-[6px] border border-border bg-secondary/40 px-2 py-1 font-mono text-xs text-muted-foreground hover:text-foreground"
              >
                trace {ref.runtime_session_id}
              </button>
            ))}
          </div>
        )}
      </Section>
    </div>
  );
}

function SubjectExecutionSummary({ view }: { view: SubjectExecutionView }) {
  const navigate = useNavigate();
  return (
    <div className="space-y-4">
      <Section title="Subject">
        <div className="flex flex-wrap gap-2 text-xs">
          <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 text-muted-foreground">
            {view.subject_ref.kind}
          </span>
          <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 font-mono text-muted-foreground">
            {view.subject_ref.id}
          </span>
        </div>
      </Section>
      {view.current_agent && (
        <Section title="Current Agent">
          <button
            type="button"
            onClick={() => navigate(`/agent/${view.current_agent?.agent_ref.agent_id}`, {
              state: { run_id: view.current_agent?.agent_ref.run_id },
            })}
            className="font-mono text-sm text-primary hover:underline"
          >
            {view.current_agent.agent_ref.agent_id}
          </button>
        </Section>
      )}
      <Section title="Runs">
        {view.runs.length === 0 ? (
          <EmptyHint message="暂无 run" />
        ) : (
          <div className="space-y-2">
            {view.runs.map((run) => (
              <button
                key={run.run_ref.run_id}
                type="button"
                onClick={() => navigate(`/run/${run.run_ref.run_id}`)}
                className="flex w-full items-center justify-between rounded-[8px] border border-border bg-secondary/20 px-3 py-2 text-left hover:bg-secondary/40"
              >
                <span className="font-mono text-xs text-primary">{run.run_ref.run_id}</span>
                <span className="text-xs text-muted-foreground">{run.status}</span>
              </button>
            ))}
          </div>
        )}
      </Section>
    </div>
  );
}

function AgentSummary({
  agent,
  frame,
}: {
  agent: AgentRunView | null;
  frame: AgentFrameRuntimeView | null;
}) {
  const navigate = useNavigate();
  if (!agent) {
    return <EmptyHint message="当前缓存中没有这个 agent。请从 ProjectAgent launch、run 或 subject 页面进入。" />;
  }
  return (
    <div className="space-y-4">
      <Section title="Agent">
        <div className="space-y-2 text-sm">
          <p className="font-mono text-xs text-muted-foreground">{agent.agent_ref.agent_id}</p>
          <p className="text-foreground">
            {agent.agent_kind || agent.agent_role}
            {agent.agent_role && agent.agent_role !== "primary" && (
              <span className="ml-1.5 rounded-[6px] bg-secondary px-1.5 text-[10px] text-muted-foreground">
                {agent.agent_role}
              </span>
            )}
          </p>
          <p className="text-xs text-muted-foreground">status: {agent.status}</p>
          <button
            type="button"
            onClick={() => navigate(agentRunWorkspacePath(agent.agent_ref.run_id, agent.agent_ref.agent_id))}
            className="font-mono text-xs text-primary hover:underline"
          >
            打开 AgentRun
          </button>
        </div>
      </Section>

      <Section title="Frame Runtime">
        {!frame ? (
          <EmptyHint message="暂无 frame runtime 投影" />
        ) : (
          <div className="space-y-3">
            <div className="flex flex-wrap gap-2 text-xs">
              <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 font-mono text-muted-foreground">
                {frame.frame_ref.frame_id}
              </span>
            </div>
            {frame.runtime_session_refs.length > 0 && (
              <div className="flex flex-wrap gap-2">
                {frame.runtime_session_refs.map((ref) => (
                  <button
                    key={ref.runtime_session_id}
                    type="button"
                    onClick={() => {}}
                    className="rounded-[6px] border border-border bg-secondary/40 px-2 py-1 font-mono text-xs text-muted-foreground hover:text-foreground"
                  >
                    trace {ref.runtime_session_id.slice(0, 8)}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
      </Section>
    </div>
  );
}

export function LifecycleRunPage() {
  const { runId: routeRunId = "" } = useParams<{ runId: string }>();
  const lifecycleRunId = routeRunId;
  const fetchAndIngestLifecycleRun = useLifecycleStore((s) => s.fetchAndIngestLifecycleRun);
  const lifecycleRun = useLifecycleStore((s) => s.lifecycleRuns.get(lifecycleRunId) ?? null);
  const error = useLifecycleStore((s) => s.error);

  useEffect(() => {
    if (lifecycleRunId) void fetchAndIngestLifecycleRun(lifecycleRunId);
  }, [fetchAndIngestLifecycleRun, lifecycleRunId]);

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="mx-auto max-w-5xl space-y-4">
        <h1 className="text-lg font-semibold text-foreground">Lifecycle Run</h1>
        {error && <p className="text-sm text-destructive">{error}</p>}
        {lifecycleRun ? <RunSummary lifecycleRun={lifecycleRun} /> : <EmptyHint message="正在加载 run view" />}
      </div>
    </div>
  );
}

export function SubjectExecutionPage() {
  const { kind = "", id = "" } = useParams<{ kind: string; id: string }>();
  const fetchSubjectExecution = useLifecycleStore((s) => s.fetchSubjectExecution);
  const view = useLifecycleStore((s) => s.subjectExecutions.get(subjectExecutionKey(kind, id)) ?? null);
  const error = useLifecycleStore((s) => s.error);

  useEffect(() => {
    if (kind && id) void fetchSubjectExecution(kind, id);
  }, [fetchSubjectExecution, id, kind]);

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="mx-auto max-w-5xl space-y-4">
        <h1 className="text-lg font-semibold text-foreground">Subject Execution</h1>
        {error && <p className="text-sm text-destructive">{error}</p>}
        {view ? <SubjectExecutionSummary view={view} /> : <EmptyHint message="正在加载 subject execution view" />}
      </div>
    </div>
  );
}

function isAgentRouteState(value: unknown): value is { run_id?: string | null; frame_id?: string | null } {
  return Boolean(value && typeof value === "object");
}

export function LifecycleAgentPage() {
  const { agentId = "" } = useParams<{ agentId: string }>();
  const location = useLocation();
  const routeState = isAgentRouteState(location.state) ? location.state : {};
  const fetchAndIngestLifecycleRun = useLifecycleStore((s) => s.fetchAndIngestLifecycleRun);
  const fetchFrame = useLifecycleStore((s) => s.fetchFrame);
  const agent = useLifecycleStore((s) => s.agents.get(agentId) ?? null);
  const frameId = routeState.frame_id ?? agent?.current_frame_id ?? null;
  const frame = useLifecycleStore((s) => (frameId ? s.frames.get(frameId) ?? null : null));
  const error = useLifecycleStore((s) => s.error);

  useEffect(() => {
    if (routeState.run_id) void fetchAndIngestLifecycleRun(routeState.run_id);
  }, [fetchAndIngestLifecycleRun, routeState.run_id]);

  useEffect(() => {
    if (frameId) void fetchFrame(frameId);
  }, [fetchFrame, frameId]);

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="mx-auto max-w-5xl space-y-4">
        <h1 className="text-lg font-semibold text-foreground">Lifecycle Agent</h1>
        {error && <p className="text-sm text-destructive">{error}</p>}
        <AgentSummary agent={agent} frame={frame} />
      </div>
    </div>
  );
}
