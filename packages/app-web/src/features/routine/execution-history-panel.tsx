import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { RoutineExecutionStatus } from "../../types";
import { useRoutineStore } from "../../stores/routineStore";

const EXEC_STATUS_STYLE: Record<RoutineExecutionStatus, string> = {
  pending: "border-border bg-secondary/50 text-muted-foreground",
  running: "border-info/30 bg-info/10 text-info",
  completed: "border-success/30 bg-success/10 text-success",
  failed: "border-destructive/30 bg-destructive/10 text-destructive",
  skipped: "border-warning/30 bg-warning/10 text-warning",
};

export function ExecutionHistoryContent({ routineId }: { routineId: string }) {
  const navigate = useNavigate();
  const { executionsByRoutineId, fetchExecutions } = useRoutineStore();
  const executions = executionsByRoutineId[routineId] ?? [];
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setLoading(true);
    void fetchExecutions(routineId, 20, 0).finally(() => setLoading(false));
  }, [routineId, fetchExecutions]);

  const loadMore = () => {
    void fetchExecutions(routineId, 20, executions.length);
  };

  if (loading && executions.length === 0) {
    return <p className="py-8 text-center text-sm text-muted-foreground">加载中...</p>;
  }

  if (executions.length === 0) {
    return <p className="py-8 text-center text-sm text-muted-foreground">暂无执行记录</p>;
  }

  return (
    <div className="space-y-2 p-4">
      {executions.map((exec) => (
        <div key={exec.id} className="rounded-[8px] border border-border bg-background/75 p-3">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className={`inline-block rounded-[6px] border px-2 py-0.5 text-[10px] ${EXEC_STATUS_STYLE[exec.status]}`}>
                {exec.status}
              </span>
              <span className="text-xs text-muted-foreground">{exec.trigger_source}</span>
            </div>
            <span className="text-[10px] text-muted-foreground">
              {new Date(exec.started_at).toLocaleString()}
            </span>
          </div>
          {exec.error && (
            <p className="mt-2 rounded-[6px] bg-destructive/5 px-2 py-1 text-xs text-destructive">{exec.error}</p>
          )}
          {exec.session_id && (
            <button
              type="button"
              onClick={() => navigate(`/session/${exec.session_id}`)}
              className="mt-2 text-xs text-primary underline hover:no-underline"
            >
              查看 Session
            </button>
          )}
        </div>
      ))}
      <button
        type="button"
        onClick={loadMore}
        className="w-full rounded-[8px] border border-border py-2 text-xs text-muted-foreground transition-colors hover:bg-secondary"
      >
        加载更多
      </button>
    </div>
  );
}
