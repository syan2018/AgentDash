/**
 * 活跃会话列表 — ActiveSessionList
 *
 * 设计（PR3 + 两行式行调整）：
 * - 顶部精简筛选条：搜索框 + 状态 tab（全部 / 进行中 / 空闲 / 已结束）
 * - 行级展示：两行式布局，单行约 52-60px
 *   - 第 1 行：状态圆点 + 标题（flex-1 截断）+ 最后活动时间（右侧紧凑元信息）
 *   - 第 2 行：agent 名 · Story/Task 归属（含"打开 Story"小按钮）· 状态标签
 *   - hover 才出现右侧操作区（跳转按钮）
 * - Story 分组带折叠头；折叠状态写入 localStorage（见 session-grouping.ts）
 * - Companion 默认折叠，父行右侧显示 `+N` 徽标，点击展开/收起
 *   - 折叠状态本地 useState，不持久化
 *   - 徽标点击 stopPropagation，避免触发行的 session 切换
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ProjectSessionEntry } from "../../types";
import {
  groupSessionsByStory,
  readStoryCollapsed,
  writeStoryCollapsed,
  type SessionGroupNode,
} from "./session-grouping";
import {
  applySessionFilters,
  type SessionStatusFilter,
} from "./session-filter";

// ─── 通用工具 ──────────────────────────────────────────────────────────────

function getAgentLabel(session: ProjectSessionEntry): string {
  if (session.agent_display_name) return session.agent_display_name;
  // agent_key 是 GUID，对用户无意义；agent 已删除时给占位文案
  return "已删除 Agent";
}

function isAgentDeleted(session: ProjectSessionEntry): boolean {
  return !session.agent_display_name;
}

function getOwnerBadgeLabel(session: ProjectSessionEntry): string | null {
  // 第二行显示的归属小字；优先用 owner_title / story_title
  if (session.owner_type === "story") {
    return session.owner_title ? `Story · ${session.owner_title}` : "Story";
  }
  if (session.owner_type === "task") {
    const storyPart = session.story_title ? `${session.story_title} / ` : "";
    const taskPart = session.owner_title ?? "Task";
    return `Task · ${storyPart}${taskPart}`;
  }
  return null; // project 会话不展示归属
}

function formatRelativeTime(timestamp: number | null): string {
  if (timestamp == null) return "无活动";
  const ts = timestamp < 1e12 ? timestamp * 1000 : timestamp;
  const diffMs = Date.now() - ts;
  if (diffMs < 0) return "刚刚";
  const seconds = Math.floor(diffMs / 1000);
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const date = new Date(ts);
  return `${date.getMonth() + 1}/${date.getDate()} ${date.getHours().toString().padStart(2, "0")}:${date.getMinutes().toString().padStart(2, "0")}`;
}

// 状态圆点样式（running 加脉冲动画）
function StatusDot({ status }: { status: ProjectSessionEntry["execution_status"] }) {
  const base = "h-2 w-2 shrink-0 rounded-full";
  switch (status) {
    case "running":
      return (
        <span className="relative flex h-2 w-2 shrink-0">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
          <span className={`${base} bg-emerald-500`} />
        </span>
      );
    case "completed":
      return <span className={`${base} bg-blue-500`} />;
    case "failed":
      return <span className={`${base} bg-red-500`} />;
    case "interrupted":
      return <span className={`${base} bg-amber-400`} />;
    default: // idle
      return <span className={`${base} bg-muted-foreground/25`} />;
  }
}

const statusLabel: Record<ProjectSessionEntry["execution_status"], string> = {
  running: "运行中",
  idle: "空闲",
  completed: "已完成",
  failed: "失败",
  interrupted: "已中断",
};

const statusPillClass: Record<ProjectSessionEntry["execution_status"], string> = {
  running: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
  completed: "bg-blue-500/10 text-blue-500",
  failed: "bg-red-500/10 text-red-500",
  interrupted: "bg-amber-500/10 text-amber-600 dark:text-amber-400",
  idle: "bg-muted text-muted-foreground",
};

// ─── SessionRow：两行式会话行 ────────────────────────────────────────────

interface SessionRowProps {
  session: ProjectSessionEntry;
  isSelected: boolean;
  onSelectSession: (sessionId: string) => void;
  /** 缩进层级（0 = story 下的 task；1 = companion；以此类推）。用于左侧 padding。 */
  indent: number;
  /** 是否作为 companion 展示（决定前缀箭头与次级视觉权重） */
  isCompanion?: boolean;
  /** companion 数量：>0 时在标题右侧显示 `+N` 徽标 */
  companionCount?: number;
  /** companion 是否已展开（仅当 companionCount>0 时生效） */
  companionsExpanded?: boolean;
  /** 切换 companion 展开状态 */
  onToggleCompanions?: () => void;
}

