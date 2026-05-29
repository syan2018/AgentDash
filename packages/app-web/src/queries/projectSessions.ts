/**
 * 项目活跃会话列表 — react-query 承载。
 *
 * 取代原先的 `activeSessionsStore` 与 `sidebarSessionsStore`：两者数据源同为
 * GET /api/projects/{id}/sessions，仅刷新生命周期不同（agent tab 按需 + 选中时
 * 8s 轮询；sidebar 挂载即拉 + 30s 轮询）。react-query 以 projectId 为 key 天然
 * 去重缓存，轮询间隔由各调用方通过 `refetchInterval` 配置，竞态/loading/缓存由
 * query 内建处理，无需再手写 loadedProjectId 比对。
 */

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import type { ProjectSessionEntry } from "../types";
import { fetchProjectSessions } from "../services/session";

export const projectSessionsKey = (projectId: string | null) =>
  ["project-sessions", projectId] as const;

export interface UseProjectSessionsOptions {
  /** 轮询间隔（ms）；不传则不轮询 */
  refetchInterval?: number;
}

export interface UseProjectSessionsResult {
  sessions: ProjectSessionEntry[];
  isLoading: boolean;
  /** 主动刷新当前项目会话列表（如打开会话、turn 结束、companion 事件后） */
  refresh: () => Promise<void>;
}

export function useProjectSessions(
  projectId: string | null,
  options: UseProjectSessionsOptions = {},
): UseProjectSessionsResult {
  const queryClient = useQueryClient();

  const query = useQuery({
    queryKey: projectSessionsKey(projectId),
    queryFn: () => fetchProjectSessions(projectId as string),
    enabled: Boolean(projectId),
    // 切项目时旧数据不再展示：保持空列表占位，由 enabled + key 切换驱动
    placeholderData: undefined,
    refetchInterval: options.refetchInterval,
  });

  const refresh = useCallback(async () => {
    if (!projectId) return;
    await queryClient.invalidateQueries({ queryKey: projectSessionsKey(projectId) });
  }, [projectId, queryClient]);

  return {
    sessions: query.data ?? [],
    isLoading: query.isPending && Boolean(projectId),
    refresh,
  };
}
