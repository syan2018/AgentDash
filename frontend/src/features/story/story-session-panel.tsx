import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { SessionBinding, Story } from "../../types";
import { useStoryStore, type CreateStorySessionInput } from "../../stores/storyStore";

interface StorySessionPanelProps {
  story: Story;
  showTitle?: boolean;
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

export function StorySessionPanel({ story, showTitle = true }: StorySessionPanelProps) {
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
        {showTitle ? <h4 className="text-sm font-medium text-foreground">伴随会话</h4> : <div />}
        <button
          type="button"
          onClick={() => setShowForm((v) => !v)}
          className="rounded-[10px] border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        >
          {showForm ? "取消" : "+ 新建会话"}
        </button>
      </div>

      {showForm && (
        <div className="space-y-2 rounded-[12px] border border-border bg-secondary/35 p-3">
          <input
            type="text"
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            placeholder="会话标题（可选）"
            className="agentdash-form-input"
          />
          <div className="flex items-center gap-2">
            <select
              value={newLabel}
              onChange={(e) => setNewLabel(e.target.value)}
              className="agentdash-form-select text-xs"
            >
              <option value="companion">伴随</option>
              <option value="planning">规划</option>
              <option value="review">评审</option>
            </select>
            <button
              type="button"
              disabled={isCreating}
              onClick={() => void handleCreate()}
              className="agentdash-button-primary text-xs"
            >
              {isCreating ? "创建中..." : "创建"}
            </button>
          </div>
        </div>
      )}

      {sessions.length === 0 ? (
        <div className="rounded-[12px] border border-dashed border-border bg-secondary/25 px-4 py-6 text-center text-xs text-muted-foreground">
          暂无伴随会话，点击上方按钮创建
        </div>
      ) : (
        <div className="space-y-2">
          {sessions.map((binding) => (
            <div
              key={binding.id}
              className="flex items-center gap-2 rounded-[10px] border border-border bg-background px-3 py-2.5 transition-colors hover:border-primary/20 hover:bg-secondary/25"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <span className="inline-flex rounded-full border border-border bg-secondary px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
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
                  className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                >
                  打开
                </button>
                <button
                  type="button"
                  onClick={() => void handleUnbind(binding)}
                  className="rounded-[8px] border border-destructive/20 bg-background px-2 py-1 text-[11px] text-destructive transition-colors hover:bg-destructive/10"
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
