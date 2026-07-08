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
  const hasRawProtocolRefs = Object.keys(presentation.rawProtocolRefs).length > 0;

  return (
    <div className={`${CB.sectionGap} text-xs`}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-foreground">{title}</p>
          <p className="text-[10px] text-muted-foreground/60">AgentDash subagent dispatch</p>
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

      {presentation.resultPreview && (
        <div>
          <p className={`mb-0.5 ${CB.sectionTitle}`}>结果</p>
          <p className="whitespace-pre-wrap text-foreground/80">{presentation.resultPreview}</p>
        </div>
      )}

      <div className="grid gap-2 sm:grid-cols-2">
        <InfoBlock label="Child agent" value={presentation.childAgentId ?? "等待解析"} mono />
        <InfoBlock label="Journal" value={presentation.journalUri ?? "等待 journal ref"} mono />
        {projectedRef?.last_activity_at && (
          <InfoBlock label="Last activity" value={projectedRef.last_activity_at} mono />
        )}
        {presentation.frameId && <InfoBlock label="Frame" value={presentation.frameId} mono />}
        {presentation.gateId && <InfoBlock label="Gate" value={presentation.gateId} mono />}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        {openTarget.enabled ? (
          <a
            className="inline-flex min-h-8 shrink-0 items-center justify-center rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-foreground transition-colors hover:border-foreground/30 hover:bg-secondary"
            href={openTarget.path}
          >
            打开 child workspace
          </a>
        ) : (
          <button
            className="inline-flex min-h-8 shrink-0 items-center justify-center rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-muted-foreground opacity-60"
            disabled
            type="button"
          >
            {openTarget.reason}
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

function InfoBlock({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="min-w-0">
      <p className={CB.sectionTitle}>{label}</p>
      <p className={`truncate text-foreground/80 ${mono ? "font-mono" : ""}`} title={value}>
        {value}
      </p>
    </div>
  );
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
