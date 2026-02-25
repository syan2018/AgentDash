import { useEffect } from 'react';
import { useCoordinatorStore } from '../stores/coordinatorStore';
import { useStoryStore } from '../stores/storyStore';
import { KanbanBoard } from '../components/kanban/KanbanBoard';

export function DashboardPage() {
  const { currentBackendId } = useCoordinatorStore();
  const { stories, isLoading, fetchStories } = useStoryStore();

  useEffect(() => {
    if (currentBackendId) {
      fetchStories(currentBackendId);
    }
  }, [currentBackendId, fetchStories]);

  if (!currentBackendId) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-gray-600">欢迎使用 AgentDashboard</h2>
          <p className="mt-2 text-gray-400">
            请在左侧添加或选择一个后端连接开始工作
          </p>
        </div>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-gray-400">加载中...</div>
      </div>
    );
  }

  return (
    <div className="h-full">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold text-gray-800">Story 看板</h2>
        <button className="px-4 py-2 bg-primary hover:bg-primary-hover text-white text-sm rounded-md transition-colors">
          + 新建 Story
        </button>
      </div>
      <KanbanBoard stories={stories} />
    </div>
  );
}
