import type { StoryStatus } from '../../types';

export const STORY_NEXT_STEP: Partial<Record<StoryStatus, { to: StoryStatus; label: string }>> = {
  created: { to: 'context_ready', label: '标记就绪' },
  context_ready: { to: 'executing', label: '开始执行' },
  executing: { to: 'decomposed', label: '提交验收' },
  decomposed: { to: 'completed', label: '验收通过' },
};

export function getStoryNextStep(status: StoryStatus): { to: StoryStatus; label: string } | null {
  return STORY_NEXT_STEP[status] ?? null;
}
