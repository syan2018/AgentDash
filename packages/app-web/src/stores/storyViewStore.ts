import { create } from 'zustand';
import type { StoryPriority, StoryStatus, StoryType } from '../types';

export type StoryViewMode = 'board' | 'list';
export type StorySortKey = 'priority' | 'updated' | 'title';
export type StoryScope = 'all' | 'active' | 'done';

interface StoryViewState {
  search: string;
  scope: StoryScope;
  statusFilter: StoryStatus | 'all';
  priorityFilter: StoryPriority | 'all';
  typeFilter: StoryType | 'all';
  sort: StorySortKey;
  viewMode: StoryViewMode;
  selectedIds: Set<string>;
  isCreateOpen: boolean;
  createInitialStatus: StoryStatus;
  isQuickJumpOpen: boolean;
  quickAddColumn: StoryStatus | null;
  focusedStoryId: string | null;
  pendingPickerStoryId: string | null;
  pendingPickerKind: 'priority' | 'status' | 'type' | null;

  setSearch: (v: string) => void;
  setScope: (v: StoryScope) => void;
  setStatusFilter: (v: StoryStatus | 'all') => void;
  setPriorityFilter: (v: StoryPriority | 'all') => void;
  setTypeFilter: (v: StoryType | 'all') => void;
  setSort: (v: StorySortKey) => void;
  setViewMode: (v: StoryViewMode) => void;
  clearFilters: () => void;
  toggleSelect: (id: string) => void;
  setSelected: (ids: string[]) => void;
  clearSelection: () => void;
  openCreate: (status?: StoryStatus) => void;
  closeCreate: () => void;
  setQuickJumpOpen: (v: boolean) => void;
  openQuickAddColumn: (status: StoryStatus | null) => void;
  setFocusedStory: (id: string | null) => void;
  requestPicker: (storyId: string, kind: 'priority' | 'status' | 'type') => void;
  clearPickerRequest: () => void;
}

export const useStoryViewStore = create<StoryViewState>((set) => ({
  search: '',
  scope: 'all',
  statusFilter: 'all',
  priorityFilter: 'all',
  typeFilter: 'all',
  sort: 'priority',
  viewMode: 'board',
  selectedIds: new Set<string>(),
  isCreateOpen: false,
  createInitialStatus: 'created',
  isQuickJumpOpen: false,
  quickAddColumn: null,
  focusedStoryId: null,
  pendingPickerStoryId: null,
  pendingPickerKind: null,

  setSearch: (search) => set({ search }),
  setScope: (scope) => set({ scope }),
  setStatusFilter: (statusFilter) => set({ statusFilter }),
  setPriorityFilter: (priorityFilter) => set({ priorityFilter }),
  setTypeFilter: (typeFilter) => set({ typeFilter }),
  setSort: (sort) => set({ sort }),
  setViewMode: (viewMode) => set({ viewMode }),
  clearFilters: () =>
    set({ search: '', statusFilter: 'all', priorityFilter: 'all', typeFilter: 'all' }),
  toggleSelect: (id) =>
    set((state) => {
      const next = new Set(state.selectedIds);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return { selectedIds: next };
    }),
  setSelected: (ids) => set({ selectedIds: new Set(ids) }),
  clearSelection: () => set({ selectedIds: new Set<string>() }),
  openCreate: (status) =>
    set((state) => ({
      isCreateOpen: true,
      createInitialStatus:
        status ?? (state.statusFilter === 'all' ? 'created' : (state.statusFilter as StoryStatus)),
    })),
  closeCreate: () => set({ isCreateOpen: false }),
  setQuickJumpOpen: (v) => set({ isQuickJumpOpen: v }),
  openQuickAddColumn: (status) => set({ quickAddColumn: status }),
  setFocusedStory: (id) => set({ focusedStoryId: id }),
  requestPicker: (storyId, kind) =>
    set({ pendingPickerStoryId: storyId, pendingPickerKind: kind }),
  clearPickerRequest: () =>
    set({ pendingPickerStoryId: null, pendingPickerKind: null }),
}));
