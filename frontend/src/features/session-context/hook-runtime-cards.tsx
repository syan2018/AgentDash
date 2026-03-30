import { useState, type ReactNode } from "react";
import type {
  ActiveWorkflowHookMetadata,
  HookInjection,
  HookSessionRuntimeInfo,
  HookTraceEntry,
} from "../../types";
import { SurfaceCard } from "./surface-card";

// ─── Hook Runtime Surface Card ─────────────────────────

export function HookRuntimeSurfaceCard({
  hookRuntime,
}: {
  hookRuntime: HookSessionRuntimeInfo;
}) {
  const { snapshot } = hookRuntime;
  const activeWorkflow = snapshot.metadata?.active_workflow ?? null;
  const unresolvedActions = hookRuntime.pending_actions.filter(
    (action) => action.status === "pending",
  );
  const resolvedActions = hookRuntime.pending_actions.filter(
    (action) => action.status === "resolved",
  );
  return (
    <SurfaceCard eyebrow="运行中 Hook Runtime" title={`revision ${hookRuntime.revision}`}>
      <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          owners: {snapshot.owners.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          sources: {snapshot.sources.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          injections: {snapshot.injections.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          diagnostics: {hookRuntime.diagnostics.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          trace: {hookRuntime.trace.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          actions: {hookRuntime.pending_actions.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          open: {unresolvedActions.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1">
          resolved: {resolvedActions.length}
        </span>
      </div>
      {snapshot.tags.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-2">
          {snapshot.tags.map((tag) => (
            <span
              key={tag}
              className="rounded-full border border-border bg-background px-2 py-1 text-[10px] text-muted-foreground"
            >
              {tag}
            </span>
          ))}
        </div>
      )}
      {activeWorkflow && <HookRuntimeWorkflowMetaCard metadata={activeWorkflow} />}
      {snapshot.sources.length > 0 && (
        <div className="mt-3 rounded-[10px] border border-border bg-background/70 px-3 py-2">
          <p className="text-xs font-medium text-foreground">Hook 来源注册表</p>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {snapshot.sources.map((source) => (
              <span
                key={source}
                className="rounded-full border border-border bg-background px-2 py-1 text-[10px] text-muted-foreground"
              >
                {source}
              </span>
            ))}
          </div>
        </div>
      )}
      {snapshot.injections.length > 0 && (
        <div className="mt-3 space-y-1.5">
          <p className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground/60">
            注入项（{snapshot.injections.length} 条）
          </p>
          {snapshot.injections.map((injection, index) => (
            <HookInjectionRow key={`${injection.slot}-${injection.source}-${index}`} injection={injection} />
          ))}
        </div>
      )}
      <p className="mt-2 text-[11px] leading-5 text-muted-foreground">
        这里显示的是执行层真实加载并参与 loop 的 session 级 hook snapshot，而不是 owner 级静态上下文推导。
      </p>
    </SurfaceCard>
  );
}

// ─── Hook Runtime Sub-components ───────────────────────

function HookRuntimeWorkflowMetaCard({
  metadata,
}: {
  metadata: ActiveWorkflowHookMetadata;
}) {
  return (
    <div className="mt-3 rounded-[10px] border border-border bg-background/70 px-3 py-2">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-medium text-foreground">
          {metadata.lifecycle_name} / {metadata.step_title}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
          run: {metadata.run_status}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
          step: {metadata.step_key}
        </span>
      </div>
      <div className="mt-2 flex flex-wrap gap-2 text-[10px] text-muted-foreground">
        <span className="rounded-full border border-border bg-background px-2 py-1">
          lifecycle_id: {metadata.lifecycle_id}
        </span>
        <span className="rounded-full border border-border bg-background px-2 py-1">
          run_id: {metadata.run_id}
        </span>
        <span className="rounded-full border border-border bg-background px-2 py-1">
          workflow: {metadata.workflow_key ?? metadata.primary_workflow_key ?? "—"}
        </span>
      </div>
    </div>
  );
}

function HookInjectionRow({ injection }: { injection: HookInjection }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded-[10px] border border-border bg-background/70 overflow-hidden">
      <button
        type="button"
        onClick={() => injection.content && setOpen((v) => !v)}
        className={`flex w-full items-center gap-2.5 px-3 py-2 text-left transition-colors ${injection.content ? "hover:bg-secondary/35 cursor-pointer" : "cursor-default"}`}
      >
        <span className="inline-flex rounded-[4px] border border-border bg-secondary/60 px-1.5 py-0 text-[9px] font-mono text-muted-foreground/70 shrink-0">
          {injection.slot}
        </span>
        <span className="min-w-0 flex-1 truncate text-xs text-foreground/85">{injection.source}</span>
        {injection.content && (
          <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
        )}
      </button>
      {open && injection.content && (
        <div className="border-t border-border/50 px-3 py-2.5">
          <pre className="max-h-56 overflow-auto whitespace-pre-wrap text-[11px] leading-relaxed text-foreground/75">
            {injection.content}
          </pre>
        </div>
      )}
    </div>
  );
}

// ─── Hook Runtime Diagnostics ──────────────────────────

export function HookRuntimeDiagnosticsCard({
  hookRuntime,
}: {
  hookRuntime: HookSessionRuntimeInfo;
}) {
  return (
    <SurfaceCard eyebrow="Hook 诊断" title="运行时命中记录">
      {hookRuntime.diagnostics.length > 0 ? (
        <div className="space-y-2">
          {hookRuntime.diagnostics.map((entry, index) => (
            <div
              key={`${entry.code}-${index}`}
              className="rounded-[10px] border border-border bg-background/70 px-3 py-2"
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
                  {entry.code}
                </span>
                <span className="text-xs text-foreground/85">{entry.message}</span>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">当前还没有记录到额外的 Hook 诊断。</p>
      )}
    </SurfaceCard>
  );
}

// ─── Hook Runtime Trace ────────────────────────────────

export function HookRuntimeTraceCard({
  hookRuntime,
}: {
  hookRuntime: HookSessionRuntimeInfo;
}) {
  return (
    <SurfaceCard eyebrow="Hook Trace" title="最近触发记录">
      {hookRuntime.trace.length > 0 ? (
        <div className="space-y-2">
          {hookRuntime.trace
            .slice()
            .reverse()
            .map((entry) => (
              <HookTraceEntryCard key={`${entry.sequence}-${entry.revision}`} entry={entry} />
            ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">当前还没有记录到 Hook trigger trace。</p>
      )}
    </SurfaceCard>
  );
}

// ─── Hook Runtime Pending Actions ──────────────────────

export function HookRuntimePendingActionsCard({
  hookRuntime,
}: {
  hookRuntime: HookSessionRuntimeInfo;
}) {
  return (
    <SurfaceCard eyebrow="Hook Actions" title="干预项状态">
      {hookRuntime.pending_actions.length > 0 ? (
        <div className="space-y-2">
          {hookRuntime.pending_actions.map((action) => {
            const createdAt = Number.isFinite(action.created_at_ms)
              ? new Date(action.created_at_ms).toLocaleTimeString("zh-CN", {
                hour12: false,
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
              })
              : "-";
            const resolvedAt = typeof action.resolved_at_ms === "number" && Number.isFinite(action.resolved_at_ms)
              ? new Date(action.resolved_at_ms).toLocaleTimeString("zh-CN", {
                hour12: false,
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
              })
              : null;
            return (
              <div key={action.id} className="rounded-[10px] border border-border bg-background/70 px-3 py-2">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
                    {action.action_type}
                  </span>
                  <span className="rounded-full border border-border bg-background px-2 py-1 text-[10px] text-muted-foreground">
                    {action.status}
                  </span>
                  <span className="text-xs font-medium text-foreground/90">{action.title}</span>
                  <span className="text-[11px] text-muted-foreground">{createdAt}</span>
                </div>
                <p className="mt-2 text-[11px] leading-5 text-muted-foreground">{action.summary}</p>
                <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
                  <span>action: {action.id}</span>
                  {action.turn_id && <span>turn: {action.turn_id}</span>}
                  <span>trigger: {action.source_trigger}</span>
                  <span>injections: {action.injections.length}</span>
                  {action.last_injected_at_ms != null && <span>last_injected: 已注入</span>}
                  {action.resolution_kind && <span>resolution: {action.resolution_kind}</span>}
                  {action.resolution_turn_id && <span>resolution_turn: {action.resolution_turn_id}</span>}
                  {resolvedAt && <span>resolved_at: {resolvedAt}</span>}
                </div>
                {action.resolution_note && (
                  <p className="mt-2 text-[11px] leading-5 text-foreground/80">
                    结案说明：{action.resolution_note}
                  </p>
                )}
                {action.injections.length > 0 && (
                  <div className="mt-2 space-y-1">
                    {action.injections.map((injection, index) => (
                      <p key={`${action.id}-${injection.slot}-${index}`} className="text-[11px] leading-5 text-foreground/80">
                        [{injection.slot}] {injection.content}
                      </p>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">当前还没有记录到 hook 干预项。</p>
      )}
    </SurfaceCard>
  );
}

// ─── Trace Entry ───────────────────────────────────────

function HookTraceEntryCard({ entry }: { entry: HookTraceEntry }) {
  const timestamp = Number.isFinite(entry.timestamp_ms)
    ? new Date(entry.timestamp_ms).toLocaleTimeString("zh-CN", {
      hour12: false,
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })
    : "-";
  const completionStatus = entry.completion
    ? entry.completion.advanced
      ? "已推进"
      : entry.completion.satisfied
        ? "已满足"
        : "未满足"
    : null;

  return (
    <div className="rounded-[10px] border border-border bg-background/70 px-3 py-2">
      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
          #{entry.sequence}
        </span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[10px] text-muted-foreground">
          {entry.trigger}
        </span>
        <span className="text-xs font-medium text-foreground/90">{entry.decision}</span>
        <span className="text-[11px] text-muted-foreground">
          rev {entry.revision} · {timestamp}
        </span>
      </div>
      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
        {entry.tool_name && <span>tool: {entry.tool_name}</span>}
        {entry.tool_call_id && <span>call: {entry.tool_call_id}</span>}
        {entry.subagent_type && <span>subagent: {entry.subagent_type}</span>}
        {entry.refresh_snapshot && <span>已刷新 snapshot</span>}
      </div>
      {entry.completion && completionStatus && (
        <p className="mt-2 text-[11px] leading-5 text-muted-foreground">
          completion: {entry.completion.mode} · {completionStatus} · {entry.completion.reason}
        </p>
      )}
      {entry.block_reason && (
        <p className="mt-2 text-[11px] leading-5 text-destructive">{entry.block_reason}</p>
      )}
      {entry.matched_rule_keys.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {entry.matched_rule_keys.map((ruleKey) => (
            <span
              key={ruleKey}
              className="rounded-full border border-border bg-secondary/40 px-2 py-1 text-[10px] text-muted-foreground"
            >
              {ruleKey}
            </span>
          ))}
        </div>
      )}
      {entry.diagnostics.length > 0 && (
        <div className="mt-2 space-y-1">
          {entry.diagnostics.map((diagnostic, index) => (
            <div key={`${diagnostic.code}-${index}`} className="rounded-[8px] border border-border/70 bg-background/60 px-2 py-1.5">
              <p className="text-[11px] leading-5 text-muted-foreground">
                {diagnostic.code}: {diagnostic.message}
              </p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── Shared: Raw Diagnostics Wrapper ───────────────────

export function RawDiagnosticsSection({ children }: { children: ReactNode }) {
  return (
    <details className="rounded-[12px] border border-dashed border-border bg-background/60 px-3 py-2">
      <summary className="cursor-pointer text-xs font-medium text-muted-foreground">
        查看原始结构化诊断信息
      </summary>
      <div className="mt-3 space-y-3">
        {children}
      </div>
    </details>
  );
}
