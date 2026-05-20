import { useCallback, useState } from "react";
import { useStoryStore } from "../../stores/storyStore";
import { useStoryViewStore } from "../../stores/storyViewStore";
import {
  StoryPriorityBadge,
  StoryStatusBadge,
  StoryStatusToken,
  StoryTypeBadge,
} from "../../components/ui/status-badge";
import {
  PropertyPicker,
  type PropertyPickerOption,
} from "../../components/ui/property-picker";
import type { Story, StoryPriority, StoryStatus, StoryType } from "../../types";

const STATUS_OPTIONS: PropertyPickerOption<StoryStatus>[] = [
  { value: "draft", label: "draft", preview: <StoryStatusBadge status="draft" /> },
  { value: "ready", label: "ready", preview: <StoryStatusBadge status="ready" /> },
  { value: "running", label: "running", preview: <StoryStatusBadge status="running" /> },
  { value: "review", label: "review", preview: <StoryStatusBadge status="review" /> },
  { value: "completed", label: "completed", preview: <StoryStatusBadge status="completed" /> },
  { value: "failed", label: "failed", preview: <StoryStatusBadge status="failed" /> },
  { value: "cancelled", label: "cancelled", preview: <StoryStatusBadge status="cancelled" /> },
];

const PRIORITY_OPTIONS: PropertyPickerOption<StoryPriority>[] = [
  { value: "p0", label: "P0 紧急", preview: <StoryPriorityBadge priority="p0" /> },
  { value: "p1", label: "P1 高", preview: <StoryPriorityBadge priority="p1" /> },
  { value: "p2", label: "P2 中", preview: <StoryPriorityBadge priority="p2" /> },
  { value: "p3", label: "P3 低", preview: <StoryPriorityBadge priority="p3" /> },
];

const TYPE_OPTIONS: PropertyPickerOption<StoryType>[] = [
  { value: "feature", label: "feature 功能", preview: <StoryTypeBadge type="feature" /> },
  { value: "bugfix", label: "bugfix 缺陷", preview: <StoryTypeBadge type="bugfix" /> },
  { value: "refactor", label: "refactor 重构", preview: <StoryTypeBadge type="refactor" /> },
  { value: "docs", label: "docs 文档", preview: <StoryTypeBadge type="docs" /> },
  { value: "test", label: "test 测试", preview: <StoryTypeBadge type="test" /> },
  { value: "other", label: "other 其他", preview: <StoryTypeBadge type="other" /> },
];

interface BaseProps {
  story: Story;
  align?: "start" | "end";
  onChange?: (story: Story) => void;
}

function usePickerExternalOpen(storyId: string, kind: "priority" | "status" | "type") {
  const externalRequested = useStoryViewStore(
    (s) => s.pendingPickerStoryId === storyId && s.pendingPickerKind === kind,
  );
  const clearRequest = useStoryViewStore((s) => s.clearPickerRequest);
  const [internalOpen, setInternalOpen] = useState(false);
  const open = internalOpen || externalRequested;

  const setOpen = useCallback(
    (next: boolean) => {
      setInternalOpen(next);
      if (!next && externalRequested) clearRequest();
    },
    [clearRequest, externalRequested],
  );

  return { open, setOpen };
}

export function EditableStatusBadge({
  story,
  align,
  onChange,
  variant = "badge",
  count,
}: BaseProps & { variant?: "badge" | "token"; count?: number }) {
  const updateStory = useStoryStore((s) => s.updateStory);
  const { open, setOpen } = usePickerExternalOpen(story.id, "status");
  return (
    <PropertyPicker<StoryStatus>
      triggerLabel={`status: ${story.status}`}
      trigger={
        variant === "token" ? (
          <StoryStatusToken status={story.status} count={count} />
        ) : (
          <StoryStatusBadge status={story.status} />
        )
      }
      value={story.status}
      options={STATUS_OPTIONS}
      align={align}
      open={open}
      onOpenChange={setOpen}
      onChange={async (next) => {
        if (next === story.status) return;
        const updated = await updateStory(story.id, { status: next });
        if (updated) onChange?.(updated);
      }}
    />
  );
}

export function EditablePriorityBadge({ story, align, onChange }: BaseProps) {
  const updateStory = useStoryStore((s) => s.updateStory);
  const { open, setOpen } = usePickerExternalOpen(story.id, "priority");
  return (
    <PropertyPicker<StoryPriority>
      triggerLabel={`priority: ${story.priority}`}
      trigger={<StoryPriorityBadge priority={story.priority} />}
      value={story.priority}
      options={PRIORITY_OPTIONS}
      align={align}
      open={open}
      onOpenChange={setOpen}
      onChange={async (next) => {
        if (next === story.priority) return;
        const updated = await updateStory(story.id, { priority: next });
        if (updated) onChange?.(updated);
      }}
    />
  );
}

export function EditableTypeBadge({ story, align, onChange }: BaseProps) {
  const updateStory = useStoryStore((s) => s.updateStory);
  const { open, setOpen } = usePickerExternalOpen(story.id, "type");
  return (
    <PropertyPicker<StoryType>
      triggerLabel={`type: ${story.story_type}`}
      trigger={<StoryTypeBadge type={story.story_type} />}
      value={story.story_type}
      options={TYPE_OPTIONS}
      align={align}
      open={open}
      onOpenChange={setOpen}
      onChange={async (next) => {
        if (next === story.story_type) return;
        const updated = await updateStory(story.id, { story_type: next });
        if (updated) onChange?.(updated);
      }}
    />
  );
}
