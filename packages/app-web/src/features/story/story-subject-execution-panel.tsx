import { useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import type { Story, SubjectExecutionView } from "../../types";
import { subjectExecutionKey } from "../../types";
import { useLifecycleStore } from "../../stores/lifecycleStore";

interface StorySubjectExecutionPanelProps {
  story: Story;
}

function JsonBlock({ value }: { value: unknown }) {
  const text = useMemo(() => JSON.stringify(value ?? null, null, 2), [value]);
  return (
    <pre className="max-h-56 overflow-auto rounded-[8px] border border-border bg-secondary/20 p-3 text-xs text-muted-foreground">
      {text}
    </pre>
  );
}

function SubjectExecutionContent({ view }: { view: SubjectExecutionView | null }) {
  const navigate = useNavigate();

  if (!view) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-center">
        <div>
          <p className="text-sm font-medium text-foreground">暂无 Story 执行投影</p>
          <p className="mt-1 text-xs text-muted-foreground">
            Story 关联到 lifecycle run 或 agent 后会在这里展示。
          </p>
        </div>
      </div>
    );
  }

  const latestRuntimeNode = view.latest_runtime_node;

  return (
    <div className="space-y-4 p-4">
      <section className="grid gap-3 lg:grid-cols-3">
        <div className="rounded-[8px] border border-border bg-background p-3">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Subject</p>
          <p className="mt-2 font-mono text-xs text-foreground">
            {view.subject_ref.kind}:{view.subject_ref.id.slice(0, 8)}
          </p>
        </div>
        <div className="rounded-[8px] border border-border bg-background p-3">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Current Agent</p>
          {view.current_agent ? (
            <button
              type="button"
              onClick={() => navigate(`/agent/${view.current_agent?.agent_ref.agent_id}`, {
                state: { run_id: view.current_agent?.agent_ref.run_id },
              })}
              className="mt-2 block w-full truncate text-left font-mono text-xs text-primary hover:underline"
            >
              {view.current_agent.agent_ref.agent_id}
            </button>
          ) : (
            <p className="mt-2 text-xs text-muted-foreground">未分配</p>
          )}
        </div>
        <div className="rounded-[8px] border border-border bg-background p-3">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Latest Runtime Node</p>
          {latestRuntimeNode ? (
            <p className="mt-2 text-xs text-foreground">
              {latestRuntimeNode.node_path} #{latestRuntimeNode.attempt} · {latestRuntimeNode.status}
            </p>
          ) : (
            <p className="mt-2 text-xs text-muted-foreground">暂无</p>
          )}
        </div>
      </section>

      <section className="space-y-2">
        <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Lifecycle Runs</p>
        {view.runs.length === 0 ? (
          <p className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-6 text-center text-sm text-muted-foreground">
            暂无 run
          </p>
        ) : (
          view.runs.map((run) => (
            <div key={run.run_ref.run_id} className="rounded-[8px] border border-border bg-background p-3">
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => navigate(`/run/${run.run_ref.run_id}`)}
                  className="truncate font-mono text-xs text-primary hover:underline"
                >
                  {run.run_ref.run_id}
                </button>
                <span className="rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {run.status}
                </span>
              </div>
              <div className="mt-2 flex flex-wrap gap-1.5">
                {run.agents.map((agent) => (
                  <button
                    key={agent.agent_ref.agent_id}
                    type="button"
                    onClick={() => navigate(`/agent/${agent.agent_ref.agent_id}`, {
                      state: { run_id: run.run_ref.run_id },
                    })}
                    className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground hover:text-foreground"
                  >
                    agent {agent.agent_ref.agent_id.slice(0, 8)}
                  </button>
                ))}
                {run.runtime_trace_refs.map((ref) => (
                  <button
                    key={ref.runtime_session_id}
                    type="button"
                    onClick={() => navigate(`/session/${ref.runtime_session_id}`)}
                    className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground hover:text-foreground"
                  >
                    trace {ref.runtime_session_id.slice(0, 8)}
                  </button>
                ))}
              </div>
            </div>
          ))
        )}
      </section>

      <section className="space-y-2">
        <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Artifacts</p>
        <JsonBlock value={view.artifacts} />
      </section>
    </div>
  );
}

export function StorySubjectExecutionPanel({ story }: StorySubjectExecutionPanelProps) {
  const fetchSubjectExecution = useLifecycleStore((s) => s.fetchSubjectExecution);
  const view = useLifecycleStore((s) => s.subjectExecutions.get(subjectExecutionKey("story", story.id)) ?? null);

  useEffect(() => {
    void fetchSubjectExecution("story", story.id);
  }, [fetchSubjectExecution, story.id, story.updated_at]);

  return (
    <div className="h-full overflow-y-auto bg-background">
      <SubjectExecutionContent view={view} />
    </div>
  );
}