function SessionRow({
  session,
  isSelected,
  onSelectSession,
  indent,
  isCompanion = false,
  companionCount = 0,
  companionsExpanded = false,
  onToggleCompanions,
}: SessionRowProps) {
  const navigate = useNavigate();

  // 行左 padding：基础 12px + 每层 16px（story 标题下 task 行 indent=1 → 28px）
  const leftPadPx = 12 + indent * 16;

  const agentText = getAgentLabel(session);
  const timeText = formatRelativeTime(session.last_activity);
  const ownerLabel = getOwnerBadgeLabel(session);

  const handleClick = () => onSelectSession(session.session_id);

  const canOpenStory =
    session.owner_type === "story" || (session.owner_type === "task" && !!session.story_id);

  const storyIdForNav =
    session.owner_type === "story" ? session.owner_id : session.story_id;

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          handleClick();
        }
      }}
      style={{ paddingLeft: leftPadPx }}
      className={`group flex cursor-pointer flex-col justify-center gap-0.5 border-b border-border/40 py-2 pr-3 transition-colors ${
        isSelected ? "bg-primary/5" : "hover:bg-muted/40"
      }`}
      title={session.session_title ?? "无标题会话"}
    >
      {/* ── 第 1 行：圆点 + 标题 + 时间 + hover 操作区 ── */}
      <div className="flex items-center gap-2">
        {/* 前缀：companion 箭头 */}
        {isCompanion && (
          <span className="shrink-0 text-[11px] text-violet-400/70">↳</span>
        )}

        {/* 状态圆点 */}
        <StatusDot status={session.execution_status} />

        {/* 标题 */}
        <span
          className={`min-w-0 flex-1 truncate ${
            isCompanion ? "text-xs text-muted-foreground" : "text-sm text-foreground"
          } ${isSelected ? "font-medium" : ""}`}
        >
          {session.session_title ?? "无标题会话"}
        </span>

        {/* 时间 */}
        <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground/60">
          {timeText}
        </span>

        {/* Hover 操作区 */}
        <div className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
          {canOpenStory && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                if (storyIdForNav) navigate(`/story/${storyIdForNav}`);
              }}
              className="rounded px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-background hover:text-foreground"
              title="打开所属 Story"
            >
              打开 Story ↗
            </button>
          )}
        </div>
      </div>

      {/* ── 第 2 行：agent · 归属 · 状态 · companion 折叠 ── */}
      <div className="flex items-center gap-1.5 pl-4 text-xs text-muted-foreground">
        <span
          className={`min-w-0 max-w-[55%] shrink-0 truncate ${
            isAgentDeleted(session) ? "italic text-muted-foreground/50" : ""
          }`}
          title={agentText}
        >
          {agentText}
        </span>
        {ownerLabel && (
          <>
            <span className="shrink-0 text-muted-foreground/30">·</span>
            <span className="min-w-0 flex-1 truncate" title={ownerLabel}>
              {ownerLabel}
            </span>
          </>
        )}
        {!ownerLabel && <span className="min-w-0 flex-1" />}
        <span
          className={`shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-medium ${statusPillClass[session.execution_status]}`}
        >
          {statusLabel[session.execution_status]}
        </span>

        {/* Companion 折叠按钮（右下角，明确文案） */}
        {companionCount > 0 && onToggleCompanions && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleCompanions();
            }}
            className="flex shrink-0 items-center gap-1 rounded-full border border-violet-400/40 bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-medium text-violet-600 transition-colors hover:bg-violet-500/20 dark:text-violet-300"
            title={
              companionsExpanded
                ? `折叠 ${companionCount} 个 companion 子会话`
                : `展开 ${companionCount} 个 companion 子会话`
            }
            aria-expanded={companionsExpanded}
            aria-label={
              companionsExpanded
                ? `折叠 ${companionCount} 个 companion`
                : `展开 ${companionCount} 个 companion`
            }
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="11"
              height="11"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.4"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
              className={`transition-transform ${companionsExpanded ? "rotate-90" : ""}`}
            >
              <path d="m9 18 6-6-6-6" />
            </svg>
            <span className="leading-none">
              {companionsExpanded ? "折叠" : "展开"} {companionCount} 个 companion
            </span>
          </button>
        )}
      </div>
    </div>
  );
}

