import type { StreamEvent } from '../types';
import { buildApiPath } from './origin';

/**
 * 连接 SSE 事件流，支持 Resume（断连重连）
 *
 * EventSource 具备内建自动重连机制，无需手动管理。
 * 当连接断开时浏览器会自动尝试重连。
 */
export function connectEventStream(
  onEvent: (event: StreamEvent) => void,
  onOpen?: () => void,
  onError?: (error: Event) => void,
): EventSource {
  const source = new EventSource(buildApiPath('/events/stream'));

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
export async function fetchEventsSince(sinceId: number) {
  const res = await fetch(buildApiPath(`/events/since/${sinceId}`));
  if (!res.ok) throw new Error(`Resume 失败: HTTP ${res.status}`);
  return res.json();
}
