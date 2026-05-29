import { useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useMatch, useNavigate } from "react-router-dom";
import { SessionStatusDot } from "../ui/session-status-dot";
import type { ProjectSessionEntry } from "../../types";
import {
  buildSessionShortcutRows,
  type SessionShortcutRow,
} from "./session-shortcut-rows";
import { sessionParentRelationLabel } from "../../features/agent/session-relations";

// ─── Session 快捷列表（容器高度自适应 + 末尾 ...） ──────────

function isUuidLike(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

function getShortcutAgentLabel(session: ProjectSessionEntry): string | null {
  const displayName = session.agent_display_name?.trim();
  if (displayName) return displayName;

  const agentKey = session.agent_key?.trim();
  if (agentKey && !isUuidLike(agentKey)) return agentKey;
  return null;
}

function getShortcutOwnerLabel(session: ProjectSessionEntry): string | null {
  if (session.owner_type === "story") {
    return session.owner_title?.trim() ? `Story · ${session.owner_title.trim()}` : "Story";
  }
  if (session.owner_type === "task") {
    const taskTitle = session.owner_title?.trim() || "Task";
    const storyTitle = session.story_title?.trim();
    return storyTitle ? `Task · ${storyTitle} / ${taskTitle}` : `Task · ${taskTitle}`;
  }
  return null;
}

function getShortcutIndentClass(depth: number): string {
  if (depth <= 0) return "pl-2.5";
  if (depth === 1) return "pl-5";
  return "pl-8";
}

function estimateShortcutRowHeight(row: SessionShortcutRow): number {
  const titleLength = row.session.session_title?.trim().length ?? 0;
  const hasMeta = Boolean(
    row.parentRelationKind ||
      getShortcutAgentLabel(row.session) ||
      getShortcutOwnerLabel(row.session),
  );
  if (titleLength > 34 || hasMeta) return 58;
  return 42;
}

export function SessionShortcutList({ sessions }: { sessions: ProjectSessionEntry[] }) {
  const navigate = useNavigate();
  const location = useLocation();
  const listRef = useRef<HTMLDivElement>(null);
  const rowsRef = useRef<Map<string, HTMLButtonElement>>(new Map());
  const [rowHeights, setRowHeights] = useState<Map<string, number>>(new Map());
  const [containerH, setContainerH] = useState(0);

  const sessionRouteMatch = useMatch("/session/:sessionId");
  const activeSessionId = sessionRouteMatch?.params.sessionId ?? null;

  const rows = useMemo(() => buildSessionShortcutRows(sessions), [sessions]);

  // 监听容器高度变化
  useEffect(() => {
    const el = listRef.current;
    if (!el) return;
    const update = () => setContainerH(el.clientHeight);
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // 测量每行实际高度（记录到 id → height 的 Map）；DOM 变动时重算
  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const map = new Map<string, number>();
      rowsRef.current.forEach((el, id) => {
        map.set(id, el.offsetHeight);
      });
      setRowHeights((prev) => {
        // 仅当有差异时才 setState，避免无意义重渲染
        if (prev.size === map.size) {
          let same = true;
          for (const [k, v] of map) {
            if (prev.get(k) !== v) {
              same = false;
              break;
            }
          }
          if (same) return prev;
        }
        return map;
      });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [rows]);

  // 用已知行高 + 容器高度决定可见数量；未知行用保守估算
  const { displayed, hasMore } = useMemo(() => {
    if (rows.length === 0 || containerH <= 0) {
      return { displayed: rows, hasMore: false };
    }
    const estH = (row: SessionShortcutRow) =>
      rowHeights.get(row.session.session_id) ?? estimateShortcutRowHeight(row);
    let acc = 0;
    let count = 0;
    for (const row of rows) {
      const h = estH(row);
      if (acc + h > containerH) break;
      acc += h;
      count += 1;
    }
    if (count >= rows.length) {
      return { displayed: rows, hasMore: false };
    }
    return { displayed: rows.slice(0, Math.max(1, count)), hasMore: true };
  }, [rows, containerH, rowHeights]);

  return (
    <div className="flex min-h-0 flex-1 flex-col border-b border-border">
      {/* 标题行：左右各 px-4，与 ProjectDropdown 对齐 */}
      <div className="flex shrink-0 items-center justify-between px-4 pb-1.5 pt-3">
        <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">最近会话</span>
        {rows.length > 0 && (
          <span className="text-[10px] text-muted-foreground/70">
            {hasMore ? `${displayed.length} / ${rows.length}` : rows.length}
          </span>
        )}
      </div>
      {rows.length === 0 ? (
        <p className="px-4 pb-3 text-xs text-muted-foreground">暂无活跃会话</p>
      ) : (
        <>
          <div ref={listRef} className="min-h-0 flex-1 overflow-hidden px-3">
            {displayed.map((row) => {
              const { session } = row;
              const isActive = session.session_id === activeSessionId;
              const title = session.session_title?.trim() || "无标题会话";
              const agent = getShortcutAgentLabel(session);
              const owner = getShortcutOwnerLabel(session);
              const time = formatRelativeTime(session.last_activity);
              const indentClass = getShortcutIndentClass(row.depth);
              const metaParts = [
                row.parentRelationKind
                  ? sessionParentRelationLabel(row.parentRelationKind)
                  : null,
                agent,
                owner,
              ].filter((part): part is string => Boolean(part));
              const meta = metaParts.join(" · ");
              return (
                <button
                  key={session.session_id}
                  ref={(el) => {
                    if (el) rowsRef.current.set(session.session_id, el);
                    else rowsRef.current.delete(session.session_id);
                  }}
                  type="button"
                  onClick={() => {
                    if (location.pathname === `/session/${session.session_id}`) return;
                    navigate(`/session/${session.session_id}`);
                  }}
                  className={`flex w-full flex-col gap-1 rounded-[8px] py-2 pr-2.5 text-left transition-colors ${indentClass} ${
                    isActive ? "bg-primary/10" : "hover:bg-secondary/50"
                  }`}
                  title={meta ? `${title} · ${meta}` : title}
                >
                  <div className="flex items-start gap-2">
                    {row.parentRelationKind && (
                      <span className="mt-[3px] shrink-0 text-[11px] leading-none text-primary/70">
                        ↳
                      </span>
                    )}
                    <SessionStatusDot status={session.execution_status} />
                    <span className="min-w-0 flex-1 whitespace-normal break-words text-[13px] leading-[1.35] text-foreground line-clamp-2">
                      {title}
                    </span>
                    <span className="mt-[1px] shrink-0 text-[10px] tabular-nums text-muted-foreground">{time}</span>
                  </div>
                  {meta && (
                    <p className="ml-3.5 whitespace-normal break-words text-[11px] leading-[1.35] text-muted-foreground line-clamp-2">
                      {meta}
                    </p>
                  )}
                </button>
              );
            })}
          </div>
          {/* 固定按钮槽：无论 hasMore 与否都占相同高度，列表容器尺寸稳定 */}
          <div className="flex h-7 shrink-0 items-center justify-center px-3 pb-1">
            {hasMore && (
              <button
                type="button"
                onClick={() => navigate("/dashboard/agent")}
                title={`查看全部会话（还有 ${rows.length - displayed.length} 个）`}
                className="flex w-full items-center justify-center rounded-[8px] py-1 text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                  <circle cx="5" cy="12" r="1.5" />
                  <circle cx="12" cy="12" r="1.5" />
                  <circle cx="19" cy="12" r="1.5" />
                </svg>
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function formatRelativeTime(timestamp: number | null): string {
  if (timestamp == null) return "—";
  const ts = timestamp < 1e12 ? timestamp * 1000 : timestamp;
  const diffMs = Date.now() - ts;
  if (diffMs < 0) return "刚刚";
  const seconds = Math.floor(diffMs / 1000);
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d`;
  const date = new Date(ts);
  return `${date.getMonth() + 1}/${date.getDate()}`;
}
