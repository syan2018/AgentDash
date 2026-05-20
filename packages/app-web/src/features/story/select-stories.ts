import type { Story, StoryPriority, StoryStatus, StoryType } from '../../types';
import type { StorySortKey, StoryScope } from '../../stores/storyViewStore';

const priorityWeight: Record<StoryPriority, number> = {
  p0: 0,
  p1: 1,
  p2: 2,
  p3: 3,
};

export interface SelectStoriesParams {
  search: string;
  scope: StoryScope;
  statusFilter: StoryStatus | 'all';
  priorityFilter: StoryPriority | 'all';
  typeFilter: StoryType | 'all';
  sort: StorySortKey;
}

export function selectFilteredStories(
  stories: Story[],
  params: SelectStoriesParams,
): Story[] {
  const keyword = params.search.trim().toLowerCase();
  const matchesKeyword = (story: Story): boolean => {
    if (!keyword) return true;
    const haystack = [story.title, story.description ?? '', ...story.tags]
      .join(' ')
      .toLowerCase();
    return haystack.includes(keyword);
  };

  const filtered = stories.filter((story) => {
    if (
      params.scope === 'active' &&
      (story.status === 'completed' || story.status === 'cancelled')
    ) {
      return false;
    }
    if (
      params.scope === 'done' &&
      story.status !== 'completed' &&
      story.status !== 'cancelled'
    ) {
      return false;
    }
    if (!matchesKeyword(story)) return false;
    if (params.statusFilter !== 'all' && story.status !== params.statusFilter) return false;
    if (params.priorityFilter !== 'all' && story.priority !== params.priorityFilter) return false;
    if (params.typeFilter !== 'all' && story.story_type !== params.typeFilter) return false;
    return true;
  });

  return [...filtered].sort((a, b) => {
    if (params.sort === 'priority') {
      const byPriority = priorityWeight[a.priority] - priorityWeight[b.priority];
      if (byPriority !== 0) return byPriority;
      return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
    }
    if (params.sort === 'title') return a.title.localeCompare(b.title);
    return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
  });
}

export function activeFilterCount(params: SelectStoriesParams): number {
  let count = 0;
  if (params.search.trim()) count += 1;
  if (params.statusFilter !== 'all') count += 1;
  if (params.priorityFilter !== 'all') count += 1;
  if (params.typeFilter !== 'all') count += 1;
  return count;
}
