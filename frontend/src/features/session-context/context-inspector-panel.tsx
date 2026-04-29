/**
 * Context Inspector Panel
 *
 * 展示 session 的 `ContextFragment` 审计时间线：按 bundle_id 分组 → trigger 分小节 →
 * 每个 fragment 一行。首版支持 scope / slot / source_prefix 过滤，只读，不提供编辑/
 * 禁用按钮（PRD D5 决策）。
 *
 * 数据来自 `/sessions/{id}/context/audit`，3 秒轮询刷新。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  FRAGMENT_SCOPE_TAGS,
  fetchContextAudit,
  type ContextAuditEvent,
  type FragmentScopeTag,
} from "../../services/contextAudit";
import { SurfaceCard } from "./surface-card";

const POLL_INTERVAL_MS = 3000;

const SCOPE_LABEL: Record<FragmentScopeTag, string> = {
  runtime_agent: "Runtime Agent",
  title_gen: "Title Gen",
  summarizer: "Summarizer",
  bridge_replay: "Bridge Replay",
  audit: "Audit",
};

const SCOPE_BADGE_CLASS: Record<FragmentScopeTag, string> = {
  runtime_agent:
    "bg-blue-500/10 text-blue-600 border-blue-500/30 dark:text-blue-300",
  title_gen:
    "bg-purple-500/10 text-purple-600 border-purple-500/30 dark:text-purple-300",
  summarizer:
    "bg-amber-500/10 text-amber-700 border-amber-500/30 dark:text-amber-300",
  bridge_replay:
    "bg-green-500/10 text-green-700 border-green-500/30 dark:text-green-300",
  audit:
    "bg-muted text-muted-foreground border-border",
};

function formatTimestamp(at_ms: number): string {
  try {
    return new Date(at_ms).toLocaleTimeString();
  } catch {
    return String(at_ms);
  }
}

function describeTrigger(trigger: string): string {
  switch (trigger) {
    case "session_bootstrap":
      return "Session Bootstrap";
    case "composer_rebuild":
      return "Composer Rebuild";
    case "session_plan":
      return "Session Plan";
    case "capability":
      return "Capability";
    default:
      if (trigger.startsWith("hook:")) return `Hook · ${trigger.slice(5)}`;
      if (trigger.startsWith("filter:")) return `Filter · ${trigger.slice(7)}`;
      return trigger;
  }
}

interface ContextInspectorPanelProps {
  sessionId: string;
}

/**
 * 右侧抽屉中显示 Context Inspector 的单页面板。
 *
 * 调用方（SessionPage / Context Panel 等）负责决定何时挂载；挂载后自动开始轮询。
 */
