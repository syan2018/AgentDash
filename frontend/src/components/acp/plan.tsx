import type { PlanEntry } from "../../types";

export function PlanView({ entries }: { entries: PlanEntry[] }) {
  if (entries.length === 0) return null;

  return (
    <div className="rounded-md border border-border bg-card p-3">
      <p className="mb-2 text-xs font-medium text-muted-foreground">执行计划</p>
      <ul className="space-y-1.5 text-sm">
        {entries.map((entry, index) => (
          <li key={index} className="flex items-start gap-2">
            <span className="mt-0.5 text-xs text-muted-foreground">
              {entry.status === "completed" ? "✓" : entry.status === "in_progress" ? "…" : "○"}
            </span>
            <span className={entry.status === "completed" ? "text-muted-foreground line-through" : "text-foreground"}>
              {entry.content}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
