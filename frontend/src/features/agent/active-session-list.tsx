/**
 * 活跃会话列表 — ActiveSessionList
 *
 * 设计（PR2）：
 * - 行级展示（line-row）：高度 ~36-40px、`border-b` 分隔、hover 时右侧淡入操作区
 * - 分组：先按 `groupSessionsByStory` 做 Story → Task → Companion 深嵌套
 *   - Story 分组带折叠头；折叠状态写入 localStorage（key 见 session-grouping.ts）
 *   - Task 作为 Story 的 child；Companion 按 parent_session_id 嵌套在所属行下方
 *   - orphan / project 会话以根级 line-row 渲染
 * - 行点击切换到 SessionChatView（由父层 onSelectSession 处理）
 *
 * 注：本次重构只影响右栏会话列表的视觉与分组，不动 SSE / store / 选中态流转。
 */

import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ProjectSessionEntry } from "../../types";
import {
  groupSessionsByStory,
  readStoryCollapsed,
  writeStoryCollapsed,
  type SessionGroupNode,
} from "./session-grouping";

// ─── 通用工具 ──────────────────────────────────────────────────────────────

function getAgentLabel(session: ProjectSessionEntry): string {
  return session.agent_display_name ?? session.agent_key ?? "—";
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

// ─── SessionRow：单行会话 ────────────────────────────────────────────────

interface SessionRowProps {
  session: ProjectSessionEntry;
  isSelected: boolean;
  onSelectSession: (sessionId: string) => void;
  /** 缩进层级（0 = story 下的 task；1 = companion；以此类推）。用于左侧 padding。 */
  indent: number;
  /** 是否作为 companion 展示（决定前缀箭头与次级视觉权重） */
  isCompanion?: boolean;
}

function SessionRow({
  session,
  isSelected,
  onSelectSession,
  indent,
  isCompanion = false,
}: SessionRowProps) {
  const navigate = useNavigate();

  // 行左 padding：基础 12px + 每层 16px（story 标题下 task 行 indent=1 → 28px）
  const leftPadPx = 12 + indent * 16;

  // 时间/agent 次要信息；companion 行字号更小
  const agentText = getAgentLabel(session);
  const timeText = formatRelativeTime(session.last_activity);

  const handleClick = () => onSelectSession(session.session_id);

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
      className={`group flex h-[38px] cursor-pointer items-center gap-2 border-b border-border/50 pr-3 transition-colors ${
        isSelected
          ? "bg-primary/5"
          : "hover:bg-muted/40"
      }`}
      title={session.session_title ?? "无标题会话"}
    >
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

      {/* Agent（始终可见，次要信息，缩短） */}
      <span className="hidden shrink-0 truncate text-[11px] text-muted-foreground/70 sm:inline-block sm:max-w-[110px]">
        {agentText}
      </span>

      {/* 分隔 */}
      <span className="hidden shrink-0 text-muted-foreground/30 sm:inline">·</span>

      {/* 时间 */}
      <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground/60">
        {timeText}
      </span>

      {/* Hover 操作区：状态 pill + 打开 Story 快捷键 */}
      <div className="flex shrink-0 items-center gap-2 opacity-0 transition-opacity group-hover:opacity-100">
        <span
          className={`inline-block rounded-full px-1.5 py-0.5 text-[10px] font-medium ${
            session.execution_status === "running"
              ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
              : session.execution_status === "failed"
                ? "bg-red-500/10 text-red-500"
                : session.execution_status === "completed"
                  ? "bg-blue-500/10 text-blue-500"
                  : "bg-muted text-muted-foreground"
          }`}
        >
          {statusLabel[session.execution_status]}
        </span>
        {/* Task / Story 行：提供跳转到 Story 详情页的按钮 */}
        {(session.owner_type === "story" || (session.owner_type === "task" && session.story_id)) && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              const storyId =
                session.owner_type === "story" ? session.owner_id : session.story_id;
              if (storyId) navigate(`/story/${storyId}`);
            }}
            className="rounded px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-background hover:text-foreground"
            title="打开所属 Story"
          >
            打开 Story ↗
          </button>
        )}
      </div>
    </div>
  );
}

// ─── SessionSubtree：递归渲染 task + companions ─────────────────────────

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
  return (
    <>
      <SessionRow
        session={node.session}
        isSelected={selectedSessionId === node.session.session_id}
        onSelectSession={onSelectSession}
        indent={indent}
        isCompanion={asCompanion}
      />
      {/* Story 的 child task */}
      {node.children.map((child) => (
        <SessionSubtree
          key={child.session.session_id}
          node={child}
          selectedSessionId={selectedSessionId}
          onSelectSession={onSelectSession}
          indent={indent + 1}
        />
      ))}
      {/* Companion（父子 session） */}
      {node.companions.map((companion) => (
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
          />
          {/* Story 自己的 companions */}
          {node.companions.map((companion) => (
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
  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
        <p className="text-sm font-medium text-muted-foreground">暂无活跃会话</p>
        <p className="text-xs text-muted-foreground/60">
          点击左侧 Agent 的「打开会话」按钮来创建或恢复会话
        </p>
      </div>
    );
  }

  const roots = groupSessionsByStory(sessions);

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
            <p className="text-xs text-muted-foreground">{sessions.length} 个会话</p>
          </div>
        </div>
      </div>

      {/* 列表 */}
      <div className="flex-1 overflow-y-auto">
        {roots.map((root) => {
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
        })}
      </div>
    </div>
  );
}
