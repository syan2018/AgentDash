import type { StreamEvent } from '../types';
import { buildApiPath } from './origin';
import { getStoredToken, authenticatedFetch } from './client';

/**
 * 连接 SSE 事件流，支持 Resume（断连重连）
 *
 * EventSource 具备内建自动重连机制，无需手动管理。
 * 当连接断开时浏览器会自动尝试重连。
 */
export function connectEventStream(
  projectId: string,
  onEvent: (event: StreamEvent) => void,
  onOpen?: () => void,
  onError?: (error: Event) => void,
): EventSource {
  const params = new URLSearchParams({ project_id: projectId });
  const token = getStoredToken();
  if (token) params.set("token", token);
  const source = new EventSource(buildApiPath(`/events/stream?${params.toString()}`));

  source.onopen = () => {
    onOpen?.();
  };

  source.onmessage = (e) => {
    try {
      const event: StreamEvent = JSON.parse(e.data);
      onEvent(event);
    } catch {
      console.warn('无法解析事件流数据:', e.data);
    }
  };

  source.onerror = (e) => {
    onError?.(e);
  };

  return source;
}

/**
 * 手动获取 since_id 之后的变更（用于 Resume 场景）
 */
export async function fetchEventsSince(projectId: string, sinceId: number) {
  const params = new URLSearchParams({ project_id: projectId });
  const res = await authenticatedFetch(buildApiPath(`/events/since/${sinceId}?${params.toString()}`));
  if (!res.ok) throw new Error(`Resume 失败: HTTP ${res.status}`);
  return res.json();
}