// ─── SessionSubtree：递归渲染 task + companions（含折叠） ─────────────

interface SessionSubtreeProps {
  node: SessionGroupNode;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  /** 本节点自身行的 indent（companions 会 +1） */
  indent: number;
  /** 本节点自身是否以 companion 形式渲染（↳ 前缀） */
  asCompanion?: boolean;
}

function SessionSubtree({
  node,
  selectedSessionId,
  onSelectSession,
  indent,
  asCompanion = false,
}: SessionSubtreeProps) {
  // Companion 折叠：默认收起；本地 useState，不持久化
  const [companionsExpanded, setCompanionsExpanded] = useState(false);
  const companionCount = node.companions.length;

  return (
    <>
      <SessionRow
        session={node.session}
        isSelected={selectedSessionId === node.session.session_id}
        onSelectSession={onSelectSession}
        indent={indent}
        isCompanion={asCompanion}
        companionCount={companionCount}
        companionsExpanded={companionsExpanded}
        onToggleCompanions={
          companionCount > 0 ? () => setCompanionsExpanded((v) => !v) : undefined
        }
      />
      {/* Story 的 child task（不参与 companion 折叠） */}
      {node.children.map((child) => (
        <SessionSubtree
          key={child.session.session_id}
          node={child}
          selectedSessionId={selectedSessionId}
          onSelectSession={onSelectSession}
          indent={indent + 1}
        />
      ))}
      {/* Companion 行：受折叠控制 */}
      {companionsExpanded &&
        node.companions.map((companion) => (
          <SessionRow
            key={companion.session_id}
            session={companion}
            isSelected={selectedSessionId === companion.session_id}
            onSelectSession={onSelectSession}
            indent={indent + 1}
            isCompanion
          />
        ))}
    </>
  );
}

// ─── StoryGroupHeader：可折叠的 Story 分组头 ─────────────────────────

interface StoryGroupHeaderProps {
  node: SessionGroupNode; // kind === "story"
  collapsed: boolean;
  onToggle: () => void;
  descendantCount: number;
}

function StoryGroupHeader({ node, collapsed, onToggle, descendantCount }: StoryGroupHeaderProps) {
  const navigate = useNavigate();
  const story = node.session;
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onToggle}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onToggle();
        }
      }}
      className="group flex h-[36px] cursor-pointer items-center gap-2 border-b border-border bg-muted/30 px-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground transition-colors hover:bg-muted/50"
      aria-expanded={!collapsed}
      title={collapsed ? "展开 Story" : "折叠 Story"}
    >
      <span
        className={`inline-block shrink-0 text-[10px] transition-transform ${collapsed ? "" : "rotate-90"}`}
      >
        ▶
      </span>
      <span className="shrink-0 text-[10px] tracking-[0.14em] text-muted-foreground/70">STORY</span>
      <span className="min-w-0 flex-1 truncate text-[12px] font-semibold normal-case tracking-normal text-foreground">
        {story.owner_title ?? story.session_title ?? "未命名 Story"}
      </span>
      <span className="shrink-0 text-[11px] font-normal normal-case tracking-normal text-muted-foreground/70">
        {descendantCount} 个会话
      </span>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          navigate(`/story/${story.owner_id}`);
        }}
        className="ml-1 shrink-0 rounded px-1.5 py-0.5 text-[10px] font-normal normal-case tracking-normal text-muted-foreground opacity-0 transition-opacity hover:bg-background hover:text-foreground group-hover:opacity-100"
        title="打开 Story"
      >
        打开 ↗
      </button>
    </div>
  );
}

// ─── StoryGroup：Story 分组（头 + 子树） ─────────────────────────────

interface StoryGroupProps {
  node: SessionGroupNode; // kind === "story"
  projectId: string;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
}

