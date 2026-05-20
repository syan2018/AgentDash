import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { useNavigate } from "react-router-dom";
import { useStoryStore } from "../../stores/storyStore";
import { useStoryViewStore } from "../../stores/storyViewStore";
import type { Story } from "../../types";
import { StoryStatusBadge, StoryPriorityBadge } from "../../components/ui/status-badge";

function formatStoryKey(id: string): string {
  return `ST-${id.slice(0, 4).toUpperCase()}`;
}

function fuzzyMatch(haystack: string, needle: string): boolean {
  if (!needle) return true;
  const h = haystack.toLowerCase();
  const n = needle.toLowerCase();
  if (h.includes(n)) return true;
  let i = 0;
  for (const ch of h) {
    if (ch === n[i]) i += 1;
    if (i === n.length) return true;
  }
  return false;
}

interface QuickJumpProps {
  projectId: string | null;
}

export function StoryQuickJump({ projectId }: QuickJumpProps) {
  const navigate = useNavigate();
  const open = useStoryViewStore((s) => s.isQuickJumpOpen);
  const setOpen = useStoryViewStore((s) => s.setQuickJumpOpen);
  const stories = useStoryStore((s) =>
    projectId ? s.storiesByProjectId[projectId] ?? [] : [],
  );

  const [query, setQuery] = useState("");
  const [highlight, setHighlight] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const composingRef = useRef(false);
  const [openSnapshot, setOpenSnapshot] = useState(open);

  if (open !== openSnapshot) {
    setOpenSnapshot(open);
    if (open) {
      setQuery("");
      setHighlight(0);
    }
  }

  useEffect(() => {
    if (open) {
      const t = window.setTimeout(() => inputRef.current?.focus(), 0);
      return () => window.clearTimeout(t);
    }
  }, [open]);

  const matches = useMemo(() => {
    const trimmed = query.trim();
    const list = stories.filter((story) => {
      const haystack = `${formatStoryKey(story.id)} ${story.title} ${story.description ?? ""} ${story.tags.join(" ")}`;
      return fuzzyMatch(haystack, trimmed);
    });
    return list.slice(0, 50);
  }, [query, stories]);

  const effectiveHighlight =
    matches.length === 0 ? 0 : Math.min(highlight, matches.length - 1);

  if (!open) return null;

  const handleKeyDown = (event: ReactKeyboardEvent) => {
    if (composingRef.current) return;
    if (event.key === "Escape") {
      event.preventDefault();
      setOpen(false);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (matches.length === 0) return;
      setHighlight((effectiveHighlight + 1) % matches.length);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (matches.length === 0) return;
      setHighlight((effectiveHighlight - 1 + matches.length) % matches.length);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const target = matches[effectiveHighlight];
      if (target) jumpTo(target);
    }
  };

  const jumpTo = (story: Story) => {
    setOpen(false);
    navigate(`/story/${story.id}`);
  };

  return (
    <div
      className="fixed inset-0 z-[60] flex items-start justify-center bg-foreground/30 px-4 pt-24 backdrop-blur-sm"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) setOpen(false);
      }}
    >
      <div
        className="w-full max-w-xl overflow-hidden rounded-[12px] border border-border bg-card shadow-xl"
        onKeyDown={handleKeyDown}
      >
        <div className="border-b border-border px-3 py-2">
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onCompositionStart={() => {
              composingRef.current = true;
            }}
            onCompositionEnd={() => {
              composingRef.current = false;
            }}
            placeholder="跳转 Story · 输入标题、key 或描述"
            className="h-9 w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
          />
        </div>
        <ul className="max-h-80 overflow-y-auto p-1">
          {matches.length === 0 ? (
            <li className="px-3 py-8 text-center text-xs text-muted-foreground">
              {stories.length === 0 ? "当前 Project 暂无 Story" : "无匹配"}
            </li>
          ) : (
            matches.map((story, idx) => (
              <li
                key={story.id}
                onMouseEnter={() => setHighlight(idx)}
                onClick={() => jumpTo(story)}
                className={`flex cursor-pointer items-center gap-3 rounded-[8px] px-3 py-2 ${
                  idx === effectiveHighlight ? "bg-secondary/60" : ""
                }`}
              >
                <span className="font-mono text-[11px] text-muted-foreground">
                  {formatStoryKey(story.id)}
                </span>
                <span className="min-w-0 flex-1 truncate text-sm text-foreground">{story.title}</span>
                <StoryPriorityBadge priority={story.priority} />
                <StoryStatusBadge status={story.status} />
              </li>
            ))
          )}
        </ul>
        <div className="flex items-center justify-between border-t border-border bg-secondary/20 px-3 py-1.5 text-[10px] text-muted-foreground">
          <span>↑↓ 切换 · Enter 跳转 · Esc 关闭</span>
          <span>{matches.length} / {stories.length}</span>
        </div>
      </div>
    </div>
  );
}
