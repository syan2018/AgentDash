/**
 * Story 内嵌会话面板
 *
 * 顶部为会话选择器栏（切换/创建/解绑），
 * 下方复用 SessionChatView 提供完整聊天能力。
 */

import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { SessionBinding, SessionNavigationState, Story } from "../../types";
import { useStoryStore, type CreateStorySessionInput } from "../../stores/storyStore";
import { SessionChatView } from "../acp-session";

interface StorySessionPanelProps {
  story: Story;
}

const LABEL_DISPLAY: Record<string, string> = {
  companion: "伴随",
  planning: "规划",
  review: "评审",
  execution: "执行",
};

function labelText(label: string): string {
  return (LABEL_DISPLAY[label] ?? label) || "通用";
}

const EMPTY_SESSIONS: SessionBinding[] = [];

export function StorySessionPanel({ story }: StorySessionPanelProps) {
  const navigate = useNavigate();
  const sessions = useStoryStore((s) => s.sessionsByStoryId[story.id] ?? EMPTY_SESSIONS);
  const fetchStorySessions = useStoryStore((s) => s.fetchStorySessions);
  const createStorySession = useStoryStore((s) => s.createStorySession);
  const unbindStorySession = useStoryStore((s) => s.unbindStorySession);

  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [newLabel, setNewLabel] = useState("companion");

  useEffect(() => {
    void fetchStorySessions(story.id);
  }, [fetchStorySessions, story.id]);

  // 自动选中第一个会话
  useEffect(() => {
    if (activeSessionId) {
      const stillExists = sessions.some((s) => s.session_id === activeSessionId);
      if (stillExists) return;
    }
    setActiveSessionId(sessions.length > 0 ? sessions[0].session_id : null);
  }, [sessions, activeSessionId]);

  // storyId 变化时重置
  useEffect(() => {
    setActiveSessionId(null);
    setShowCreateForm(false);
  }, [story.id]);

  // 创建新会话
  const handleCreate = useCallback(async () => {
    if (isCreating) return;
    setIsCreating(true);
    try {
      const input: CreateStorySessionInput = {
        title: newTitle.trim() || undefined,
        label: newLabel || undefined,
      };
      const result = await createStorySession(story.id, input);
      if (result) {
        setActiveSessionId(result.session_id);
        setNewTitle("");
        setShowCreateForm(false);
      }
    } finally {
      setIsCreating(false);
    }
  }, [createStorySession, isCreating, newLabel, newTitle, story.id]);

  // 通过 ChatView 的 onCreateSession 回调创建会话（用户在无会话时直接发消息）
  const handleCreateFromChat = useCallback(async (title: string) => {
    const result = await createStorySession(story.id, { title, label: "companion" });
    if (!result) throw new Error("创建会话失败");
    return result.session_id;
  }, [createStorySession, story.id]);

  const handleSessionIdChange = useCallback((id: string) => {
    setActiveSessionId(id);
  }, []);

  const handleMessageSent = useCallback(() => {
    void fetchStorySessions(story.id);
  }, [fetchStorySessions, story.id]);

  // 解绑会话
  const handleUnbind = useCallback(
    async (binding: SessionBinding) => {
      await unbindStorySession(story.id, binding.id);
      if (activeSessionId === binding.session_id) setActiveSessionId(null);
    },
    [story.id, unbindStorySession, activeSessionId],
  );

  // 在独立会话页打开
  const handleOpenFull = useCallback(() => {
    if (!activeSessionId) return;
    const state: SessionNavigationState = {
      return_to: { owner_type: "story", story_id: story.id },
    };
    navigate(`/session/${activeSessionId}`, { state });
  }, [activeSessionId, navigate, story.id]);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* 会话选择器栏 */}
      <div className="flex shrink-0 items-center gap-2 border-b border-border bg-secondary/20 px-4 py-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto">
          {sessions.length === 0 && !showCreateForm && (
            <span className="text-xs text-muted-foreground">暂无会话</span>
          )}
          {sessions.map((binding) => (
            <button
              key={binding.id}
              type="button"
              onClick={() => setActiveSessionId(binding.session_id)}
              className={`group relative flex shrink-0 items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-xs transition-colors ${
                activeSessionId === binding.session_id
                  ? "border-primary/40 bg-primary/8 font-medium text-foreground"
                  : "border-border bg-background text-muted-foreground hover:border-primary/20 hover:text-foreground"
              }`}
            >
              <span className="rounded-full border border-border bg-secondary px-1.5 py-0.5 text-[10px]">
                {labelText(binding.label)}
              </span>
              <span className="max-w-[120px] truncate">
                {binding.session_title || binding.session_id.slice(0, 10) + "…"}
              </span>
              <button
                type="button"
                onClick={(e) => { e.stopPropagation(); void handleUnbind(binding); }}
                className="ml-0.5 hidden rounded p-0.5 text-muted-foreground hover:text-destructive group-hover:inline-flex"
                title="解绑"
              >
                <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </button>
          ))}
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          {activeSessionId && (
            <button
              type="button"
              onClick={handleOpenFull}
              className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
              title="在独立会话页打开"
            >
              展开
            </button>
          )}
          <button
            type="button"
            onClick={() => setShowCreateForm((v) => !v)}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            {showCreateForm ? "取消" : "+ 新建"}
          </button>
        </div>
      </div>

      {/* 创建表单（内联） */}
      {showCreateForm && (
        <div className="flex shrink-0 items-center gap-2 border-b border-border bg-secondary/10 px-4 py-2">
          <input
            type="text"
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            placeholder="会话标题（可选）"
            className="h-7 flex-1 rounded-[8px] border border-border bg-background px-2 text-xs outline-none ring-ring focus:ring-1"
            onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); void handleCreate(); } }}
          />
          <select
            value={newLabel}
            onChange={(e) => setNewLabel(e.target.value)}
            className="h-7 rounded-[8px] border border-border bg-background px-2 text-xs"
          >
            <option value="companion">伴随</option>
            <option value="planning">规划</option>
            <option value="review">评审</option>
          </select>
          <button
            type="button"
            disabled={isCreating}
            onClick={() => void handleCreate()}
            className="h-7 rounded-[8px] border border-primary bg-primary px-3 text-xs font-medium text-primary-foreground transition-colors hover:opacity-95 disabled:opacity-50"
          >
            {isCreating ? "…" : "创建"}
          </button>
        </div>
      )}

      {/* 复用的聊天视图 */}
      <div className="flex-1 overflow-hidden">
        <SessionChatView
          sessionId={activeSessionId}
          onCreateSession={handleCreateFromChat}
          onSessionIdChange={handleSessionIdChange}
          onMessageSent={handleMessageSent}
        />
      </div>
    </div>
  );
}