function countDescendants(node: SessionGroupNode): number {
  // Story 行自身 + children (task) + 各层 companions
  let count = 1 + node.companions.length;
  for (const child of node.children) {
    count += 1 + child.companions.length;
  }
  return count;
}

function StoryGroup({ node, projectId, selectedSessionId, onSelectSession }: StoryGroupProps) {
  const storyId = node.session.owner_id;
  const [collapsed, setCollapsed] = useState<boolean>(() =>
    readStoryCollapsed(projectId, storyId),
  );

  // 项目或 story 切换时，重新读取持久化状态（key 变了）
  useEffect(() => {
    setCollapsed(readStoryCollapsed(projectId, storyId));
  }, [projectId, storyId]);

  const handleToggle = useCallback(() => {
    setCollapsed((prev) => {
      const next = !prev;
      writeStoryCollapsed(projectId, storyId, next);
      return next;
    });
  }, [projectId, storyId]);

  const descendantCount = countDescendants(node);

  // Story 自身的 companion 折叠状态（同样本地 useState）
  const [storyCompanionsExpanded, setStoryCompanionsExpanded] = useState(false);
  const storyCompanionCount = node.companions.length;

  return (
    <div>
      <StoryGroupHeader
        node={node}
        collapsed={collapsed}
        onToggle={handleToggle}
        descendantCount={descendantCount}
      />
      {!collapsed && (
        <>
          {/* Story session 自身作为第一行（indent 0） */}
          <SessionRow
            session={node.session}
            isSelected={selectedSessionId === node.session.session_id}
            onSelectSession={onSelectSession}
            indent={0}
            companionCount={storyCompanionCount}
            companionsExpanded={storyCompanionsExpanded}
            onToggleCompanions={
              storyCompanionCount > 0
                ? () => setStoryCompanionsExpanded((v) => !v)
                : undefined
            }
          />
          {/* Story 自己的 companions：受折叠控制 */}
          {storyCompanionsExpanded &&
            node.companions.map((companion) => (
              <SessionRow
                key={companion.session_id}
                session={companion}
                isSelected={selectedSessionId === companion.session_id}
                onSelectSession={onSelectSession}
                indent={1}
                isCompanion
              />
            ))}
          {/* Story 下的 Task（indent=1） + task 自己的 companions (indent=2) */}
          {node.children.map((child) => (
            <SessionSubtree
              key={child.session.session_id}
              node={child}
              selectedSessionId={selectedSessionId}
              onSelectSession={onSelectSession}
              indent={1}
            />
          ))}
        </>
      )}
    </div>
  );
}

// ─── 筛选条：搜索框 + 状态 tab ─────────────────────────────────────

interface SessionFilterBarProps {
  keyword: string;
  onKeywordChange: (value: string) => void;
  status: SessionStatusFilter;
  onStatusChange: (value: SessionStatusFilter) => void;
  counts: Record<SessionStatusFilter, number>;
}

const STATUS_TABS: Array<{ key: SessionStatusFilter; label: string }> = [
  { key: "all", label: "全部" },
  { key: "running", label: "进行中" },
  { key: "idle", label: "空闲" },
  { key: "ended", label: "已结束" },
];

