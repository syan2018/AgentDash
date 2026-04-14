/**
 * 活跃会话列表 — ActiveSessionList / ActiveSessionCard
 *
 * 功能：
 * - 按时间倒序展示项目下所有活跃会话
 * - Companion 子会话嵌套在父会话下方渲染，而非平铺
 * - Task / Story 归属显示为可点击链接，跳转到对应 Story 页
 * - 状态圆点带 running 动画
 */

import { useNavigate } from "react-router-dom";
import type { ProjectSessionEntry } from "../../types";

// ─── 工具函数 ──────────────────────────────────────────────────────────────

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

// ─── OwnerLink：Story / Task 归属链接 ─────────────────────────────────────

interface OwnerLinkProps {
  session: ProjectSessionEntry;
}

function OwnerLink({ session }: OwnerLinkProps) {
  const navigate = useNavigate();

  // project 层级：显示项目名
  if (session.owner_type === "project") {
    return (
      <span className="truncate text-[11px] text-muted-foreground/50">
        {session.owner_title ?? "Project"}
      </span>
    );
  }

  // story 层级：链接到 Story 页
  if (session.owner_type === "story") {
    return (
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation(); // 阻止触发卡片 onSelectSession
          navigate(`/story/${session.owner_id}`);
        }}
        className="group inline-flex max-w-full items-center gap-1 truncate text-[11px] text-muted-foreground transition-colors hover:text-foreground"
        title={`打开 Story：${session.owner_title ?? ""}`}
      >
        <span className="truncate">{session.owner_title ?? "未知 Story"}</span>
        <span className="shrink-0 opacity-0 transition-opacity group-hover:opacity-100">↗</span>
      </button>
    );
  }

  // task 层级：链接到所属 Story 页（Task 在 StoryPage 内通过 drawer 打开，暂无独立路由）
  const storyId = session.story_id;
  if (storyId) {
    return (
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          navigate(`/story/${storyId}`);
        }}
        className="group inline-flex max-w-full items-center gap-1 truncate text-[11px] text-muted-foreground transition-colors hover:text-foreground"
        title={`打开 Story：${session.story_title ?? ""}`}
      >
        <span className="shrink-0 text-muted-foreground/40">{session.story_title ?? "Story"}</span>
        <span className="shrink-0 text-muted-foreground/30">/</span>
        <span className="truncate">{session.owner_title ?? "未知 Task"}</span>
        <span className="shrink-0 opacity-0 transition-opacity group-hover:opacity-100">↗</span>
      </button>
    );
  }

  return (
    <span className="truncate text-[11px] text-muted-foreground">
      {session.owner_title ?? "未知归属"}
    </span>
  );
}

// ─── ActiveSessionCard ────────────────────────────────────────────────────

interface ActiveSessionCardProps {
  session: ProjectSessionEntry;
  companions: ProjectSessionEntry[];
  isSelected: boolean;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  depth?: number;
}

function ActiveSessionCard({
  session,
  companions,
  isSelected,
  selectedSessionId,
  onSelectSession,
  depth = 0,
}: ActiveSessionCardProps) {
  const isCompanion = depth > 0;

  return (
    <div>
      <div
        role="button"
        tabIndex={0}
        onClick={() => onSelectSession(session.session_id)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") onSelectSession(session.session_id);
        }}
        className={`cursor-pointer transition-colors ${
          isCompanion
            ? `rounded-[8px] border border-dashed ${
                isSelected
                  ? "border-primary/25 bg-primary/3"
                  : "border-border/40 bg-transparent hover:border-border/60 hover:bg-background/50"
              }`
            : `rounded-[12px] border ${
                isSelected
                  ? "border-primary/30 bg-primary/5"
                  : "border-border bg-background/80 hover:border-border/80 hover:bg-background"
              }`
        }`}
      >
        <div className={isCompanion ? "px-2.5 py-2" : "px-3.5 py-3"}>
          {/* ── 顶行：状态 + 标题 + 时间 ── */}
          <div className="flex items-center gap-2">
            <StatusDot status={session.execution_status} />
            <p className={`min-w-0 flex-1 truncate font-medium text-foreground ${isCompanion ? "text-xs" : "text-sm"}`}>
              {session.session_title ?? "无标题会话"}
            </p>
            <span className={`shrink-0 text-muted-foreground/50 ${isCompanion ? "text-[10px]" : "text-[11px]"}`}>
              {formatRelativeTime(session.last_activity)}
            </span>
          </div>

          {/* ── 底行：归属 · Agent · 状态 ── */}
          <div className={isCompanion ? "mt-1 flex items-center gap-1.5" : "mt-1.5 flex items-center gap-2"}>
            {isCompanion
              ? <span className="shrink-0 text-[11px] text-violet-400/60">↳</span>
              : <OwnerLink session={session} />
            }
            <span className="shrink-0 text-muted-foreground/25">·</span>
            <span className={`shrink-0 text-muted-foreground/50 ${isCompanion ? "text-[10px]" : "text-[11px]"}`}>
              {getAgentLabel(session)}
            </span>
            <span className="ml-auto shrink-0">
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
            </span>
          </div>
        </div>
      </div>

      {/* 嵌套 Companion 子会话 */}
      {companions.length > 0 && (
        <div className="ml-3 mt-1 space-y-1 border-l border-border/30 pl-2.5">
          {companions.map((companion) => (
            <ActiveSessionCard
              key={companion.session_id}
              session={companion}
              companions={[]}
              isSelected={selectedSessionId === companion.session_id}
              selectedSessionId={selectedSessionId}
              onSelectSession={onSelectSession}
              depth={depth + 1}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ─── ActiveSessionList ────────────────────────────────────────────────────

interface ActiveSessionListProps {
  sessions: ProjectSessionEntry[];
  isLoading: boolean;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
}

export function ActiveSessionList({
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

  // 将 sessions 按父子关系分组：根会话 + companion 映射
  const rootSessions = sessions.filter((s) => s.parent_session_id === null);
  const companionsByParent = sessions.reduce<Record<string, ProjectSessionEntry[]>>((acc, s) => {
    if (s.parent_session_id) {
      (acc[s.parent_session_id] ??= []).push(s);
    }
    return acc;
  }, {});

  // 没有找到父会话的孤立 companion（父会话可能不在当前项目范围内），降级为根节点展示
  const orphanCompanions = sessions.filter(
    (s) => s.parent_session_id !== null && !rootSessions.some((r) => r.session_id === s.parent_session_id),
  );

  const allRoots = [...rootSessions, ...orphanCompanions];

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
      <div className="flex-1 overflow-y-auto p-4">
        <div className="space-y-2">
          {allRoots.map((session) => (
            <ActiveSessionCard
              key={session.session_id}
              session={session}
              companions={companionsByParent[session.session_id] ?? []}
              isSelected={selectedSessionId === session.session_id}
              selectedSessionId={selectedSessionId}
              onSelectSession={onSelectSession}
            />
          ))}
        </div>
      </div>
    </div>
  );
}
