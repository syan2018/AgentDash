import { Button } from "@agentdash/ui";
import type { StoryPriority, StoryStatus, StoryType } from "../../types";
import {
  useStoryViewStore,
  type StorySortKey,
  type StoryScope,
  type StoryViewMode,
} from "../../stores/storyViewStore";
import { Tooltip } from "../../components/ui/tooltip";

const statusOptions: { value: StoryStatus; label: string }[] = [
  { value: "created", label: "created" },
  { value: "context_ready", label: "context_ready" },
  { value: "executing", label: "executing" },
  { value: "decomposed", label: "decomposed" },
  { value: "completed", label: "completed" },
  { value: "failed", label: "failed" },
  { value: "cancelled", label: "cancelled" },
];

const priorityOptions: { value: StoryPriority; label: string }[] = [
  { value: "p0", label: "p0" },
  { value: "p1", label: "p1" },
  { value: "p2", label: "p2" },
  { value: "p3", label: "p3" },
];

const storyTypeOptions: { value: StoryType; label: string }[] = [
  { value: "feature", label: "feature" },
  { value: "bugfix", label: "bugfix" },
  { value: "refactor", label: "refactor" },
  { value: "docs", label: "docs" },
  { value: "test", label: "test" },
  { value: "other", label: "other" },
];

const scopeOptions: { value: StoryScope; label: string }[] = [
  { value: "all", label: "All" },
  { value: "active", label: "Active" },
  { value: "done", label: "Done" },
];

const sortOptions: { value: StorySortKey; label: string }[] = [
  { value: "priority", label: "priority" },
  { value: "updated", label: "updated" },
  { value: "title", label: "title" },
];