function SessionFilterBar({
  keyword,
  onKeywordChange,
  status,
  onStatusChange,
  counts,
}: SessionFilterBarProps) {
  return (
    <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border bg-background px-3">
      {/* 搜索框 */}
      <div className="relative flex h-7 min-w-0 flex-1 items-center">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
          className="pointer-events-none absolute left-2 text-muted-foreground/70"
        >
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.3-4.3" />
        </svg>
        <input
          type="text"
          value={keyword}
          onChange={(e) => onKeywordChange(e.target.value)}
          placeholder="搜索 session…"
          className="h-7 w-full rounded-md border border-border bg-muted/40 pl-8 pr-7 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary focus:bg-background"
          aria-label="搜索 session"
        />
        {keyword.length > 0 && (
          <button
            type="button"
            onClick={() => onKeywordChange("")}
            className="absolute right-1 flex h-5 w-5 items-center justify-center rounded-full text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground"
            title="清除搜索"
            aria-label="清除搜索"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M18 6 6 18" />
              <path d="m6 6 12 12" />
            </svg>
          </button>
        )}
      </div>
      {/* 状态 tab */}
      <div className="flex shrink-0 items-center gap-1">
        {STATUS_TABS.map((tab) => {
          const active = status === tab.key;
          const count = counts[tab.key];
          return (
            <button
              key={tab.key}
              type="button"
              onClick={() => onStatusChange(tab.key)}
              className={`flex h-7 items-center gap-1 rounded-md px-2 text-[11px] font-medium transition-colors ${
                active
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              }`}
              aria-pressed={active}
            >
              <span>{tab.label}</span>
              <span
                className={`tabular-nums ${active ? "text-primary/80" : "text-muted-foreground/60"}`}
              >
                {count}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

// ─── ActiveSessionList ────────────────────────────────────────────────────

interface ActiveSessionListProps {
  projectId: string;
  sessions: ProjectSessionEntry[];
  isLoading: boolean;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
}

export function ActiveSessionList({
  projectId,
  sessions,
  isLoading,
  selectedSessionId,
  onSelectSession,
}: ActiveSessionListProps) {
  const [keyword, setKeyword] = useState("");
  const [status, setStatus] = useState<SessionStatusFilter>("all");

  // 项目切换时重置筛选状态（官方「在 props 变化时调整 state」模式）
  const [lastProjectId, setLastProjectId] = useState(projectId);
  if (lastProjectId !== projectId) {
    setLastProjectId(projectId);
    setKeyword("");
    setStatus("all");
  }

  // 预计算各状态 tab 的计数（仅受 keyword 过滤影响，不含当前 status）
  const countsByStatus: Record<SessionStatusFilter, number> = useMemo(() => {
    const base = applySessionFilters(sessions, keyword, "all");
    const counts: Record<SessionStatusFilter, number> = {
      all: base.length,
      running: 0,
      idle: 0,
      ended: 0,
    };
    for (const s of base) {
      switch (s.execution_status) {
        case "running":
          counts.running += 1;
          break;
        case "idle":
          counts.idle += 1;
          break;
        case "completed":
        case "failed":
        case "interrupted":
          counts.ended += 1;
          break;
      }
    }
    return counts;
  }, [sessions, keyword]);

  const filteredSessions = useMemo(
    () => applySessionFilters(sessions, keyword, status),
    [sessions, keyword, status],
  );

  const roots = useMemo(() => groupSessionsByStory(filteredSessions), [filteredSessions]);

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  const isEmpty = sessions.length === 0;
  const isFilteredEmpty = !isEmpty && filteredSessions.length === 0;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Header */}
      <div className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-5">
        <div className="flex items-center gap-2.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            SESSION
          </span>
          <div>
            <p className="text-sm font-semibold tracking-tight text-foreground">活跃会话</p>
            <p className="text-xs text-muted-foreground">
              {isEmpty
                ? "0 个会话"
                : keyword || status !== "all"
                  ? `${filteredSessions.length} / ${sessions.length} 个会话`
                  : `${sessions.length} 个会话`}
            </p>
          </div>
        </div>
      </div>

      {/* 筛选条（有 session 才显示） */}
      {!isEmpty && (
        <SessionFilterBar
          keyword={keyword}
          onKeywordChange={setKeyword}
          status={status}
          onStatusChange={setStatus}
          counts={countsByStatus}
        />
      )}

      {/* 列表 */}
      <div className="flex-1 overflow-y-auto">
        {isEmpty ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <p className="text-sm font-medium text-muted-foreground">暂无活跃会话</p>
            <p className="text-xs text-muted-foreground/60">
              点击左侧 Agent 的「打开会话」按钮来创建或恢复会话
            </p>
          </div>
        ) : isFilteredEmpty ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <p className="text-sm font-medium text-muted-foreground">未匹配到会话</p>
            <p className="text-xs text-muted-foreground/60">请调整搜索关键词或状态筛选</p>
          </div>
        ) : (
          roots.map((root) => {
            if (root.kind === "story") {
              return (
                <StoryGroup
                  key={root.session.session_id}
                  node={root}
                  projectId={projectId}
                  selectedSessionId={selectedSessionId}
                  onSelectSession={onSelectSession}
                />
              );
            }
            // orphan / project：根级 line-row + companions
            return (
              <SessionSubtree
                key={root.session.session_id}
                node={root}
                selectedSessionId={selectedSessionId}
                onSelectSession={onSelectSession}
                indent={0}
              />
            );
          })
        )}
      </div>
    </div>
  );
}
