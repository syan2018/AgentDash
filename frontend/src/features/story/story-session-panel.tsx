import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { SessionBinding, Story } from "../../types";
import { useStoryStore, type CreateStorySessionInput } from "../../stores/storyStore";

interface StorySessionPanelProps {
  story: Story;
}

function formatTimestamp(ts: number | undefined): string {
  if (ts == null) return "-";
  const d = new Date(ts);
  return d.toLocaleString();
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

  const [isCreating, setIsCreating] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [newLabel, setNewLabel] = useState("companion");
  const [showForm, setShowForm] = useState(false);

  useEffect(() => {
    void fetchStorySessions(story.id);
  }, [fetchStorySessions, story.id]);

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
        setNewTitle("");
        setShowForm(false);
      }
    } finally {
      setIsCreating(false);
    }
  }, [createStorySession, isCreating, newLabel, newTitle, story.id]);

  const handleUnbind = useCallback(
    async (binding: SessionBinding) => {
      await unbindStorySession(story.id, binding.id);
    },
    [story.id, unbindStorySession],
  );

  const handleNavigate = useCallback(
    (sessionId: string) => {
      navigate(`/session/${sessionId}`, {
        state: {
          return_to: { story_id: story.id },
        },
      });
    },
    [navigate, story.id],
  );

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium text-foreground">伴随会话</h4>
        <button
          type="button"
          onClick={() => setShowForm((v) => !v)}
          className="rounded border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-muted"
        >
          {showForm ? "取消" : "+ 新建会话"}
        </button>
      </div>

      {showForm && (
        <div className="space-y-2 rounded-md border border-border bg-muted/30 p-3">
          <input
            type="text"
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            placeholder="会话标题（可选）"
            className="w-full rounded border border-border bg-background px-2.5 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <div className="flex items-center gap-2">
            <select
              value={newLabel}
              onChange={(e) => setNewLabel(e.target.value)}
              className="rounded border border-border bg-background px-2 py-1.5 text-xs outline-none ring-ring focus:ring-1"
            >
              <option value="companion">伴随</option>
              <option value="planning">规划</option>
              <option value="review">评审</option>
            </select>
            <button
              type="button"
              disabled={isCreating}
              onClick={() => void handleCreate()}
              className="rounded bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground disabled:opacity-50"
            >
              {isCreating ? "创建中..." : "创建"}
            </button>
          </div>
        </div>
      )}

      {sessions.length === 0 ? (
        <div className="rounded-md border border-dashed border-border px-4 py-6 text-center text-xs text-muted-foreground">
          暂无伴随会话，点击上方按钮创建
        </div>
      ) : (
        <div className="space-y-1.5">
          {sessions.map((binding) => (
            <div
              key={binding.id}
              className="flex items-center gap-2 rounded-md border border-border bg-background px-3 py-2 transition-colors hover:border-primary/40"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <span className="inline-flex rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                    {labelText(binding.label)}
                  </span>
                  <span className="truncate text-sm text-foreground">
                    {binding.session_title || binding.session_id.slice(0, 16) + "..."}
                  </span>
                </div>
                <p className="mt-0.5 text-[11px] text-muted-foreground">
                  {formatTimestamp(binding.session_updated_at)}
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-1">
                <button
                  type="button"
                  onClick={() => handleNavigate(binding.session_id)}
                  className="rounded border border-border px-2 py-1 text-[11px] text-foreground hover:bg-muted"
                >
                  打开
                </button>
                <button
                  type="button"
                  onClick={() => void handleUnbind(binding)}
                  className="rounded border border-border px-2 py-1 text-[11px] text-destructive hover:bg-destructive/10"
                >
                  解绑
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