export function ContextInspectorPanel({ sessionId }: ContextInspectorPanelProps) {
  const [events, setEvents] = useState<ContextAuditEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [scopeFilter, setScopeFilter] = useState<FragmentScopeTag | "">("");
  const [slotFilter, setSlotFilter] = useState("");
  const [sourcePrefix, setSourcePrefix] = useState("");
  const [expandedEventIds, setExpandedEventIds] = useState<Set<string>>(new Set());

  const loadEvents = useCallback(async () => {
    try {
      const list = await fetchContextAudit(sessionId, {
        scope: scopeFilter || undefined,
        slot: slotFilter.trim() || undefined,
        source_prefix: sourcePrefix.trim() || undefined,
      });
      setEvents(list);
      setError(null);
    } catch (err) {
      setError((err as Error).message || "加载失败");
    }
  }, [sessionId, scopeFilter, slotFilter, sourcePrefix]);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      if (cancelled) return;
      await loadEvents();
    };
    void tick();
    const handle = window.setInterval(tick, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(handle);
    };
  }, [loadEvents]);

  const grouped = useMemo(() => groupEventsByBundleAndTrigger(events), [events]);

  const toggleExpanded = useCallback((eventId: string) => {
    setExpandedEventIds((prev) => {
      const next = new Set(prev);
      if (next.has(eventId)) {
        next.delete(eventId);
      } else {
        next.add(eventId);
      }
      return next;
    });
  }, []);

  return (
    <div className="flex h-full flex-col gap-3 overflow-y-auto p-3">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <label className="flex items-center gap-1">
          <span className="text-muted-foreground">Scope</span>
          <select
            value={scopeFilter}
            onChange={(e) => setScopeFilter(e.target.value as FragmentScopeTag | "")}
            className="rounded-md border border-border bg-background px-2 py-1"
          >
            <option value="">全部</option>
            {FRAGMENT_SCOPE_TAGS.map((tag) => (
              <option key={tag} value={tag}>
                {SCOPE_LABEL[tag]}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-1">
          <span className="text-muted-foreground">Slot</span>
          <input
            value={slotFilter}
            onChange={(e) => setSlotFilter(e.target.value)}
            placeholder="例如 task / workflow"
            className="w-32 rounded-md border border-border bg-background px-2 py-1"
          />
        </label>
        <label className="flex items-center gap-1">
          <span className="text-muted-foreground">Source prefix</span>
          <input
            value={sourcePrefix}
            onChange={(e) => setSourcePrefix(e.target.value)}
            placeholder="例如 legacy:session_plan / hook:"
            className="w-48 rounded-md border border-border bg-background px-2 py-1"
          />
        </label>
        <span className="ml-auto text-[11px] text-muted-foreground">
          {events.length} 条记录（每 {POLL_INTERVAL_MS / 1000}s 刷新）
        </span>
      </div>

      {error && (
        <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          {error}
        </div>
      )}

      {grouped.length === 0 && !error && (
        <div className="flex flex-1 items-center justify-center rounded-md border border-dashed border-border bg-background/50 px-4 py-12 text-center text-xs text-muted-foreground">
          暂无审计事件。首次 Bundle 产出后会在此显示。
        </div>
      )}

      {grouped.map((group) => (
        <SurfaceCard
          key={group.bundleId}
          eyebrow={`Bundle · ${group.bundleId.slice(0, 8)}`}
          title={`${group.events.length} 条 fragment · ${formatTimestamp(group.atMs)}`}
        >
          <div className="flex flex-col gap-3">
            {group.sections.map((section) => (
              <div key={section.trigger} className="flex flex-col gap-1">
                <div className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                  {describeTrigger(section.trigger)}
                </div>
                <ul className="flex flex-col gap-1">
                  {section.events.map((event) => {
                    const expanded = expandedEventIds.has(event.event_id);
                    return (
                      <li
                        key={event.event_id}
                        className="rounded-md border border-border bg-background/70 px-3 py-2"
                      >
                        <button
                          type="button"
                          onClick={() => toggleExpanded(event.event_id)}
                          className="flex w-full flex-wrap items-center gap-2 text-left text-xs"
                        >
                          <span className="font-mono text-[11px] text-muted-foreground">
                            #{event.order}
                          </span>
                          <span className="rounded-md bg-secondary px-1.5 py-0.5 font-mono text-[11px] text-foreground">
                            {event.slot}
                          </span>
                          {event.scope.map((tag) => (
                            <span
                              key={tag}
                              className={`rounded-md border px-1.5 py-0.5 text-[10px] ${SCOPE_BADGE_CLASS[tag]}`}
                            >
                              {SCOPE_LABEL[tag]}
                            </span>
                          ))}
                          <span className="truncate text-[11px] text-muted-foreground">
                            {event.source}
                          </span>
                          <span className="ml-auto text-[10px] text-muted-foreground">
                            {expanded ? "收起" : "展开"}
                          </span>
                        </button>
                        {expanded && (
                          <div className="mt-2 flex flex-col gap-1 text-xs">
                            <div className="text-[11px] text-muted-foreground">
                              {event.label} · hash=
                              {event.content_hash.toString(16).slice(0, 12)}
                              {event.full_content_available && " · 已截断 (2KB)"}
                            </div>
                            <pre className="max-h-[320px] overflow-auto rounded-md bg-muted/50 px-2 py-1 text-[11px] leading-relaxed">
                              {event.content_preview || "（内容为空）"}
                            </pre>
                          </div>
                        )}
                      </li>
                    );
                  })}
                </ul>
              </div>
            ))}
          </div>
        </SurfaceCard>
      ))}
    </div>
  );
}

interface BundleGroupSection {
  trigger: string;
  events: ContextAuditEvent[];
}

interface BundleGroup {
  bundleId: string;
  atMs: number;
  events: ContextAuditEvent[];
  sections: BundleGroupSection[];
}

function groupEventsByBundleAndTrigger(events: ContextAuditEvent[]): BundleGroup[] {
  const byBundle = new Map<
    string,
    {
      atMs: number;
      events: ContextAuditEvent[];
      sections: Map<string, ContextAuditEvent[]>;
    }
  >();

  for (const event of events) {
    let entry = byBundle.get(event.bundle_id);
    if (!entry) {
      entry = { atMs: event.at_ms, events: [], sections: new Map() };
      byBundle.set(event.bundle_id, entry);
    }
    entry.atMs = Math.min(entry.atMs, event.at_ms);
    entry.events.push(event);
    const section = entry.sections.get(event.trigger) ?? [];
    section.push(event);
    entry.sections.set(event.trigger, section);
  }

  return Array.from(byBundle.entries())
    .map(([bundleId, value]) => ({
      bundleId,
      atMs: value.atMs,
      events: value.events,
      sections: Array.from(value.sections.entries()).map(([trigger, list]) => ({
        trigger,
        events: list.slice().sort((a, b) => a.order - b.order),
      })),
    }))
    .sort((a, b) => a.atMs - b.atMs);
}
