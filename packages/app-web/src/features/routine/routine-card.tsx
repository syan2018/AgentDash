import type { Routine, RoutineTriggerType, ProjectAgent } from "../../types";
import { DetailMenu } from "@agentdash/ui";
import { formatRelativeTime } from "../../lib/format";

const TRIGGER_TYPE_BADGE: Record<RoutineTriggerType, { label: string; className: string }> = {
  scheduled: { label: "定时", className: "border-info/30 bg-info/10 text-info" },
  webhook: { label: "Webhook", className: "border-success/30 bg-success/10 text-success" },
  plugin: { label: "Plugin", className: "border-primary/30 bg-primary/10 text-primary" },
};

export function RoutineCard({
  routine,
  projectAgents,
  onEdit,
  onToggleEnable,
  onViewHistory,
  onDelete,
}: {
  routine: Routine;
  projectAgents: ProjectAgent[];
  onEdit: () => void;
  onToggleEnable: () => void;
  onViewHistory: () => void;
  onDelete: () => void;
}) {
  const badge = TRIGGER_TYPE_BADGE[routine.trigger_config.type];
  const projectAgent = projectAgents.find((agent) => agent.id === routine.project_agent_id);
  const agentName = projectAgent?.name || routine.project_agent_id;

  const triggerDetail = (() => {
    switch (routine.trigger_config.type) {
      case "scheduled":
        return routine.trigger_config.cron_expression ?? "";
      case "webhook":
        return routine.trigger_config.endpoint_id ?? "";
      case "plugin":
        return routine.trigger_config.provider_key ?? "";
    }
  })();

  return (
    <article className="group rounded-[12px] border border-border bg-background/75 p-4 transition-colors hover:bg-background">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className={`inline-block h-2 w-2 shrink-0 rounded-full ${routine.enabled ? "bg-success" : "bg-muted-foreground/30"}`} />
            <h3 className="truncate text-sm font-medium text-foreground">{routine.name}</h3>
          </div>

          <div className="mt-2 flex flex-wrap items-center gap-1.5">
            <span className={`inline-block rounded-[6px] border px-2 py-0.5 text-[10px] ${badge.className}`}>
              {badge.label}
            </span>
            {triggerDetail && (
              <span className="rounded-[6px] border border-border bg-secondary/50 px-2 py-0.5 font-mono text-[10px] text-muted-foreground">
                {triggerDetail}
              </span>
            )}
            <span className="rounded-[6px] border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
              {agentName}
            </span>
          </div>

          <div className="mt-2 flex items-center gap-3 text-[11px] text-muted-foreground">
            <span>最近触发: {formatRelativeTime(routine.last_fired_at, { emptyLabel: "从未触发" })}</span>
            <span>·</span>
            <span>{routine.enabled ? "已启用" : "已禁用"}</span>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          <button
            type="button"
            onClick={onToggleEnable}
            className={`rounded-[8px] border px-2.5 py-1 text-[11px] transition-colors ${
              routine.enabled
                ? "border-success/30 bg-success/10 text-success hover:bg-success/20"
                : "border-border bg-secondary text-muted-foreground hover:bg-secondary/80"
            }`}
          >
            {routine.enabled ? "启用中" : "已禁用"}
          </button>
          <DetailMenu
            items={[
              { key: "edit", label: "编辑", onSelect: onEdit },
              { key: "history", label: "执行历史", onSelect: onViewHistory },
              { key: "delete", label: "删除", onSelect: onDelete, danger: true },
            ]}
          />
        </div>
      </div>
    </article>
  );
}
