import type { WorkspaceBindingStatus, WorkspaceStatus } from "../../../types";
import {
  BINDING_STATUS_LABELS,
  RESOLUTION_STATE_LABELS,
  WORKSPACE_STATUS_LABELS,
} from "../model/workspaceTerms";

const statusClassNames: Record<WorkspaceStatus, string> = {
  pending: "border-border bg-secondary text-muted-foreground",
  ready: "border-success/20 bg-success/10 text-success",
  active: "border-primary/20 bg-primary/10 text-primary",
  archived: "border-border bg-secondary text-muted-foreground",
  error: "border-destructive/20 bg-destructive/10 text-destructive",
};

export function WorkspaceStatusBadge({ status }: { status: WorkspaceStatus }) {
  return (
    <span className={`inline-flex items-center rounded-full border px-2.5 py-1 text-[10px] font-medium ${statusClassNames[status]}`}>
      {WORKSPACE_STATUS_LABELS[status]}
    </span>
  );
}

export function BindingStatusBadge({ status }: { status: WorkspaceBindingStatus }) {
  const tone = status === "ready"
    ? "bg-success"
    : status === "offline"
      ? "bg-muted-foreground/45"
      : "bg-destructive";
  return (
    <span className="inline-flex w-fit items-center gap-1.5 self-start whitespace-nowrap text-[11px] text-muted-foreground">
      <span className={`h-1.5 w-1.5 rounded-full ${tone}`} />
      {BINDING_STATUS_LABELS[status]}
    </span>
  );
}

export function ResolutionBadge({ state }: { state: "resolved" | "warning" | "blocked" }) {
  const cls = state === "resolved"
    ? "border-success/20 bg-success/10 text-success"
    : state === "warning"
      ? "border-warning/25 bg-warning/10 text-warning"
      : "border-destructive/25 bg-destructive/10 text-destructive";
  return (
    <span className={`inline-flex rounded-full border px-2 py-0.5 text-[10px] font-medium ${cls}`}>
      {RESOLUTION_STATE_LABELS[state]}
    </span>
  );
}
