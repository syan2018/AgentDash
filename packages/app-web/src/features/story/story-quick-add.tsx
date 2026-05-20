import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { StoryStatus } from "../../types";
import { useStoryStore } from "../../stores/storyStore";

interface StoryQuickAddProps {
  status: StoryStatus;
  projectId: string;
  onClose: () => void;
}

export function StoryQuickAdd({ status, projectId, onClose }: StoryQuickAddProps) {
  const createStory = useStoryStore((s) => s.createStory);
  const [title, setTitle] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleSubmit = async () => {
    const trimmed = title.trim();
    if (!trimmed) {
      onClose();
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const created = await createStory(projectId, trimmed, undefined, {
        status,
        priority: "p2",
        story_type: "feature",
        tags: [],
      });
      if (!created) {
        setError("创建失败，请重试");
        return;
      }
      setTitle("");
      inputRef.current?.focus();
    } finally {
      setSubmitting(false);
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void handleSubmit();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  };

  return (
    <div className="mb-2 rounded-[8px] border border-primary/30 bg-card p-2 shadow-sm">
      <input
        ref={inputRef}
        value={title}
        onChange={(event) => setTitle(event.target.value)}
        onKeyDown={handleKeyDown}
        onBlur={() => {
          if (!title.trim() && !submitting) onClose();
        }}
        placeholder={`新建 Story · ${status}`}
        disabled={submitting}
        className="w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
      />
      {error && <p className="mt-1 text-[10px] text-destructive">{error}</p>}
      <p className="mt-1 text-[10px] text-muted-foreground">
        Enter 创建 · Esc 取消 · 标题为空时自动收起
      </p>
    </div>
  );
}
