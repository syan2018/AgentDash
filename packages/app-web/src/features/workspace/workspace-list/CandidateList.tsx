import type { BackendConfig, WorkspaceInventoryCandidate } from "../../../types";
import {
  backendDisplayName,
  identitySummary,
} from "../model/workspaceRouting";
import { IDENTITY_KIND_LABELS } from "../model/workspaceTerms";
import { candidateKey } from "./editorHelpers";

interface CandidateListProps {
  candidates: WorkspaceInventoryCandidate[];
  backends: BackendConfig[];
  selectedKey?: string | null;
  emptyText?: string;
  onSelect?: (candidate: WorkspaceInventoryCandidate) => void;
  onAddBinding?: (candidate: WorkspaceInventoryCandidate) => void;
}

export function CandidateList({
  candidates,
  backends,
  selectedKey,
  emptyText = "暂无可选目录。可以浏览本机目录添加新的可选目录。",
  onSelect,
  onAddBinding,
}: CandidateListProps) {
  if (candidates.length === 0) {
    return (
      <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
        {emptyText}
      </p>
    );
  }

  return (
    <div className="space-y-2">
      {candidates.map((candidate) => {
        const key = candidateKey(candidate);
        const active = selectedKey === key;
        return (
          <div
            key={key}
            className={`rounded-[8px] border px-3 py-3 ${
              active ? "border-primary/30 bg-primary/[0.04]" : "border-border bg-background"
            }`}
          >
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="rounded-[8px] border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                    {IDENTITY_KIND_LABELS[candidate.identity_kind]}
                  </span>
                  <span className="truncate font-mono text-xs text-foreground">{candidate.root_ref}</span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                  {backendDisplayName(backends, candidate.backend_id)} · {candidate.reason}
                </p>
                <p className="mt-1 truncate text-xs text-muted-foreground">
                  {identitySummary(candidate.identity_kind, candidate.identity_payload)}
                </p>
              </div>
              <div className="flex shrink-0 gap-2">
                {onSelect && (
                  <button
                    type="button"
                    onClick={() => onSelect(candidate)}
                    className="rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs text-muted-foreground hover:bg-secondary"
                  >
                    {active ? "已选择" : "选择"}
                  </button>
                )}
                {onAddBinding && (
                  <button
                    type="button"
                    onClick={() => onAddBinding(candidate)}
                    className="agentdash-button-secondary text-xs"
                  >
                    添加运行位置
                  </button>
                )}
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
