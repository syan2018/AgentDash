import type { Story } from '../../types';

const STATUS_LABELS: Record<string, { label: string; color: string }> = {
  created: { label: '已创建', color: 'bg-gray-100 text-gray-700' },
  context_ready: { label: '上下文就绪', color: 'bg-blue-100 text-blue-700' },
  decomposed: { label: '已拆解', color: 'bg-purple-100 text-purple-700' },
  executing: { label: '执行中', color: 'bg-amber-100 text-amber-700' },
  completed: { label: '已完成', color: 'bg-green-100 text-green-700' },
  failed: { label: '失败', color: 'bg-red-100 text-red-700' },
};

interface StoryCardProps {
  story: Story;
  onClick?: () => void;
}

export function StoryCard({ story, onClick }: StoryCardProps) {
  const statusInfo = STATUS_LABELS[story.status] ?? STATUS_LABELS.created;

  return (
    <div
      onClick={onClick}
      className="bg-white rounded-lg border border-gray-200 p-4 hover:shadow-md transition-shadow cursor-pointer"
    >
      <div className="flex items-start justify-between gap-2">
        <h3 className="font-medium text-gray-900 text-sm leading-snug">
          {story.title}
        </h3>
        <span
          className={`shrink-0 px-2 py-0.5 text-xs rounded-full font-medium ${statusInfo.color}`}
        >
          {statusInfo.label}
        </span>
      </div>
      {story.description && (
        <p className="mt-2 text-xs text-gray-500 line-clamp-2">
          {story.description}
        </p>
      )}
      <div className="mt-3 flex items-center justify-between text-xs text-gray-400">
        <span>{new Date(story.created_at).toLocaleDateString('zh-CN')}</span>
        <span className="font-mono">{story.id.slice(0, 8)}</span>
      </div>
    </div>
  );
}