function ChevronIcon() {
  return (
    <svg
      className="h-3.5 w-3.5 text-muted-foreground"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function SearchIcon() {
  return (
    <svg
      className="h-3.5 w-3.5 text-muted-foreground"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
    >
      <circle cx="11" cy="11" r="8" />
      <path d="m21 21-4.3-4.3" />
    </svg>
  );
}

function BoardIcon() {
  return (
    <svg
      className="h-3.5 w-3.5"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
    >
      <rect x="3" y="4" width="5" height="16" rx="1.5" />
      <rect x="10" y="4" width="5" height="16" rx="1.5" />
      <rect x="17" y="4" width="4" height="16" rx="1.5" />
    </svg>
  );
}

function ListIcon() {
  return (
    <svg
      className="h-3.5 w-3.5"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
    >
      <path d="M8 6h13" />
      <path d="M8 12h13" />
      <path d="M8 18h13" />
      <path d="M3 6h.01" />
      <path d="M3 12h.01" />
      <path d="M3 18h.01" />
    </svg>
  );
}

function ToolbarSelect<T extends string>({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: T | "all";
  onChange: (value: T | "all") => void;
  options: { value: T; label: string }[];
}) {
  const selectedLabel =
    value === "all"
      ? "all"
      : options.find((option) => option.value === value)?.label ?? value;
  const active = value !== "all";

  return (
    <label
      className={`relative inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[8px] border px-2.5 text-xs transition-colors ${
        active
          ? "border-primary/30 bg-primary/5 text-foreground"
          : "border-border bg-background text-muted-foreground hover:bg-secondary/30 hover:text-foreground"
      }`}
    >
      <span className="font-medium">{label}</span>
      <span className="font-mono text-[11px]">{selectedLabel}</span>
      <ChevronIcon />
      <select
        value={value}
        onChange={(event) => onChange(event.target.value as T | "all")}
        className="absolute inset-0 cursor-pointer opacity-0"
      >
        <option value="all">all</option>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

function SortSelect() {
  const sort = useStoryViewStore((s) => s.sort);
  const setSort = useStoryViewStore((s) => s.setSort);
  const selectedLabel =
    sortOptions.find((option) => option.value === sort)?.label ?? sort;

  return (
    <label className="relative inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 text-xs text-muted-foreground transition-colors hover:bg-secondary/30 hover:text-foreground">
      <span className="font-medium">Sort</span>
      <span className="font-mono text-[11px]">{selectedLabel}</span>
      <ChevronIcon />
      <select
        value={sort}
        onChange={(event) => setSort(event.target.value as StorySortKey)}
        className="absolute inset-0 cursor-pointer opacity-0"
      >
        {sortOptions.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

interface StoryToolbarProps {
  filterCount: number;
  hasFilters: boolean;
}

export function StoryToolbar({ filterCount, hasFilters }: StoryToolbarProps) {
  const search = useStoryViewStore((s) => s.search);
  const setSearch = useStoryViewStore((s) => s.setSearch);
  const scope = useStoryViewStore((s) => s.scope);
  const setScope = useStoryViewStore((s) => s.setScope);
  const statusFilter = useStoryViewStore((s) => s.statusFilter);
  const setStatusFilter = useStoryViewStore((s) => s.setStatusFilter);
  const priorityFilter = useStoryViewStore((s) => s.priorityFilter);
  const setPriorityFilter = useStoryViewStore((s) => s.setPriorityFilter);
  const typeFilter = useStoryViewStore((s) => s.typeFilter);
  const setTypeFilter = useStoryViewStore((s) => s.setTypeFilter);
  const viewMode = useStoryViewStore((s) => s.viewMode);
  const setViewMode = useStoryViewStore((s) => s.setViewMode);
  const clearFilters = useStoryViewStore((s) => s.clearFilters);

  return (
    <div className="flex min-h-12 items-center justify-between gap-3 border-t border-border px-4 py-2">
      <div className="flex min-w-0 items-center gap-1">
        {scopeOptions.map((option) => (
          <button
            key={option.value}
            type="button"
            onClick={() => setScope(option.value)}
            className={`h-8 rounded-[8px] border px-3 text-xs font-medium transition-colors ${
              scope === option.value
                ? "border-border bg-secondary/60 text-foreground"
                : "border-border bg-background text-muted-foreground hover:bg-secondary/30 hover:text-foreground"
            }`}
          >
            {option.label}
          </button>
        ))}
      </div>

      <div className="flex min-w-0 flex-1 items-center justify-end gap-1.5">
        <label className="flex h-8 w-56 min-w-40 items-center gap-2 rounded-[8px] border border-border bg-background px-2.5 text-xs text-muted-foreground transition-colors focus-within:border-primary/30 focus-within:ring-1 focus-within:ring-ring">
          <SearchIcon />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search"
            className="min-w-0 flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
          />
        </label>
        <ToolbarSelect label="status" value={statusFilter} onChange={setStatusFilter} options={statusOptions} />
        <ToolbarSelect label="priority" value={priorityFilter} onChange={setPriorityFilter} options={priorityOptions} />
        <ToolbarSelect label="type" value={typeFilter} onChange={setTypeFilter} options={storyTypeOptions} />
        {hasFilters && (
          <Button type="button" variant="ghost" size="sm" onClick={clearFilters}>
            Clear {filterCount}
          </Button>
        )}
        <SortSelect />
        <div className="flex h-8 rounded-[8px] border border-border bg-background p-0.5">
          <ViewModeButton mode="board" current={viewMode} onSelect={setViewMode}>
            <BoardIcon />
          </ViewModeButton>
          <ViewModeButton mode="list" current={viewMode} onSelect={setViewMode}>
            <ListIcon />
          </ViewModeButton>
        </div>
      </div>
    </div>
  );
}

function ViewModeButton({
  mode,
  current,
  onSelect,
  children,
}: {
  mode: StoryViewMode;
  current: StoryViewMode;
  onSelect: (mode: StoryViewMode) => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip content={mode === "board" ? "看板视图" : "列表视图"} side="bottom">
      <button
        type="button"
        title={mode === "board" ? "Board" : "List"}
        onClick={() => onSelect(mode)}
        className={`inline-flex h-6 w-7 items-center justify-center rounded-[6px] transition-colors ${
          current === mode ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"
        }`}
      >
        {children}
      </button>
    </Tooltip>
  );
}
