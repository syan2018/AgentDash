import { Link } from "react-router-dom";
import type { AgentRunRuntimeTarget } from "../../../../services/agentRunRuntime";
import type {
  CompanionSubagentDispatchPresentation,
  CompanionSubagentDispatchStatus,
  CompanionSubagentKnownAgentRef,
} from "../../model/companionSubagentDispatch";
import {
  resolveCompanionSubagentKnownRef,
  resolveCompanionSubagentOpenTarget,
} from "../../model/companionSubagentDispatch";
import { normalizeAgentRunDeliveryStatus } from "../../../agent/agent-run-delivery-status";
import { useDebugPrefs } from "../../../../hooks/use-debug-prefs";
import { CB } from "./cardBodyTokens";
import { JsonTree } from "./JsonTree";

export interface CompanionSubagentDispatchCardBodyProps {
  presentation: CompanionSubagentDispatchPresentation;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  companionSubagents?: readonly CompanionSubagentKnownAgentRef[];
}

export function CompanionSubagentDispatchCardBody({
  presentation,
  agentRunTarget,
  companionSubagents,
}: CompanionSubagentDispatchCardBodyProps) {
  const { prefs } = useDebugPrefs();
  const projectedRef = resolveCompanionSubagentKnownRef(presentation, companionSubagents);
  const openTarget = resolveCompanionSubagentOpenTarget(presentation, {
    currentRunId: agentRunTarget?.runId,
    knownAgentRefs: companionSubagents,
  });
  const projectedStatus = projectedRef?.delivery_status
    ? statusFromDeliveryStatus(projectedRef.delivery_status)
    : null;
  const effectiveStatus = projectedStatus ?? presentation.status;
  const status = statusLabel(effectiveStatus);
  const projectedTitle = projectedRef?.display_title?.trim();
  const title = projectedTitle || presentation.title;
  const resultSummary = presentation.resultSummary;
  const hasRawProtocolRefs = Object.keys(presentation.rawProtocolRefs).length > 0;

  return (
    <div className={`${CB.sectionGap} text-xs`}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-foreground">{title}</p>
          <p className="text-[10px] text-muted-foreground/60">Companion 子 Agent</p>
        </div>
        <span className={`rounded-[4px] bg-secondary/40 px-1.5 py-0.5 text-[10px] font-medium ${status.className}`}>
          {status.label}
        </span>
      </div>

      {presentation.summary && (
        <div>
          <p className={`mb-0.5 ${CB.sectionTitle}`}>摘要</p>
          <p className="whitespace-pre-wrap text-foreground/80">{presentation.summary}</p>
        </div>
      )}

      {resultSummary && (
        <div>
          <p className={`mb-0.5 ${CB.sectionTitle}`}>结果摘要</p>
          <p className="whitespace-pre-wrap text-foreground/80">{resultSummary}</p>
        </div>
      )}

      {presentation.resultDetails != null && (
        <details className="rounded-[6px] bg-secondary/10 px-2 py-1.5">
          <summary className="cursor-pointer text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground/60">
            结果详情
          </summary>
          <div className="mt-1 max-h-48 overflow-auto">
            <JsonTree data={presentation.resultDetails} defaultDepth={1} />
          </div>
        </details>
      )}

      <div className="flex flex-wrap items-center gap-2">
        {openTarget.enabled ? (
          <Link
            className="inline-flex min-h-8 shrink-0 items-center justify-center rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-foreground transition-colors hover:border-foreground/30 hover:bg-secondary"
            to={openTarget.path}
          >
            查看子 Agent
          </Link>
        ) : (
          <button
            className="inline-flex min-h-8 shrink-0 items-center justify-center rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-muted-foreground opacity-60"
            disabled
            type="button"
          >
            {openTarget.reason === "等待 child agent id" ? "等待子 Agent" : openTarget.reason}
          </button>
        )}
      </div>

      {prefs.hookVerbose && hasRawProtocolRefs && (
        <div>
          <p className={`mb-1 ${CB.sectionTitle}`}>Raw protocol</p>
          <pre className={`${CB.codeBlock} max-h-48 overflow-auto whitespace-pre-wrap break-words`}>
            {JSON.stringify(presentation.rawProtocolRefs, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

function statusFromDeliveryStatus(status: string): CompanionSubagentDispatchStatus {
  const normalized = normalizeAgentRunDeliveryStatus(status);
  switch (normalized) {
    case "idle":
    case "suspended":
      return "pending";
    case "running":
    case "cancelling":
      return "running";
    case "completed":
      return "completed";
    case "failed":
    case "lost":
      return "failed";
    case "interrupted":
      return "interrupted";
  }
}

function statusLabel(status: CompanionSubagentDispatchPresentation["status"]): {
  label: string;
  className: string;
} {
  switch (status) {
    case "pending":
      return { label: "等待中", className: CB.statusNeutral };
    case "running":
      return { label: "运行中", className: CB.statusWarning };
    case "completed":
      return { label: "已完成", className: CB.statusSuccess };
    case "failed":
      return { label: "失败", className: CB.statusFailed };
    case "interrupted":
      return { label: "已中断", className: CB.statusWarning };
    case "timed_out":
      return { label: "等待超时", className: CB.statusWarning };
    default:
      return { label: "未知", className: CB.statusNeutral };
  }
}
