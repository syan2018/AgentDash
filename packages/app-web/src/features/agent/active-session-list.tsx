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
 * - Parent relation children 默认折叠，父行右侧显示 `+N` 徽标，点击展开/收起
 *   - 折叠状态本地 useState，不持久化
 *   - 徽标点击 stopPropagation，避免触发行的 session 切换
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { SessionStatusDot } from "../../components/ui/session-status-dot";
import type { ProjectSessionEntry } from "../../types";
import {
  groupSessionsByStory,
  readStoryCollapsed,
  writeStoryCollapsed,
  type SessionGroupNode,
} from "./session-grouping";
import { sessionParentRelationLabel } from "./session-relations";
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

const statusLabel: Record<ProjectSessionEntry["execution_status"], string> = {
  running: "运行中",
  idle: "空闲",
  completed: "已完成",
  failed: "失败",
  interrupted: "已中断",
};

const statusPillClass: Record<ProjectSessionEntry["execution_status"], string> = {
  running: "bg-success/10 text-success",
  completed: "bg-info/10 text-info",
  failed: "bg-destructive/10 text-destructive",
  interrupted: "bg-warning/10 text-warning",
  idle: "bg-muted text-muted-foreground",
};

// ─── SessionRow：两行式会话行 ────────────────────────────────────────────

interface SessionRowProps {
  session: ProjectSessionEntry;
  isSelected: boolean;
  onSelectSession: (sessionId: string) => void;
  /** 缩进层级（0 = story 下的 task；1 = relation child；以此类推）。用于左侧 padding。 */
  indent: number;
  /** 当前行是否作为 parent relation child 展示 */
  parentRelationKind?: ProjectSessionEntry["parent_relation_kind"];
  /** 关联子会话数量：>0 时在标题右侧显示折叠按钮 */
  linkedChildCount?: number;
  /** 关联子会话是否已展开 */
  linkedChildrenExpanded?: boolean;
  /** 切换关联子会话展开状态 */
  onToggleLinkedChildren?: () => void;
}

