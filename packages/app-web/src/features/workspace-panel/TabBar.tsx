/**
 * 动态 Tab 栏
 *
 * 左侧为钉选 Tab（不可关闭、不可拖拽），右侧为动态 Tab（可关闭、可拖拽排序）。
 * 使用 dnd-kit 实现拖拽排序。
 */

import { useCallback, useMemo } from "react";
import {
  DndContext,
  PointerSensor,
  KeyboardSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  horizontalListSortingStrategy,
  useSortable,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  type TabInstance,
  type TabTypeDescriptor,
} from "./tab-type-registry";
import { AddTabMenu } from "./AddTabMenu";
import type { CanvasModuleOpenOption } from "./model/canvasModuleOpen";

// ─── Props ──────────────────────────────────────────────

interface TabBarProps {
  tabs: TabInstance[];
  tabTypes: TabTypeDescriptor[];
  activeTabId: string | null;
  onActivate: (tabId: string) => void;
  onClose: (tabId: string) => void;
  onReorder: (fromIndex: number, toIndex: number) => void;
  onAddTab: (typeId: string) => void;
  canvasOptions: CanvasModuleOpenOption[];
  canvasOptionsStatus: "idle" | "loading" | "ready" | "refreshing" | "error";
  canvasOpenBusyKey: string | null;
  canvasOpenError: string | null;
  onOpenCanvasModule: (option: CanvasModuleOpenOption) => Promise<boolean>;
}

// ─── TabBar ─────────────────────────────────────────────

export function TabBar({
  tabs,
  tabTypes,
  activeTabId,
  onActivate,
  onClose,
  onReorder,
  onAddTab,
  canvasOptions,
  canvasOptionsStatus,
  canvasOpenBusyKey,
  canvasOpenError,
  onOpenCanvasModule,
}: TabBarProps) {
  const pinnedTabs = useMemo(() => tabs.filter((t) => t.pinned), [tabs]);
  const dynamicTabs = useMemo(() => tabs.filter((t) => !t.pinned), [tabs]);
  const dynamicIds = useMemo(() => dynamicTabs.map((t) => t.id), [dynamicTabs]);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event;
      if (!over || active.id === over.id) return;

      const fromIndex = tabs.findIndex((t) => t.id === active.id);
      const toIndex = tabs.findIndex((t) => t.id === over.id);
      if (fromIndex >= 0 && toIndex >= 0) {
        onReorder(fromIndex, toIndex);
      }
    },
    [tabs, onReorder],
  );

  return (
    <div className="flex shrink-0 items-center border-b border-border bg-secondary/20 px-1.5 py-1">
      {/* 可滚动的 Tab 列表区域 */}
      <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
        {/* 钉选 Tab */}
        {pinnedTabs.map((tab) => (
          <TabButton
            key={tab.id}
            tab={tab}
            tabTypes={tabTypes}
            isActive={tab.id === activeTabId}
            onActivate={onActivate}
            onClose={onClose}
          />
        ))}

        {pinnedTabs.length > 0 && dynamicTabs.length > 0 && (
          <div className="mx-0.5 h-4 w-px shrink-0 bg-border/50" />
        )}

        {/* 动态 Tab（可拖拽） */}
        <DndContext sensors={sensors} onDragEnd={handleDragEnd}>
          <SortableContext items={dynamicIds} strategy={horizontalListSortingStrategy}>
            {dynamicTabs.map((tab) => (
              <SortableTabButton
                key={tab.id}
                tab={tab}
                tabTypes={tabTypes}
                isActive={tab.id === activeTabId}
                onActivate={onActivate}
                onClose={onClose}
              />
            ))}
          </SortableContext>
        </DndContext>
      </div>

      {/* "+" 新建菜单（放在 overflow 容器之外，避免下拉被裁剪） */}
      <AddTabMenu
        tabTypes={tabTypes}
        onAddTab={onAddTab}
        canvasOptions={canvasOptions}
        canvasOptionsStatus={canvasOptionsStatus}
        canvasOpenBusyKey={canvasOpenBusyKey}
        canvasOpenError={canvasOpenError}
        onOpenCanvasModule={onOpenCanvasModule}
      />
    </div>
  );
}

// ─── 普通 Tab 按钮（钉选用） ───────────────────────────

function TabButton({
  tab,
  tabTypes,
  isActive,
  onActivate,
  onClose,
}: {
  tab: TabInstance;
  tabTypes: TabTypeDescriptor[];
  isActive: boolean;
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
}) {
  const type = tabTypes.find((descriptor) => descriptor.typeId === tab.typeId);
  const Icon = type?.icon;

  return (
    <div
      className={`group flex shrink-0 items-center gap-1 rounded-[7px] px-2 py-1.5 text-xs transition-colors ${
        isActive
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:bg-background/60 hover:text-foreground"
      }`}
    >
      <button
        type="button"
        onClick={() => onActivate(tab.id)}
        className="flex items-center gap-1.5"
      >
        {Icon && <Icon className="h-3 w-3" />}
        <span className="max-w-[100px] truncate font-medium">{tab.title}</span>
      </button>
      {!tab.pinned && (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onClose(tab.id);
          }}
          className="ml-0.5 hidden h-4 w-4 items-center justify-center rounded-[4px] text-muted-foreground/60 transition-colors hover:bg-destructive/10 hover:text-destructive group-hover:flex"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
          </svg>
        </button>
      )}
    </div>
  );
}

// ─── 可拖拽 Tab 按钮（动态 Tab 用） ───────────────────

function SortableTabButton({
  tab,
  tabTypes,
  isActive,
  onActivate,
  onClose,
}: {
  tab: TabInstance;
  tabTypes: TabTypeDescriptor[];
  isActive: boolean;
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: tab.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  const type = tabTypes.find((descriptor) => descriptor.typeId === tab.typeId);
  const Icon = type?.icon;

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={`group flex shrink-0 items-center gap-1 rounded-[7px] px-2 py-1.5 text-xs transition-colors ${
        isDragging
          ? "z-10 opacity-80 shadow-md ring-1 ring-primary/30"
          : ""
      } ${
        isActive
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:bg-background/60 hover:text-foreground"
      }`}
      {...attributes}
      {...listeners}
    >
      <button
        type="button"
        onClick={() => onActivate(tab.id)}
        className="flex items-center gap-1.5"
      >
        {Icon && <Icon className="h-3 w-3" />}
        <span className="max-w-[100px] truncate font-medium">{tab.title}</span>
      </button>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onClose(tab.id);
        }}
        className="ml-0.5 hidden h-4 w-4 items-center justify-center rounded-[4px] text-muted-foreground/60 transition-colors hover:bg-destructive/10 hover:text-destructive group-hover:flex"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <path d="M18 6 6 18" />
          <path d="m6 6 12 12" />
        </svg>
      </button>
    </div>
  );
}
