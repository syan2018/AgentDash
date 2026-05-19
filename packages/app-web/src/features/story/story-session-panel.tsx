import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button, Select, TextInput } from "@agentdash/ui";
import type { SessionBinding, SessionNavigationState, Story } from "../../types";
import { useStoryStore, type CreateStorySessionInput } from "../../stores/storyStore";
import { useProjectStore } from "../../stores/projectStore";
import { SessionChatView } from "../session";

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
  const projects = useProjectStore((s) => s.projects);
  const sessions = useStoryStore((s) => s.sessionsByStoryId[story.id] ?? EMPTY_SESSIONS);
  const fetchStorySessions = useStoryStore((s) => s.fetchStorySessions);
  const createStorySession = useStoryStore((s) => s.createStorySession);
  const unbindStorySession = useStoryStore((s) => s.unbindStorySession);

  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [newLabel, setNewLabel] = useState("companion");
  const project = projects.find((item) => item.id === story.project_id);
  const workspaceId =
    story.default_workspace_id
    ?? project?.config.default_workspace_id
    ?? null;

  useEffect(() => {
    void fetchStorySessions(story.id);
  }, [fetchStorySessions, story.id]);

  const activeSessionExists = sessions.some((session) => session.session_id === activeSessionId);
  const resolvedActiveSessionId = activeSessionExists ? activeSessionId : sessions[0]?.session_id ?? null;

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

  const handleUnbind = useCallback(
    async (binding: SessionBinding) => {
      await unbindStorySession(story.id, binding.id);
      if (activeSessionId === binding.session_id) setActiveSessionId(null);
    },
    [story.id, unbindStorySession, activeSessionId],
  );

  const handleOpenFull = useCallback(() => {
    if (!resolvedActiveSessionId) return;
    const state: SessionNavigationState = {
      return_to: { owner_type: "story", story_id: story.id },
    };
    navigate(`/session/${resolvedActiveSessionId}`, { state });
  }, [navigate, resolvedActiveSessionId, story.id]);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex shrink-0 items-center gap-2 border-b border-border bg-secondary/20 px-4 py-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto">
          {sessions.length === 0 && !showCreateForm && (
            <span className="text-xs text-muted-foreground">暂无会话</span>
          )}
          {sessions.map((binding) => (
            <div
              key={binding.id}
              className={`group flex shrink-0 items-center rounded-[8px] border transition-colors ${
                resolvedActiveSessionId === binding.session_id
                  ? "border-primary/40 bg-primary/8 font-medium text-foreground"
                  : "border-border bg-background text-muted-foreground hover:border-primary/20 hover:text-foreground"
              }`}
            >
              <button
                type="button"
                onClick={() => setActiveSessionId(binding.session_id)}
                className="flex min-w-0 items-center gap-1.5 px-2.5 py-1.5 text-xs"
              >
                <span className="rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px]">
                  {labelText(binding.label)}
                </span>
                <span className="max-w-[120px] truncate">
                  {binding.session_title || binding.session_id.slice(0, 10) + "…"}
                </span>
              </button>
              <button
                type="button"
                onClick={() => void handleUnbind(binding)}
                className="mr-1 hidden rounded-[4px] p-0.5 text-muted-foreground transition-colors hover:text-destructive group-hover:inline-flex"
                title="解绑"
              >
                <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          ))}
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          {resolvedActiveSessionId && (
            <Button
              onClick={handleOpenFull}
              size="sm"
              variant="secondary"
              title="在独立会话页打开"
            >
              展开
            </Button>
          )}
          <Button
            onClick={() => setShowCreateForm((v) => !v)}
            size="sm"
            variant="secondary"
          >
            {showCreateForm ? "取消" : "+ 新建"}
          </Button>
        </div>
      </div>

      {showCreateForm && (
        <div className="flex shrink-0 items-center gap-2 border-b border-border bg-secondary/10 px-4 py-2">
          <TextInput
            type="text"
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            placeholder="会话标题（可选）"
            className="min-h-8 flex-1 py-1 text-xs"
            onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); void handleCreate(); } }}
          />
          <Select
            value={newLabel}
            onChange={(e) => setNewLabel(e.target.value)}
            className="min-h-8 py-1 text-xs"
          >
            <option value="companion">伴随</option>
            <option value="planning">规划</option>
            <option value="review">评审</option>
          </Select>
          <Button
            disabled={isCreating}
            onClick={() => void handleCreate()}
            size="sm"
            variant="primary"
          >
            {isCreating ? "…" : "创建"}
          </Button>
        </div>
      )}

      <div className="flex-1 overflow-hidden">
        <SessionChatView
          sessionId={resolvedActiveSessionId}
          workspaceId={workspaceId}
          onCreateSession={handleCreateFromChat}
          onSessionIdChange={handleSessionIdChange}
          onMessageSent={handleMessageSent}
        />
      </div>
    </div>
  );
}
