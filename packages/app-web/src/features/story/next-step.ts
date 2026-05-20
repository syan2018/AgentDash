import type { StoryStatus } from '../../types';

export const STORY_NEXT_STEP: Partial<Record<StoryStatus, { to: StoryStatus; label: string }>> = {
  draft: { to: 'ready', label: '标记就绪' },
  ready: { to: 'running', label: '开始执行' },
  running: { to: 'review', label: '提交验收' },
  review: { to: 'completed', label: '验收通过' },
};

export function getStoryNextStep(status: StoryStatus): { to: StoryStatus; label: string } | null {
  return STORY_NEXT_STEP[status] ?? null;
}
