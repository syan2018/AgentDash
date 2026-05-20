import { useState } from "react";
import { Button, DangerConfirmDialog } from "@agentdash/ui";
import { useStoryStore } from "../../stores/storyStore";
import { useStoryViewStore } from "../../stores/storyViewStore";
import {
  PropertyPicker,
  type PropertyPickerOption,
} from "../../components/ui/property-picker";
import {
  StoryPriorityBadge,
  StoryStatusBadge,
} from "../../components/ui/status-badge";
import type { StoryPriority, StoryStatus } from "../../types";

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

export function StoryBulkToolbar() {
  const selectedIds = useStoryViewStore((s) => s.selectedIds);
  const clearSelection = useStoryViewStore((s) => s.clearSelection);
  const batchUpdateStories = useStoryStore((s) => s.batchUpdateStories);
  const batchDeleteStories = useStoryStore((s) => s.batchDeleteStories);
  const [busy, setBusy] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmValue, setConfirmValue] = useState("");

  if (selectedIds.size === 0) return null;
  const ids = Array.from(selectedIds);

  const handleStatus = async (status: StoryStatus) => {
    setBusy(true);
    try {
      await batchUpdateStories(ids, { status });
    } finally {
      setBusy(false);
    }
  };

  const handlePriority = async (priority: StoryPriority) => {
    setBusy(true);
    try {
      await batchUpdateStories(ids, { priority });
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async () => {
    if (confirmValue.trim() !== String(ids.length)) return;
    setBusy(true);
    try {
      await batchDeleteStories(ids);
      clearSelection();
      setConfirmOpen(false);
      setConfirmValue("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="pointer-events-none fixed inset-x-0 bottom-4 z-40 flex justify-center">
        <div className="pointer-events-auto flex items-center gap-2 rounded-[12px] border border-border bg-card px-3 py-2 shadow-lg shadow-foreground/10">
          <span className="rounded-[6px] bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
            已选 {selectedIds.size}
          </span>
          <PropertyPicker<StoryStatus>
            triggerLabel="批量改状态"
            value="draft"
            options={STATUS_OPTIONS}
            onChange={(next) => void handleStatus(next)}
            trigger={
              <span className="inline-flex h-7 items-center gap-1 rounded-[8px] border border-border bg-background px-2 text-xs text-foreground hover:bg-secondary/40">
                改状态
              </span>
            }
          />
          <PropertyPicker<StoryPriority>
            triggerLabel="批量改优先级"
            value="p2"
            options={PRIORITY_OPTIONS}
            onChange={(next) => void handlePriority(next)}
            trigger={
              <span className="inline-flex h-7 items-center gap-1 rounded-[8px] border border-border bg-background px-2 text-xs text-foreground hover:bg-secondary/40">
                改优先级
              </span>
            }
          />
          <Button
            type="button"
            size="sm"
            variant="danger"
            disabled={busy}
            onClick={() => {
              setConfirmValue("");
              setConfirmOpen(true);
            }}
          >
            删除
          </Button>
          <Button type="button" size="sm" variant="ghost" onClick={clearSelection}>
            取消
          </Button>
        </div>
      </div>

      <DangerConfirmDialog
        open={confirmOpen}
        title={`删除 ${ids.length} 个 Story`}
        description={`这将一并删除其下 Task。请输入数字 ${ids.length} 以确认。`}
        expectedValue={String(ids.length)}
        inputValue={confirmValue}
        onInputValueChange={setConfirmValue}
        confirmLabel="确认批量删除"
        onClose={() => {
          setConfirmOpen(false);
          setConfirmValue("");
        }}
        onConfirm={() => void handleDelete()}
      />
    </>
  );
}