function SessionRow({
  session,
  isSelected,
  onSelectSession,
  indent,
  parentRelationKind = null,
  linkedChildCount = 0,
  linkedChildrenExpanded = false,
  onToggleLinkedChildren,
}: SessionRowProps) {
  const navigate = useNavigate();

  // 行左 padding：基础 12px + 每层 16px（story 标题下 task 行 indent=1 → 28px）
  const leftPadPx = 12 + indent * 16;

  const agentText = getAgentLabel(session);
  const timeText = formatRelativeTime(session.last_activity);
  const ownerLabel = getOwnerBadgeLabel(session);
  const relationLabel = parentRelationKind
    ? sessionParentRelationLabel(parentRelationKind)
    : null;

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
        {/* 前缀：parent relation */}
        {relationLabel && (
          <span className="inline-flex shrink-0 items-center gap-1 text-[11px] text-primary/70">
            <span aria-hidden>↳</span>
            <span>{relationLabel}</span>
          </span>
        )}

        {/* 状态圆点 */}
        <SessionStatusDot status={session.execution_status} size="md" />

        {/* 标题 */}
        <span
          className={`min-w-0 flex-1 truncate ${
            relationLabel ? "text-xs text-muted-foreground" : "text-sm text-foreground"
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

      {/* ── 第 2 行：agent · 归属 · 状态 · relation 折叠 ── */}
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

        {/* Relation child 折叠按钮（状态 pill 之前，灰色低调） */}
        {linkedChildCount > 0 && onToggleLinkedChildren && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleLinkedChildren();
            }}
            className="flex shrink-0 items-center gap-1 rounded-[8px] border border-border bg-muted/50 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            title={
              linkedChildrenExpanded
                ? `折叠 ${linkedChildCount} 个关联子会话`
                : `展开 ${linkedChildCount} 个关联子会话`
            }
            aria-expanded={linkedChildrenExpanded}
            aria-label={
              linkedChildrenExpanded
                ? `折叠 ${linkedChildCount} 个关联子会话`
                : `展开 ${linkedChildCount} 个关联子会话`
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
              className={`transition-transform ${linkedChildrenExpanded ? "rotate-90" : ""}`}
            >
              <path d="m9 18 6-6-6-6" />
            </svg>
            <span className="leading-none">
              {linkedChildrenExpanded ? "折叠" : "展开"} {linkedChildCount} 个关联
            </span>
          </button>
        )}

        <span
          className={`shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-medium ${statusPillClass[session.execution_status]}`}
        >
          {statusLabel[session.execution_status]}
        </span>
      </div>
    </div>
  );
}

// ─── SessionSubtree：递归渲染 task + relation children（含折叠） ─────────────

interface SessionSubtreeProps {
  node: SessionGroupNode;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  /** 本节点自身行的 indent（relation children 会 +1） */
  indent: number;
  /** 本节点自身是否以 parent relation child 形式渲染 */
  parentRelationKind?: ProjectSessionEntry["parent_relation_kind"];
}

function SessionSubtree({
  node,
  selectedSessionId,
  onSelectSession,
  indent,
  parentRelationKind = null,
}: SessionSubtreeProps) {
  // Relation child 折叠：默认收起；本地 useState，不持久化
  const [linkedChildrenExpanded, setLinkedChildrenExpanded] = useState(false);
  const linkedChildCount = node.linkedChildren.length;

  return (
    <>
      <SessionRow
        session={node.session}
        isSelected={selectedSessionId === node.session.session_id}
        onSelectSession={onSelectSession}
        indent={indent}
        parentRelationKind={parentRelationKind}
        linkedChildCount={linkedChildCount}
        linkedChildrenExpanded={linkedChildrenExpanded}
        onToggleLinkedChildren={
          linkedChildCount > 0 ? () => setLinkedChildrenExpanded((v) => !v) : undefined
        }
      />
      {/* Story 的 child task（不参与 relation child 折叠） */}
      {node.children.map((child) => (
        <SessionSubtree
          key={child.session.session_id}
          node={child}
          selectedSessionId={selectedSessionId}
          onSelectSession={onSelectSession}
          indent={indent + 1}
        />
      ))}
      {/* Parent relation 行：受折叠控制 */}
      {linkedChildrenExpanded &&
        node.linkedChildren.map((child) => (
          <SessionRow
            key={child.session.session_id}
            session={child.session}
            isSelected={selectedSessionId === child.session.session_id}
            onSelectSession={onSelectSession}
            indent={indent + 1}
            parentRelationKind={child.relation_kind}
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
  // Story 行自身 + children (task) + 各层 relation children
  let count = 1 + node.linkedChildren.length;
  for (const child of node.children) {
    count += 1 + child.linkedChildren.length;
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

  // Story 自身的 relation child 折叠状态（同样本地 useState）
  const [storyLinkedChildrenExpanded, setStoryLinkedChildrenExpanded] = useState(false);
  const storyLinkedChildCount = node.linkedChildren.length;

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
            linkedChildCount={storyLinkedChildCount}
            linkedChildrenExpanded={storyLinkedChildrenExpanded}
            onToggleLinkedChildren={
              storyLinkedChildCount > 0
                ? () => setStoryLinkedChildrenExpanded((v) => !v)
                : undefined
            }
          />
          {/* Story 自己的 relation children：受折叠控制 */}
          {storyLinkedChildrenExpanded &&
            node.linkedChildren.map((child) => (
              <SessionRow
                key={child.session.session_id}
                session={child.session}
                isSelected={selectedSessionId === child.session.session_id}
                onSelectSession={onSelectSession}
                indent={1}
                parentRelationKind={child.relation_kind}
              />
            ))}
          {/* Story 下的 Task（indent=1） + task 自己的 relation children (indent=2) */}
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
            className="absolute right-1 flex h-5 w-5 items-center justify-center rounded-[8px] text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground"
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
        {/* eslint-disable-next-line no-restricted-syntax -- 加载旋转器必须为圆形 */}
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
            // orphan / project：根级 line-row + relation children
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
