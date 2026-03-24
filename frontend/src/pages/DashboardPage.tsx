/**
 * DashboardPage — 仪表板页面容器
 *
 * 作为 /dashboard 路由的父级 Outlet 容器。
 * 子路由 /dashboard/agent 和 /dashboard/story 分别渲染 AgentTabView 和 StoryTabView。
 */

import { Outlet } from "react-router-dom";

export function DashboardPage() {
  return (
    <div className="h-full overflow-hidden">
      <Outlet />
    </div>
  );
}
