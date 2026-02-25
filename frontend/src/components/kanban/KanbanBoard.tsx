import type { Story, StoryStatus } from '../../types';
import { StoryCard } from './StoryCard';

const COLUMNS: { status: StoryStatus; title: string }[] = [
  { status: 'created', title: '待处理' },
  { status: 'context_ready', title: '上下文就绪' },
  { status: 'decomposed', title: '已拆解' },
  { status: 'executing', title: '执行中' },
  { status: 'completed', title: '已完成' },
];

interface KanbanBoardProps {
  stories: Story[];
  onStoryClick?: (story: Story) => void;
}

export function KanbanBoard({ stories, onStoryClick }: KanbanBoardProps) {
  return (
    <div className="flex gap-4 overflow-x-auto pb-4 h-full">
      {COLUMNS.map((col) => {
        const columnStories = stories.filter((s) => s.status === col.status);
        return (
          <div
            key={col.status}
            className="flex-shrink-0 w-72 bg-gray-50 rounded-lg border border-gray-200"
          >
            <div className="px-4 py-3 border-b border-gray-200">
              <div className="flex items-center justify-between">
                <h3 className="font-medium text-gray-700 text-sm">
                  {col.title}
                </h3>
                <span className="text-xs text-gray-400 bg-gray-200 rounded-full px-2 py-0.5">
                  {columnStories.length}
                </span>
              </div>
            </div>
            <div className="p-2 space-y-2 overflow-y-auto max-h-[calc(100vh-16rem)]">
              {columnStories.map((story) => (
                <StoryCard
                  key={story.id}
                  story={story}
                  onClick={() => onStoryClick?.(story)}
                />
              ))}
              {columnStories.length === 0 && (
                <p className="text-center text-xs text-gray-400 py-8">
                  暂无 Story
                </p>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
